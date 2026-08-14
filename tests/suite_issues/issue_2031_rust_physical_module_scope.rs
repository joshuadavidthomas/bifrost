//! Issue #2031: Rust current-module lookup follows physical syntax scope,
//! independent of the logical owner of an `impl` or a top-level static.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_after(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    anchor: &str,
    token: &str,
) -> Value {
    let anchor_start = source
        .find(anchor)
        .unwrap_or_else(|| panic!("missing anchor {anchor:?}"));
    let offset = anchor_start
        + source[anchor_start..]
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} after {anchor:?}"));
    let prefix = &source[..offset];
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

fn assert_definition(result: &Value, expected_fqn: &str, expected_path: &str) {
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(
        result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{result:#}"
    );
    assert_eq!(result["definitions"][0]["fqn"], expected_fqn, "{result:#}");
    assert_eq!(
        result["definitions"][0]["path"], expected_path,
        "{result:#}"
    );
}

#[test]
fn foreign_type_impl_uses_physical_file_module_for_bare_names() {
    let child = r#"use crate::owner;

trait LocalTrait {
    fn exercise();
}

fn helper() {}
struct Here;
static TOKEN: &str = "child";

impl LocalTrait for owner::External {
    fn exercise() {
        helper();
        let _ = Here;
        let _ = TOKEN;
    }
}

fn free_controls() {
    helper();
    let _ = Here;
    let _ = TOKEN;
}

mod nested {
    pub fn helper() {}
    pub struct Here;
    pub static TOKEN: &str = "nested";
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2031\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", "mod owner;\nmod child;\n")
        .file(
            "src/owner.rs",
            r#"pub struct External;
pub fn helper() {}
pub struct Here;
pub static TOKEN: &str = "owner";
"#,
        )
        .file("src/child.rs", child)
        .build();

    for (anchor, token, expected_fqn) in [
        ("        helper();", "helper", "issue_2031.child.helper"),
        ("        let _ = Here;", "Here", "issue_2031.child.Here"),
        (
            "        let _ = TOKEN;",
            "TOKEN",
            "issue_2031.child._module_.TOKEN",
        ),
        (
            "fn free_controls() {\n    helper();",
            "helper",
            "issue_2031.child.helper",
        ),
        (
            "fn free_controls() {\n    helper();\n    let _ = Here;",
            "Here",
            "issue_2031.child.Here",
        ),
        (
            "fn free_controls() {\n    helper();\n    let _ = Here;\n    let _ = TOKEN;",
            "TOKEN",
            "issue_2031.child._module_.TOKEN",
        ),
    ] {
        let result = definition_after(&project, "src/child.rs", child, anchor, token);
        assert_definition(&result, expected_fqn, "src/child.rs");
    }
}
