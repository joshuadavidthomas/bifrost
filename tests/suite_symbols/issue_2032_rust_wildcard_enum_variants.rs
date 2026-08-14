//! Issue #2032: imported Rust enum variants outrank the conservative pattern-binder fallback.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn lookup_at(source: &str, root: &std::path::Path, needle: &str) -> Value {
    let start = source
        .find(needle)
        .unwrap_or_else(|| panic!("missing `{needle}`"));
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current)| current)
        .chars()
        .count()
        + 1;
    call_search_tool_json(
        root,
        "get_definitions_by_location",
        &json!({"references": [{"path": "lib.rs", "line": line, "column": column}]}).to_string(),
    )
}

fn assert_variant(value: &Value, expected: &str) {
    let result = &value["results"][0];
    assert_eq!(result["status"], "resolved", "{value:#}");
    assert_eq!(
        result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{value:#}"
    );
    assert_eq!(result["definitions"][0]["fqn"], expected, "{value:#}");
}

#[test]
fn imported_enum_variants_resolve_before_true_pattern_binders() {
    let source = r#"
enum Token {
    Unit,
    Tuple(u8),
    Explicit,
}

use Token::*;
use Token::Explicit as Renamed;

fn read(token: Token) -> u8 {
    match token {
        Unit => 1,
        Tuple(value) => value,
        Renamed => 2,
    }
}

fn read_scoped(token: Token) -> u8 {
    use self::Token::*;
    match token {
        Unit => 3,
        Tuple(value) => value,
        Explicit => 4,
    }
}

fn bind(pair: (u8, u8)) -> u8 {
    match pair {
        (local, _) => local,
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file("lib.rs", source)
        .build();

    assert_variant(&lookup_at(source, project.root(), "Unit =>"), "Token.Unit");
    assert_variant(
        &lookup_at(source, project.root(), "Tuple(value)"),
        "Token.Tuple",
    );
    assert_variant(
        &lookup_at(source, project.root(), "Renamed =>"),
        "Token.Explicit",
    );
    assert_variant(
        &lookup_at(source, project.root(), "Unit => 3"),
        "Token.Unit",
    );

    let local = lookup_at(source, project.root(), "local, _");
    let result = &local["results"][0];
    assert_eq!(result["status"], "no_definition", "{local:#}");
    assert_eq!(
        result["diagnostics"][0]["kind"], "local_binding",
        "{local:#}"
    );
}
