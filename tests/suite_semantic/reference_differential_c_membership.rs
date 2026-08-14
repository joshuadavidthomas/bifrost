//! Issue #2089: C inverse precision has a recovery-only membership frontier.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{AnalyzerConfig, CodeUnitIndex, CppAnalyzer, Language};
use tree_sitter::Parser;

fn has_error_ancestor(source: &str, start: usize, end: usize) -> bool {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("C++ grammar");
    let tree = parser.parse(source, None).expect("parse C fixture");
    let mut node = tree
        .root_node()
        .descendant_for_byte_range(start, end)
        .expect("node at range");
    loop {
        if node.is_error() {
            return true;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

#[test]
fn recovered_c_references_back_inverse_precision_without_becoming_probes() {
    let source = r#"typedef struct Widget Widget;
struct Widget { int field; };
#define THIS(type) type *self

int trigger(Widget *value) { return value->field; }
int recovered(void) { THIS(const Widget); return 0; }

struct Stamp { int sec; int nsec; };
struct State { struct Stamp timestamp; };
#define DISCARD(value) 0
int stamp_trigger(struct State *state) { return state->timestamp.sec; }
int recovered_member(struct State *state) {
    DISCARD(const int = state->timestamp);
    return 0;
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("recovered.c", source)
        .build();
    let recovered_type = source.rfind("Widget").expect("recovered Widget");
    let recovered_member = source.rfind("timestamp").expect("recovered timestamp");
    assert!(
        has_error_ancestor(source, recovered_type, recovered_type + "Widget".len()),
        "the fixture must put the recovered type beneath ERROR"
    );
    assert!(
        has_error_ancestor(
            source,
            recovered_member,
            recovered_member + "timestamp".len()
        ),
        "the fixture must put the recovered member beneath ERROR"
    );

    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "c".to_string(),
            max_files: 20,
            max_sites: 1_000,
            max_candidates_per_file: 1_000,
            max_source_bytes: 100_000,
            max_targets: 1_000,
            max_usage_files: 20,
            max_usages: 1_000,
            probe_seed: ProbeSeed::Census,
            ..ReferenceDifferentialConfig::default()
        },
    )
    .expect("run inline C census differential");

    assert!(
        report
            .sites
            .iter()
            .all(|site| site.start_byte != recovered_type && site.start_byte != recovered_member),
        "the conservative census must not probe ERROR descendants: {:#?}",
        report.sites
    );
    assert!(
        report.inverse_precision_findings.is_empty(),
        "structured recovered references must be precision-backed: {:#?}",
        report.inverse_precision_findings
    );

    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let widget = analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.is_class() && unit.fq_name() == "Widget")
        .expect("Widget target");
    let hits = UsageFinder::new()
        .find_usages_default(&analyzer, &[widget])
        .all_hits();
    assert!(
        hits.iter().any(|hit| {
            hit.start_offset == recovered_type && hit.end_offset == recovered_type + "Widget".len()
        }),
        "the inverse graph must actually prove the recovered type hit: {hits:#?}"
    );

    let timestamp = analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.is_field() && unit.fq_name() == "State.timestamp")
        .expect("State.timestamp target");
    let hits = UsageFinder::new()
        .find_usages_default(&analyzer, &[timestamp])
        .all_hits();
    assert!(
        hits.iter().any(|hit| {
            hit.start_offset == recovered_member
                && hit.end_offset == recovered_member + "timestamp".len()
        }),
        "the inverse graph must actually prove the recovered member hit: {hits:#?}"
    );
}
