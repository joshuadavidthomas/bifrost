//! Issue #2051: postponed Python annotations use annotation scope rather than
//! runtime module-binding activation.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
    project: &BuiltInlineTestProject,
    source: &str,
    anchor: &str,
    token: &str,
) -> Value {
    let anchor_start = source.find(anchor).expect("reference anchor");
    let start = anchor_start + anchor.find(token).expect("focused token");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": "scope.py", "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn definition_names(result: &Value) -> Vec<&str> {
    result["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| {
            definition["fqName"]
                .as_str()
                .or(definition["fq_name"].as_str())
                .or(definition["fqn"].as_str())
        })
        .collect()
}

#[test]
fn postponed_annotations_resolve_later_quoted_and_nested_types() {
    let source = r#"from __future__ import annotations
from typing import Literal
from dep import Imported

pages: dict[str, tuple[Page | None, Page | None]]
quoted: "Page"
imported: Imported
runtime_before = Page

class Outer:
    class Inner:
        pass

    def method(self, value: Inner) -> "Outer.Inner":
        return value

class Page:
    pass

literal: Literal["Page"]
external: "Missing.External"
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("dep.py", "class Imported:\n    pass\n")
        .file("scope.py", source)
        .build();

    for (anchor, token, expected) in [
        ("tuple[Page | None", "Page", "scope.Page"),
        ("quoted: \"Page\"", "Page", "scope.Page"),
        ("imported: Imported", "Imported", "dep.Imported"),
        ("value: Inner", "Inner", "scope.Outer$Inner"),
        ("\"Outer.Inner\"", "Inner", "scope.Outer$Inner"),
    ] {
        let result = definition_at(&project, source, anchor, token);
        assert_eq!(result["status"], "resolved", "{anchor}: {result:#}");
        assert_eq!(
            definition_names(&result),
            vec![expected],
            "{anchor}: {result:#}"
        );
    }

    let runtime = definition_at(&project, source, "runtime_before = Page", "Page");
    assert_ne!(runtime["status"], "resolved", "{runtime:#}");
    assert!(definition_names(&runtime).is_empty(), "{runtime:#}");

    let literal = definition_at(&project, source, "Literal[\"Page\"]", "Page");
    assert_ne!(literal["status"], "resolved", "{literal:#}");
    assert!(definition_names(&literal).is_empty(), "{literal:#}");

    let external = definition_at(&project, source, "\"Missing.External\"", "External");
    assert_ne!(external["status"], "resolved", "{external:#}");
    assert!(definition_names(&external).is_empty(), "{external:#}");
}
