//! Issue #1950: a focused inner call used as another call's callee must not
//! resolve as the outer invocation.

use crate::common::{InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
    project: &crate::common::BuiltInlineTestProject,
    path: &str,
    source: &str,
    needle: &str,
) -> Value {
    let start = source.find(needle).expect("reference marker");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]}).to_string();
    call_tool(project, "get_definitions_by_location", &args)["results"][0].clone()
}

#[test]
fn focused_inner_macro_call_does_not_climb_to_outer_invocation() {
    let source = r#"#define SELECT(value) (value)

void consume(void (*callback)(int)) {
    SELECT(callback)(1);
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("nested.c", source)
        .build();

    let result = definition_at(&project, "nested.c", source, "SELECT(callback)");
    assert_eq!(
        result["status"], "resolved",
        "the focused inner macro call must resolve independently: {result:#}"
    );
    assert_eq!(
        result["definitions"][0]["fqn"], "SELECT",
        "the same-file macro is the focused target: {result:#}"
    );
}

#[test]
fn focused_inner_function_call_does_not_climb_to_outer_invocation() {
    let source = r#"typedef void (*callback_t)(int);

callback_t select_callback(void) {
    return 0;
}

void consume(void) {
    select_callback()(1);
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("nested.c", source)
        .build();

    let result = definition_at(&project, "nested.c", source, "select_callback()(");
    assert_eq!(
        result["status"], "resolved",
        "the focused inner function call must resolve independently: {result:#}"
    );
    assert_eq!(result["definitions"][0]["fqn"], "select_callback");
}
