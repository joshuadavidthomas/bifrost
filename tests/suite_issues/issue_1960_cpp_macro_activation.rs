//! Issue #1960: forward macro lookup must use the binding active at the
//! reference byte.

use crate::common::{InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
    project: &crate::common::BuiltInlineTestProject,
    source: &str,
    marker: &str,
) -> Value {
    let start = source.find(marker).expect("reference marker");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args =
        json!({"references": [{"path": "macro.c", "line": line, "column": column}]}).to_string();
    call_tool(project, "get_definitions_by_location", &args)["results"][0].clone()
}

#[test]
fn cpp_macro_lookup_obeys_definition_order_and_undef() {
    let source = r#"void before_definition(void) {
    FUTURE_MACRO(1);
}

#define FUTURE_MACRO(value) (value)
#define ACTIVE_MACRO(value) (value)

void while_active(void) {
    ACTIVE_MACRO(2);
}

#undef ACTIVE_MACRO

void after_undef(void) {
    ACTIVE_MACRO(3);
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("macro.c", source)
        .build();

    let future = definition_at(&project, source, "FUTURE_MACRO(1)");
    assert_eq!(future["status"], "no_definition", "{future:#}");

    let active = definition_at(&project, source, "ACTIVE_MACRO(2)");
    assert_eq!(active["status"], "resolved", "{active:#}");
    assert_eq!(active["definitions"][0]["fqn"], "ACTIVE_MACRO");

    let undefined = definition_at(&project, source, "ACTIVE_MACRO(3)");
    assert_eq!(undefined["status"], "no_definition", "{undefined:#}");
}
