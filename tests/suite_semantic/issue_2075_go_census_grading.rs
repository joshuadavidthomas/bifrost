//! Issue #2075: Go census grading distinguishes adjudicated language answers
//! and unavailable owners from indexed-definition gaps.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceClassification, ReferenceDifferentialConfig, ReferenceDifferentialReport,
    ReferenceDifferentialSite, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

fn site_at(report: &ReferenceDifferentialReport, start_byte: usize) -> &ReferenceDifferentialSite {
    report
        .sites
        .iter()
        .find(|site| site.start_byte == start_byte)
        .unwrap_or_else(|| panic!("census did not probe byte {start_byte}: {report:#?}"))
}

#[test]
fn go_census_grades_only_sites_with_structured_definition_evidence() {
    let source = r#"package sample

type Collisions struct {
    value int
    len   int
    error int
    Field int
}

type Known struct {
    Other int
}

func max(value int) int {
    return value
}

func use(value int, values []int) int {
    _ = value
    var problem error
    _ = missing.Config{Field: 1}
    _ = Known{Field: 2}
    return len(values) + max(1)
}
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("sample.go", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "go".to_string(),
            max_files: 10,
            max_sites: 1_000,
            max_candidates_per_file: 1_000,
            max_source_bytes: 10_000,
            max_targets: 1_000,
            max_usage_files: 10,
            max_usages: 1_000,
            probe_seed: ProbeSeed::Census,
            ..ReferenceDifferentialConfig::default()
        },
    )
    .expect("run Go census differential");

    let local = site_at(
        &report,
        source.find("_ = value").expect("local use") + "_ = ".len(),
    );
    assert_eq!(
        local.forward_status, "resolved",
        "local binding: {local:#?}"
    );
    assert!(local.targets.is_empty(), "local binding: {local:#?}");
    assert_eq!(local.tier, None, "adjudicated local: {local:#?}");
    assert_eq!(local.classification, ReferenceClassification::Inconclusive);

    for (label, start) in [
        (
            "error",
            source.find("problem error").expect("predeclared error") + "problem ".len(),
        ),
        ("len", source.rfind("len(values)").expect("predeclared len")),
    ] {
        let site = site_at(&report, start);
        assert_eq!(
            site.diagnostics
                .first()
                .map(|diagnostic| diagnostic.kind.as_str()),
            Some("predeclared_symbol_reference"),
            "predeclared {label}: {site:#?}"
        );
        assert_eq!(site.tier, None, "adjudicated {label}: {site:#?}");
        assert_eq!(site.classification, ReferenceClassification::Inconclusive);
    }

    let unknown_field = site_at(
        &report,
        source.find("Config{Field").expect("unknown-owner field") + "Config{".len(),
    );
    assert_eq!(
        unknown_field
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.kind.as_str()),
        Some("go_literal_owner_unresolved")
    );
    assert_eq!(
        unknown_field.tier,
        Some(3),
        "unknown owner: {unknown_field:#?}"
    );
    assert_eq!(
        unknown_field.classification,
        ReferenceClassification::Inconclusive
    );

    let known_field = site_at(
        &report,
        source.find("Known{Field").expect("known-owner field") + "Known{".len(),
    );
    assert_eq!(
        known_field
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.kind.as_str()),
        Some("no_indexed_definition"),
        "known owner: {known_field:#?}"
    );
    assert_eq!(known_field.tier, Some(2), "known owner: {known_field:#?}");
    assert_eq!(known_field.classification, ReferenceClassification::Missing);

    let shadowing_declaration = site_at(&report, source.rfind("max(1)").expect("package max"));
    assert_eq!(
        shadowing_declaration.forward_status, "resolved",
        "a package declaration must shadow the predeclared name: {shadowing_declaration:#?}"
    );
    assert!(
        !shadowing_declaration.targets.is_empty(),
        "package max target: {shadowing_declaration:#?}"
    );
}
