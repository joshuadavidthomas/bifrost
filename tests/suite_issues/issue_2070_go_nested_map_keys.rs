//! Issue #2070: keys in an elided nested map literal remain ordinary value
//! references instead of being consumed as unresolved struct field labels.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    occurrence: &str,
) -> Value {
    let start = source.find(occurrence).expect("occurrence");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

#[test]
fn nested_map_keys_are_value_references_while_struct_labels_keep_their_owner() {
    let source = r#"package app

const NestedKey = "nested"

type Item struct { NestedKey string }

var maps = map[string]map[string]string{
    "outer": {NestedKey: "value"},
}
var items = map[string]Item{
    "outer": {NestedKey: "field"},
}
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("model.go", source)
        .build();

    let map_key = definition_at(&project, "model.go", source, "NestedKey: \"value\"");
    assert_eq!(map_key["status"], "resolved", "{map_key:#}");
    assert_eq!(
        map_key["definitions"][0]["fqn"], "example.com/app._module_.NestedKey",
        "{map_key:#}"
    );

    let field = definition_at(&project, "model.go", source, "NestedKey: \"field\"");
    assert_eq!(field["status"], "resolved", "{field:#}");
    assert_eq!(
        field["definitions"][0]["fqn"], "example.com/app.Item.NestedKey",
        "{field:#}"
    );
}
