//! Issue #2037: shorthand destructuring reads belong to inverse membership,
//! while the simultaneous binder remains outside the census probe frontier.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

#[test]
fn typescript_shorthand_destructuring_backs_inverse_without_becoming_a_probe() {
    let source = r#"interface Options {
    halfWidth: number;
}
function use(options: Options) {
    const { halfWidth } = options;
    return options.halfWidth + halfWidth;
}
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("index.ts", source)
        .build();
    let shorthand = source.find("{ halfWidth }").expect("shorthand pattern") + "{ ".len();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "ts".to_string(),
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
    .expect("run TypeScript census differential");

    assert!(
        report.sites.iter().all(|site| site.start_byte != shorthand),
        "the shorthand binder must not become a forward probe: {report:#?}"
    );
    assert!(
        report.inverse_precision_findings.is_empty(),
        "the shorthand property read must back the inverse hit: {report:#?}"
    );
    assert_eq!(report.summary.inverse_precision_unbacked_hits, 0);
}
