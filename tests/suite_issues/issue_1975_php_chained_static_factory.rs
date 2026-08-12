//! Issue #1975: carry a PHP static factory return type into a chained call.

use crate::common::{InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::json;

#[test]
fn static_factory_return_type_resolves_the_chained_interface_method() {
    let source = r#"<?php
namespace App;
interface Hub {
    public function getLastEventId(): ?string;
}
final class Sdk {
    public static function getCurrentHub(): Hub { throw new \RuntimeException(); }
}
final class Adapter {
    public function current(): ?string {
        return Sdk::getCurrentHub()->getLastEventId();
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Hub.php", source)
        .build();
    let line = source
        .lines()
        .position(|line| line.contains("getLastEventId();"))
        .expect("chained call line")
        + 1;
    let source_line = source.lines().nth(line - 1).expect("source line");
    let column = source_line.find("getLastEventId").expect("chained method") + 1;
    let args = json!({
        "references": [{"path": "src/Hub.php", "line": line, "column": column}]
    })
    .to_string();
    let result = &call_tool(&project, "get_definitions_by_location", &args)["results"][0];

    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(
        result["definitions"][0]["fqn"], "App.Hub.getLastEventId",
        "{result:#}"
    );
}
