//! Issue #2052: Python class bodies see their own earlier bindings without
//! leaking those bindings into module-level lookup.

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
    let args = json!({"references": [{"path": "scope.py", "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn fqns(result: &Value) -> BTreeSet<&str> {
    result["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|definition| definition["fqn"].as_str())
        .collect()
}

#[test]
fn class_body_lookup_prefers_the_latest_earlier_class_binding() {
    let source = r#"def action(value):
    return value

class Handler:
    @overload
    def action(self, value): ...

    def action(self, value):
        return value

    alias = decorate(action)

    PROCESSES = ["start", "stop"]
    schema = {"enum": PROCESSES}
    PROCESSES = set(PROCESSES)

class Other:
    def action(self, value):
        return value

module_use = action(1)
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("scope.py", source)
        .build();

    let method = definition_at(&project, source, "alias = decorate(action)", "action");
    assert_eq!(method["status"], "resolved", "{method:#}");
    assert_eq!(fqns(&method), BTreeSet::from(["scope.Handler.action"]));
    assert_eq!(method["definitions"][0]["kind"], "function");

    let field = definition_at(&project, source, "{\"enum\": PROCESSES}", "PROCESSES");
    assert_eq!(field["status"], "resolved", "{field:#}");
    assert_eq!(fqns(&field), BTreeSet::from(["scope.Handler.PROCESSES"]));

    let rebinding_rhs = definition_at(&project, source, "set(PROCESSES)", "PROCESSES");
    assert_eq!(rebinding_rhs["status"], "resolved", "{rebinding_rhs:#}");
    assert_eq!(
        fqns(&rebinding_rhs),
        BTreeSet::from(["scope.Handler.PROCESSES"])
    );

    let module_use = definition_at(&project, source, "module_use = action(1)", "action");
    assert_eq!(module_use["status"], "resolved", "{module_use:#}");
    assert_eq!(fqns(&module_use), BTreeSet::from(["scope.action"]));
}

#[test]
fn conditional_module_bindings_remain_ambiguous_in_a_class_body() {
    let source = r#"if AVAILABLE:
    from package import schema
else:
    schema = None

class Loader:
    enabled = bool(schema)
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("scope.py", source)
        .file("package/__init__.py", "")
        .file("package/schema.py", "class Validator:\n    pass\n")
        .build();

    let result = definition_at(&project, source, "bool(schema)", "schema");
    assert_eq!(result["status"], "ambiguous", "{result:#}");
    assert_eq!(
        fqns(&result),
        BTreeSet::from(["package.schema", "scope.schema"]),
        "{result:#}"
    );
}
