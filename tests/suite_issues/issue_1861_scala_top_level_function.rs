//! Issue #1861: a bare call in a Scala 3 source file did not resolve to a
//! later top-level function in the same unnamed package.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::json;

#[test]
fn scala_unnamed_package_call_resolves_later_top_level_function() {
    let source = r#"def run(): String = template("value")

object Nested {
  def template(value: String): String = "wrong"
}

class Base {
  def template(value: String): String = "inherited"
}

class Child extends Base {
  def inherited(): String = template("value")
}

def template(value: String): String = value
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("merged_prs.scala", source)
        .build();
    let location = |start: usize| {
        let line = source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column = source[..start]
            .rsplit_once('\n')
            .map_or(&source[..start], |(_, current)| current)
            .chars()
            .count()
            + 1;
        json!({"path": "merged_prs.scala", "line": line, "column": column})
    };
    let root_call = source.find("template(\"value\")").expect("top-level call");
    let inherited_call = source
        .rfind("template(\"value\")")
        .expect("inherited member call");
    let value = call_search_tool_json(
        project.root(),
        "get_definitions_by_location",
        &json!({
            "references": [location(root_call), location(inherited_call)]
        })
        .to_string(),
    );

    let root = &value["results"][0];
    assert_eq!(root["status"], "resolved", "{value}");
    assert_eq!(root["definitions"].as_array().map(Vec::len), Some(1));
    assert_eq!(root["definitions"][0]["path"], "merged_prs.scala");
    assert_eq!(root["definitions"][0]["fqn"], "template");

    let inherited = &value["results"][1];
    assert_eq!(inherited["status"], "resolved", "{value}");
    assert_eq!(inherited["definitions"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        inherited["definitions"][0]["fqn"], "Base.template",
        "a root function must not replace an inherited member: {value}"
    );
}
