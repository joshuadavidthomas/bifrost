//! Issue #2040: bare references in a TypeScript enum initializer bind only to
//! earlier members of that same enum.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{CodeUnitIndex, Language, TypescriptAnalyzer};
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
    let args = json!({"references": [{"path": "flags.ts", "line": line, "column": column}]});
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
fn enum_initializers_resolve_only_earlier_members_of_the_same_enum() {
    let source = r#"enum Other {
  DEPTH = 32,
}

enum BufferBits {
  DEPTH = 1,
  STENCIL = 2,
  DEPTH_STENCIL = DEPTH | STENCIL,
  QUALIFIED = BufferBits.DEPTH,
  FORWARD = LATER,
  LATER = 8,
  LOCAL = (() => { const DEPTH = 16; return DEPTH; })(),
}
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("flags.ts", source)
        .build();

    for (anchor, token, expected) in [
        ("= DEPTH | STENCIL", "DEPTH", "BufferBits.DEPTH"),
        ("= DEPTH | STENCIL", "STENCIL", "BufferBits.STENCIL"),
        ("BufferBits.DEPTH", "DEPTH", "BufferBits.DEPTH"),
    ] {
        let result = definition_at(&project, source, anchor, token);
        assert_eq!(result["status"], "resolved", "{result:#}");
        assert_eq!(fqns(&result), BTreeSet::from([expected]), "{result:#}");
    }

    let forward = definition_at(&project, source, "FORWARD = LATER", "LATER");
    assert_eq!(forward["status"], "no_definition", "{forward:#}");
    assert!(forward["definitions"].as_array().is_none_or(Vec::is_empty));

    let local = definition_at(&project, source, "return DEPTH", "DEPTH");
    assert_eq!(local["status"], "resolved", "{local:#}");
    assert_eq!(
        local["definitions"][0]["kind"], "local_variable",
        "{local:#}"
    );
    assert!(local["definitions"][0].get("fqn").is_none(), "{local:#}");

    let analyzer = TypescriptAnalyzer::from_project(project.project().clone());
    let depth = analyzer
        .get_definitions("BufferBits.DEPTH")
        .into_iter()
        .next()
        .expect("enum member target");
    let depth_use = source
        .find("= DEPTH | STENCIL")
        .expect("bitwise initializer")
        + 2;
    let usages = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&depth))
        .into_either()
        .expect("enum member usages");
    assert!(
        usages.iter().any(|hit| {
            hit.file == project.file("flags.ts")
                && hit.start_offset == depth_use
                && hit.end_offset == depth_use + "DEPTH".len()
        }),
        "the inverse surface must retain the bare enum-member operand: {usages:#?}"
    );
}
