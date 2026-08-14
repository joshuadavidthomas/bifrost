//! Issue #2053: Python module rebinding resolution follows the structured
//! binding timeline without collapsing same-FQN declarations together.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};
use std::collections::BTreeSet;

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
    let args = json!({"references": [{"path": "timeline.py", "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn definition_kinds(result: &Value) -> BTreeSet<&str> {
    result["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| definition["kind"].as_str())
        .collect()
}

#[test]
fn module_rebindings_follow_source_order_and_conditional_arms() {
    let source = r#"def decorate(value):
    return value

def convert(value):
    return value

def deferred(value):
    return convert(value)

convert = decorate(convert)
after = convert(1)

def rank(value):
    return value

other = rank
if ENABLED:
    rank = decorate(other)
else:
    rank = decorate(rank)

if USE_NATIVE:
    utcnow = external_clock
else:
    def utcnow():
        return 0

def later():
    return utcnow()
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("timeline.py", source)
        .build();

    let deferred = definition_at(&project, source, "return convert(value)", "convert");
    assert_eq!(deferred["status"], "resolved", "{deferred:#}");
    assert_eq!(definition_kinds(&deferred), BTreeSet::from(["field"]));

    let assignment_rhs = definition_at(&project, source, "convert = decorate(convert)", "convert)");
    assert_eq!(assignment_rhs["status"], "resolved", "{assignment_rhs:#}");
    assert_eq!(
        definition_kinds(&assignment_rhs),
        BTreeSet::from(["function"])
    );

    let after = definition_at(&project, source, "after = convert(1)", "convert");
    assert_eq!(after["status"], "resolved", "{after:#}");
    assert_eq!(definition_kinds(&after), BTreeSet::from(["field"]));

    let else_rhs = definition_at(&project, source, "rank = decorate(rank)", "rank)");
    assert_eq!(else_rhs["status"], "resolved", "{else_rhs:#}");
    assert_eq!(definition_kinds(&else_rhs), BTreeSet::from(["function"]));

    let conditional = definition_at(&project, source, "return utcnow()", "utcnow");
    assert_eq!(conditional["status"], "ambiguous", "{conditional:#}");
    assert_eq!(
        definition_kinds(&conditional),
        BTreeSet::from(["field", "function"]),
        "{conditional:#}"
    );
}
