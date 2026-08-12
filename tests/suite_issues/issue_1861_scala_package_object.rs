//! Issue #1861: package-object members were not indexed as package members.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::json;

#[test]
fn scala_package_object_member_resolves_from_sibling_file() {
    let package_source = r#"package zio
package object http {
  def handler(value: String): String = value
}
object Outside {
  def marker: String = "outer"
}
"#;
    let route_source = r#"package zio.http
object Route {
  val route = handler("value")
}
"#;
    let outer_source = r#"package zio
object Use {
  val value = Outside.marker
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("zio/http/package.scala", package_source)
        .file("zio/http/Route.scala", route_source)
        .file("zio/Use.scala", outer_source)
        .build();
    let location = |path: &str, source: &str, needle: &str| {
        let start = source.find(needle).expect("reference token");
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
        json!({"path": path, "line": line, "column": column})
    };
    let value = call_search_tool_json(
        project.root(),
        "get_definitions_by_location",
        &json!({
            "references": [
                location("zio/http/Route.scala", route_source, "handler"),
                location("zio/Use.scala", outer_source, "marker")
            ]
        })
        .to_string(),
    );

    let result = &value["results"][0];
    assert_eq!(result["status"], "resolved", "{value}");
    assert_eq!(result["definitions"].as_array().map(Vec::len), Some(1));
    assert_eq!(result["definitions"][0]["path"], "zio/http/package.scala");
    assert_eq!(result["definitions"][0]["fqn"], "zio.http.handler");

    let continuation = &value["results"][1];
    assert_eq!(continuation["status"], "resolved", "{value}");
    assert_eq!(
        continuation["definitions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        continuation["definitions"][0]["fqn"], "zio.Outside$.marker",
        "a declaration after the package object must remain in the outer package: {value}"
    );
}
