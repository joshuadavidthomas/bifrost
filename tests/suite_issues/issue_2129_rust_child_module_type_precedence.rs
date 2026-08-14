//! Issue #2129: bare Rust types in a physical child module beat a same-named
//! declaration in the logical parent owner.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
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
fn bare_child_type_beats_same_named_parent_logical_owner() {
    let child = r#"pub struct Pinned;

impl From<Pinned> for super::Pinned {
    fn from(value: Pinned) -> Self {
        let _ = value;
        Self::Child
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2129\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", "mod source;\nmod sibling;\n")
        .file(
            "src/source/mod.rs",
            "pub mod child;\npub enum Pinned { Child }\n",
        )
        .file("src/source/child.rs", child)
        .file("src/sibling.rs", "pub struct Pinned;\n")
        .build();

    let bare = definition_at(
        &project,
        "src/source/child.rs",
        child,
        "fn from(value: Pinned)",
        "Pinned",
    );
    assert_definition(
        &bare,
        "issue_2129.source.child.Pinned",
        "src/source/child.rs",
    );

    let qualified = definition_at(
        &project,
        "src/source/child.rs",
        child,
        "for super::Pinned",
        "Pinned",
    );
    assert_definition(&qualified, "issue_2129.source.Pinned", "src/source/mod.rs");
}
