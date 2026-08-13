//! Issue #2074: Go interface methods and constants are declarations, not
//! forward-reference probes, even though raw census membership retains them.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

#[test]
fn go_census_excludes_interface_methods_and_constants_from_probes() {
    let source = r#"package sample

type Reader interface {
    Read([]byte) (int, error)
    Close() error
}

const Ready, Waiting = 1, 2

func use(reader Reader) int {
    _, _ = reader.Read(nil)
    return Ready
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

    let declaration_offsets = [
        source.find("Read([]byte)").expect("Read declaration"),
        source.find("Close() error").expect("Close declaration"),
        source.find("Ready, Waiting").expect("Ready declaration"),
        source.find("Waiting =").expect("Waiting declaration"),
    ];
    for declaration in declaration_offsets {
        assert!(
            report
                .sites
                .iter()
                .all(|site| site.start_byte != declaration),
            "Go declaration at {declaration} became a probe: {report:#?}"
        );
    }

    for reference in [
        source.rfind("Reader").expect("parameter type reference"),
        source.rfind("Read").expect("method reference"),
        source.rfind("Ready").expect("constant reference"),
    ] {
        assert!(
            report.sites.iter().any(|site| site.start_byte == reference),
            "Go reference at {reference} was not probed: {report:#?}"
        );
    }
    assert!(
        report.summary.declaration_sites_excluded >= declaration_offsets.len() as u64,
        "structured declaration exclusions were not accounted: {report:#?}"
    );
}
