use crate::common::InlineTestProject;
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, JavascriptAnalyzer, Language};
use std::collections::BTreeSet;
use std::sync::Arc;

fn occurrence_range(source: &str, text: &str, occurrence: usize) -> (usize, usize) {
    let start = source
        .match_indices(text)
        .nth(occurrence)
        .map(|(start, _)| start)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {text:?}"));
    (start, start + text.len())
}

fn authoritative_hits(
    analyzer: &JavascriptAnalyzer,
    target: &CodeUnit,
) -> BTreeSet<brokk_bifrost::usages::UsageHit> {
    let candidate = target.source().clone();
    let provider = ExplicitCandidateProvider::new(Arc::new(std::iter::once(candidate).collect()));
    let query = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            analyzer,
            std::slice::from_ref(target),
            Some(&provider),
            1,
            100,
        );
    match query.result {
        FuzzyResult::Success {
            hits_by_overload, ..
        } => hits_by_overload.get(target).cloned().unwrap_or_default(),
        other => panic!("expected authoritative JavaScript usage success, got {other:#?}"),
    }
}

#[test]
fn javascript_static_private_field_usages_keep_the_exact_class_owner() {
    let source = r#"class CurrentPointers {
  static #pointerType = null;

  static set(pointerType) {
    CurrentPointers.#pointerType = pointerType;
  }

  static matches(pointerType) {
    return CurrentPointers.#pointerType === pointerType;
  }
}

class Decoy {
  static #pointerType = null;

  static matches(pointerType) {
    return Decoy.#pointerType === pointerType;
  }
}
"#;
    let project = InlineTestProject::with_language(Language::JavaScript)
        .file("tools.js", source)
        .build();
    let file = project.file("tools.js");
    let analyzer = JavascriptAnalyzer::from_project(project.project().clone());
    let owner = analyzer
        .all_declarations()
        .find(|unit| unit.source() == &file && unit.fq_name() == "CurrentPointers")
        .expect("CurrentPointers class");
    let children = analyzer.get_direct_children(&owner);
    assert_eq!(
        children
            .iter()
            .filter(|unit| unit.fq_name() == "CurrentPointers.#pointerType")
            .count(),
        1,
        "the class must own one private-field declaration: {children:#?}"
    );
    let target = children
        .iter()
        .find(|unit| unit.fq_name() == "CurrentPointers.#pointerType")
        .cloned()
        .unwrap_or_else(|| panic!("CurrentPointers private field; children: {children:#?}"));
    assert_eq!(Some(owner), analyzer.parent_of(&target));

    let hits = authoritative_hits(&analyzer, &target);
    let ranges: BTreeSet<_> = hits
        .iter()
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect();

    assert_eq!(
        BTreeSet::from([
            occurrence_range(source, "#pointerType", 1),
            occurrence_range(source, "#pointerType", 2),
        ]),
        ranges,
        "same-class static private-field reads and writes must resolve without claiming the decoy: {hits:#?}"
    );
}
