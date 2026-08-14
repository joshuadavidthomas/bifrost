//! Issue #2038: an export alias is an outward declaration, not a local use.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_after(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    after: &str,
    needle: &str,
) -> Value {
    let anchor = source.find(after).expect("anchor");
    let start = anchor + source[anchor..].find(needle).expect("needle after anchor");
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

fn assert_export_alias(result: &Value) {
    assert_eq!(result["status"], "no_definition", "{result:#}");
    assert!(result["definitions"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(
        result["diagnostics"][0]["kind"], "declaration_or_import_site",
        "{result:#}"
    );
}

#[test]
fn local_export_alias_does_not_resolve_a_same_named_declaration() {
    let source = r#"interface Public {}
interface Internal {}
function named() {}
export { Internal as Public, named };
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("index.ts", source)
        .build();

    let alias = definition_after(&project, "index.ts", source, "as Public", "Public");
    assert_export_alias(&alias);

    let exported_value = definition_after(
        &project,
        "index.ts",
        source,
        "export { Internal",
        "Internal",
    );
    assert_eq!(exported_value["status"], "resolved", "{exported_value:#}");
    assert_eq!(
        exported_value["definitions"][0]["fqn"], "Internal",
        "{exported_value:#}"
    );

    let unaliased = definition_after(&project, "index.ts", source, ", named", "named");
    assert_eq!(unaliased["status"], "resolved", "{unaliased:#}");
}

#[test]
fn reexport_alias_does_not_resolve_a_visible_import_binding() {
    let module = "export default function normalize() {}\n";
    let barrel = r#"import normalize from './normalize';
export { default as normalize } from './normalize';
normalize();
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("normalize.ts", module)
        .file("index.ts", barrel)
        .build();

    let alias = definition_after(&project, "index.ts", barrel, "as normalize", "normalize");
    assert_export_alias(&alias);

    let ordinary_use = definition_after(&project, "index.ts", barrel, "normalize();", "normalize");
    assert_eq!(ordinary_use["status"], "resolved", "{ordinary_use:#}");
    assert_eq!(
        ordinary_use["definitions"][0]["path"], "normalize.ts",
        "{ordinary_use:#}"
    );
}
