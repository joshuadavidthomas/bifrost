//! Issue #2033: named fields of Rust enum struct variants are declarations
//! owned by the exact variant and their initializer labels are references.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{CodeUnit, CodeUnitIndex, Language, RustAnalyzer};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    offset: usize,
) -> Value {
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

fn field(analyzer: &RustAnalyzer, fqn: &str) -> CodeUnit {
    analyzer
        .get_definitions(fqn)
        .into_iter()
        .find(CodeUnit::is_field)
        .unwrap_or_else(|| panic!("missing field {fqn}"))
}

fn ranges_for(analyzer: &RustAnalyzer, target: &CodeUnit) -> BTreeSet<(usize, usize)> {
    UsageFinder::new()
        .find_usages_default(analyzer, std::slice::from_ref(target))
        .all_hits()
        .into_iter()
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect()
}

fn occurrence(source: &str, anchor: &str, token: &str) -> usize {
    let anchor_start = source
        .find(anchor)
        .unwrap_or_else(|| panic!("missing anchor {anchor:?}"));
    anchor_start
        + source[anchor_start..]
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} after {anchor:?}"))
}

#[test]
fn enum_struct_variant_fields_keep_exact_variant_identity() {
    let source = r#"pub struct Record { pub ser: usize }

pub enum Compound {
    Map { ser: usize, marker: bool },
    Other { ser: usize },
    Tuple(usize),
}

pub fn build() {
    let _ = Record { ser: 1 };
    let _ = Compound::Map { ser: 2, marker: true };
    let _ = Compound::Other { ser: 3 };

    struct Adapter { writer: usize, formatter: usize }
    let _ = Adapter { writer: 4, formatter: 5 };
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2033\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", source)
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let path = "src/lib.rs";

    let record_site = occurrence(source, "Record { ser: 1", "ser");
    let map_site = occurrence(source, "Compound::Map { ser: 2", "ser");
    let other_site = occurrence(source, "Compound::Other { ser: 3", "ser");
    let local_site = occurrence(source, "Adapter { writer: 4", "writer");

    let record = field(&analyzer, "issue_2033.Record.ser");
    let map = field(&analyzer, "issue_2033.Compound.Map.ser");
    let other = field(&analyzer, "issue_2033.Compound.Other.ser");
    let tuple = field(&analyzer, "issue_2033.Compound.Tuple");
    assert_eq!(
        analyzer.parent_of(&map).map(|parent| parent.fq_name()),
        Some("issue_2033.Compound.Map".to_string())
    );
    assert_eq!(
        analyzer.parent_of(&other).map(|parent| parent.fq_name()),
        Some("issue_2033.Compound.Other".to_string())
    );
    assert!(
        analyzer.direct_children(&tuple).is_empty(),
        "tuple variants must not synthesize named fields"
    );

    for (site, expected) in [
        (record_site, "issue_2033.Record.ser"),
        (map_site, "issue_2033.Compound.Map.ser"),
        (other_site, "issue_2033.Compound.Other.ser"),
    ] {
        let result = definition_at(&project, path, source, site);
        assert_eq!(result["status"], "resolved", "{result:#}");
        assert_eq!(result["definitions"][0]["fqn"], expected, "{result:#}");
    }

    let local = definition_at(&project, path, source, local_site);
    assert_eq!(local["status"], "no_definition", "{local:#}");
    assert_eq!(
        local["diagnostics"][0]["kind"], "unresolved_struct_owner",
        "{local:#}"
    );

    let name_len = "ser".len();
    assert_eq!(
        ranges_for(&analyzer, &record),
        BTreeSet::from([(record_site, record_site + name_len)])
    );
    assert_eq!(
        ranges_for(&analyzer, &map),
        BTreeSet::from([(map_site, map_site + name_len)])
    );
    assert_eq!(
        ranges_for(&analyzer, &other),
        BTreeSet::from([(other_site, other_site + name_len)])
    );
}
