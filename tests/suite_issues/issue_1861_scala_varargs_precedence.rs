//! Issue #1861: fixed Scala `apply` overloads must precede repeated overloads.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::json;

#[test]
fn scala_singleton_apply_prefers_applicable_fixed_over_repeated_overload() {
    let source = r#"package app

final class Path

object TPath {
  def apply(elems: String*): Int = elems.size
  def apply(path: Path): Int = 1
}

object Use {
  val path = new Path
  val typed = TPath(path)
  val constructed = TPath(new Path)
  val literal = TPath("segment")
  val expanded = TPath("first", "second")
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("app/Use.scala", source)
        .build();
    let location = |needle: &str| {
        let start = source.find(needle).expect("call site") + needle.find("TPath").unwrap();
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
        json!({"path": "app/Use.scala", "line": line, "column": column})
    };
    let value = call_search_tool_json(
        project.root(),
        "get_definitions_by_location",
        &json!({
            "references": [
                location("typed = TPath"),
                location("constructed = TPath"),
                location("literal = TPath"),
                location("expanded = TPath")
            ]
        })
        .to_string(),
    );

    let results = value["results"].as_array().expect("definition results");
    for result in results {
        assert_eq!(result["status"], "resolved", "{value}");
        assert_eq!(result["definitions"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["definitions"][0]["fqn"], "app.TPath$.apply");
    }
    for result in &results[..2] {
        assert_eq!(result["definitions"][0]["signature"], "(Path)", "{value}");
    }
    for result in &results[2..] {
        assert_eq!(
            result["definitions"][0]["signature"], "(String*)",
            "{value}"
        );
    }
}
