//! Issues #2025 and #2068: Go bindings begin only after their declaration's
//! language-defined activation point.

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
    let search_start = anchor + after.len();
    let start = search_start
        + source[search_start..]
            .find(needle)
            .expect("needle after anchor");
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

fn assert_type(result: &Value, fqn: &str) {
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(result["definitions"][0]["fqn"], fqn, "{result:#}");
}

#[test]
fn short_var_and_var_names_do_not_hide_their_own_initializers() {
    let source = r#"package app

type spec struct { field int }

func short() {
    spec := spec{}
    _ = spec.field
    _ = spec
}

func ordinaryVar() {
    var spec = spec{}
    _ = spec.field
}

func groupedVar() {
    var (
        spec = spec{}
        later = spec{}
    )
    _ = later
}
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("main.go", source)
        .build();

    assert_type(
        &definition_after(&project, "main.go", source, "spec := ", "spec"),
        "example.com/app.spec",
    );
    assert_type(
        &definition_after(&project, "main.go", source, "var spec = ", "spec"),
        "example.com/app.spec",
    );
    assert_type(
        &definition_after(&project, "main.go", source, "        spec = ", "spec"),
        "example.com/app.spec",
    );

    let later_initializer = definition_after(&project, "main.go", source, "later = ", "spec");
    assert_ne!(
        later_initializer["definitions"][0]["fqn"], "example.com/app.spec",
        "an earlier grouped VarSpec must be visible to the later initializer: {later_initializer:#}"
    );

    let later_use = definition_after(
        &project,
        "main.go",
        source,
        "    _ = spec.field\n    _ = ",
        "spec",
    );
    assert_eq!(later_use["status"], "resolved", "{later_use:#}");
    assert_eq!(
        later_use["definitions"][0]["kind"], "local_variable",
        "{later_use:#}"
    );
}

#[test]
fn parameter_names_begin_in_the_callable_body_not_its_signature() {
    let source = r#"package app

import "example.com/app/dep"

type kind struct{}
type auth struct{}

func same(kind kind) { _ = kind }
func pointer(auth *auth) { _ = auth }
func grouped(kind, other kind) {}
func earlier(kind int, other kind) {}
func qualified(dep *dep.Auth) { _ = dep }
func (auth *auth) method() { _ = auth }
type API interface { Use(kind kind) }

func outer() {
    kind := kind{}
    _ = func(value kind) { _ = value }
}
"#;
    let dependency = "package dep\n\ntype Auth struct{}\n";
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("main.go", source)
        .file("dep/auth.go", dependency)
        .build();

    for (after, expected) in [
        ("func same(kind ", "example.com/app.kind"),
        ("func pointer(auth *", "example.com/app.auth"),
        ("func grouped(kind, other ", "example.com/app.kind"),
        ("func earlier(kind int, other ", "example.com/app.kind"),
        ("func (auth *", "example.com/app.auth"),
        ("Use(kind ", "example.com/app.kind"),
    ] {
        let needle = expected.rsplit('.').next().expect("type terminal");
        assert_type(
            &definition_after(&project, "main.go", source, after, needle),
            expected,
        );
    }

    assert_type(
        &definition_after(
            &project,
            "main.go",
            source,
            "func qualified(dep *dep.",
            "Auth",
        ),
        "example.com/app/dep.Auth",
    );

    let body_use = definition_after(
        &project,
        "main.go",
        source,
        "func same(kind kind) { _ = ",
        "kind",
    );
    assert_eq!(body_use["status"], "resolved", "{body_use:#}");
    assert_eq!(
        body_use["definitions"][0]["kind"], "parameter",
        "{body_use:#}"
    );

    let outer_shadow = definition_after(&project, "main.go", source, "func(value ", "kind");
    assert_ne!(
        outer_shadow["definitions"][0]["fqn"], "example.com/app.kind",
        "an already-active outer local still shadows a nested literal signature: {outer_shadow:#}"
    );
}
