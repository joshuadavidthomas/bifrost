//! Issue #2086: Java inverse references recovered beneath a parser ERROR node
//! remain precision-backed without entering the forward census probe frontier.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ExactReferenceSite, ProbeSeed, ReferenceClassification, ReferenceDifferentialConfig,
    run_reference_differential,
};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{CodeUnitIndex, JavaAnalyzer, Language};

#[test]
fn recovered_java_type_reference_is_backed_only_for_inverse_precision() {
    let source = concat!(
        "@interface Nullable {}\n",
        "class Use {\n",
        "  void trigger(SslSessionTicketKey[] ticketKeys) {}\n",
        "  void recovered(SslSessionTicketKey @Nullable ... keys) {}\n",
        "}\n",
    );
    let project = InlineTestProject::with_language(Language::Java)
        .file("SslSessionTicketKey.java", "class SslSessionTicketKey {}\n")
        .file("Use.java", source)
        .build();
    let analyzer = JavaAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("SslSessionTicketKey")
        .into_iter()
        .next()
        .expect("ticket-key declaration");
    let trigger = source
        .find("SslSessionTicketKey[]")
        .expect("intact trigger reference");
    let recovered = source
        .find("SslSessionTicketKey @Nullable ...")
        .expect("recovered inverse reference");

    let usages = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .into_either()
        .expect("complete Java type usages");
    assert!(
        usages.iter().any(|hit| {
            hit.file == project.file("Use.java")
                && hit.start_offset == recovered
                && hit.end_offset == recovered + "SslSessionTicketKey".len()
        }),
        "fixture must preserve the valid recovered inverse hit: {usages:#?}"
    );

    let report = run_reference_differential(
        &analyzer,
        &ReferenceDifferentialConfig {
            corpus_language: "java".to_string(),
            max_files: 20,
            max_sites: 1_000,
            max_candidates_per_file: 1_000,
            max_source_bytes: 100_000,
            max_targets: 1_000,
            max_usage_files: 20,
            max_usages: 1_000,
            probe_seed: ProbeSeed::Census,
            exact_site: Some(ExactReferenceSite {
                path: "Use.java".to_string(),
                start_byte: trigger,
                end_byte: Some(trigger + "SslSessionTicketKey".len()),
            }),
            ..ReferenceDifferentialConfig::default()
        },
    )
    .expect("run exact Java differential");

    assert_eq!(report.sites.len(), 1, "{report:#?}");
    assert_eq!(
        report.sites[0].classification,
        ReferenceClassification::Consistent,
        "{report:#?}"
    );
    assert!(
        report.inverse_precision_findings.is_empty(),
        "the recovered type reference is membership-backed: {report:#?}"
    );
}
