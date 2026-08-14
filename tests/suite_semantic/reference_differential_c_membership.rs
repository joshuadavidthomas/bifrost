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
    let source = r#"#include "target.h"
typedef struct Widget Widget;
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

int recovered_call_trigger(int value) { return recovered_call_target(value); }
#define TARGET_KIND 1
int recovered_explicit_assignment(void) {
    int explicit = 0;
    if (explicit == 0) {
        explicit = recovered_call_target(TARGET_KIND, "m",
                                         recovered_helper("c", explicit));
    }
    return explicit;
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "target.h",
            "int recovered_call_target(int value, ...);\nint recovered_helper(const char *kind, int value);\n",
        )
        .file("recovered.c", source)
        .build();
    let recovered_type = source.rfind("Widget").expect("recovered Widget");
    let recovered_member = source.rfind("timestamp").expect("recovered timestamp");
    let recovered_callee = source
        .find("recovered_call_target(TARGET_KIND")
        .expect("recovered call target");
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
    assert!(
        has_error_ancestor(
            source,
            recovered_callee,
            recovered_callee + "recovered_call_target".len()
        ),
        "the C identifier `explicit` must put the following callable beneath ERROR"
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
        report.sites.iter().all(|site| {
            site.start_byte != recovered_type
                && site.start_byte != recovered_member
                && site.start_byte != recovered_callee
        }),
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

    let recovered_call_target = analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.is_function() && unit.fq_name() == "recovered_call_target")
        .expect("recovered_call_target target");
    let hits = UsageFinder::new()
        .find_usages_default(&analyzer, &[recovered_call_target])
        .all_hits();
    assert!(
        hits.iter().any(|hit| {
            hit.start_offset == recovered_callee
                && hit.end_offset == recovered_callee + "recovered_call_target".len()
        }),
        "the inverse graph must prove the recovered call hit: {hits:#?}"
    );
}
