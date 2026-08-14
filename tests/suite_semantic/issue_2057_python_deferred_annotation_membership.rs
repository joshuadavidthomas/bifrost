//! Issue #2057: structured identifiers inside deferred Python annotations
//! back inverse hits without becoming ordinary string probes.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

#[test]
fn deferred_annotation_identifiers_back_inverse_precision_only() {
    let source = r#"from typing import Literal

class Widget:
    pass

class Gadget:
    pass

def trigger(widget: Widget, gadget: Gadget) -> Widget:
    return widget

def deferred(widget: "Widget | list[Gadget]") -> "Widget":
    return widget

def ordinary() -> str:
    return "Widget"

def literal(value: Literal["Widget"]) -> None:
    pass
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("models.py", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "py".to_string(),
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
    .expect("run Python census differential");

    let deferred_offsets = [
        source.find("Widget | list").expect("deferred Widget"),
        source.find("Gadget]").expect("deferred Gadget"),
        source
            .find("-> \"Widget\"")
            .expect("deferred return Widget")
            + "-> \"".len(),
    ];
    for offset in deferred_offsets {
        assert!(
            report.sites.iter().all(|site| site.start_byte != offset),
            "annotation strings must not become probes: {report:#?}"
        );
    }
    assert!(
        report.inverse_precision_findings.is_empty(),
        "structured deferred annotations must back inverse hits: {report:#?}"
    );
    assert_eq!(report.summary.inverse_precision_unbacked_hits, 0);
}
