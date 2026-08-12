//! Issue #1818: a recursive C call inside a definition-only function was
//! hidden because the function body was also the target declaration range.

use crate::common::InlineTestProject;
use brokk_bifrost::usages::{ExplicitCandidateProvider, UsageFinder, UsageHitKind};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, CodeUnitType, CppAnalyzer, Language, ProjectFile};
use std::sync::Arc;

fn function(analyzer: &CppAnalyzer, name: &str) -> CodeUnit {
    analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.kind() == CodeUnitType::Function && unit.identifier() == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

fn token_range(source: &str, line: &str, token: &str) -> (usize, usize) {
    let line_start = source
        .find(line)
        .unwrap_or_else(|| panic!("missing fixture line {line:?}"));
    let token_start = line
        .find(token)
        .unwrap_or_else(|| panic!("missing token {token:?} in {line:?}"));
    let start = line_start + token_start;
    (start, start + token.len())
}

fn assert_recursive_contract(
    analyzer: &CppAnalyzer,
    target: &CodeUnit,
    candidate: &ProjectFile,
    recursive_range: (usize, usize),
    external_range: (usize, usize),
) {
    let provider =
        ExplicitCandidateProvider::new(Arc::new(std::iter::once(candidate.clone()).collect()));
    let result = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            analyzer,
            std::slice::from_ref(target),
            Some(&provider),
            1,
            1000,
        )
        .result;

    let editor_hits = result.all_hits_including_imports();
    assert!(
        editor_hits.iter().any(|hit| {
            (hit.start_offset, hit.end_offset) == recursive_range
                && hit.kind == UsageHitKind::SelfReceiver
        }),
        "the recursive call must be an editor self-reference: {editor_hits:#?}"
    );
    assert!(
        result
            .all_hits()
            .iter()
            .all(|hit| (hit.start_offset, hit.end_offset) != recursive_range),
        "the recursive call must not be an external usage: {result:#?}"
    );
    assert!(
        result.all_hits().iter().any(|hit| {
            (hit.start_offset, hit.end_offset) == external_range
                && hit.kind == UsageHitKind::Reference
        }),
        "the separate caller must remain an external usage: {result:#?}"
    );
}

#[test]
fn recursive_c_calls_are_editor_visible_with_and_without_a_prototype() {
    let source = r#"int definition_only(int value) {
    return value > 0 ? definition_only(value - 1) : 0;
}

int declared_first(int value);
int declared_first(int value) {
    return value > 0 ? declared_first(value - 1) : 0;
}

int caller(int value) {
    return definition_only(value) + declared_first(value);
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("recursive.c", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let candidate = project.file("recursive.c");

    assert_recursive_contract(
        &analyzer,
        &function(&analyzer, "definition_only"),
        &candidate,
        token_range(
            source,
            "    return value > 0 ? definition_only(value - 1) : 0;",
            "definition_only",
        ),
        token_range(
            source,
            "    return definition_only(value) + declared_first(value);",
            "definition_only",
        ),
    );
    assert_recursive_contract(
        &analyzer,
        &function(&analyzer, "declared_first"),
        &candidate,
        token_range(
            source,
            "    return value > 0 ? declared_first(value - 1) : 0;",
            "declared_first",
        ),
        token_range(
            source,
            "    return definition_only(value) + declared_first(value);",
            "declared_first",
        ),
    );
}
