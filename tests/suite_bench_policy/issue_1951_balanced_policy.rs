//! Issue #1951: the balanced source-call scenario on the production `.rqlp`
//! route, for every documented direct-ready language entry.
//!
//! Each language evaluates the DataFlowBench `core-direct.rqlp` policy shape
//! against the exact balanced fixture pair shared with the direct
//! `ValueFlowPlan`, JSON CodeQuery, and RQL routes. The positive must retain
//! one finding whose bound source and sink observation points are byte-equal
//! to the direct route's plan endpoints; the negative must retain none. Run
//! completion must agree with the direct route's typed completeness: complete
//! languages finish `Complete`, languages with a path-relevant semantic gap
//! stay honestly `Inconclusive { PartialDiscovery }`, and no case degrades
//! into a capability failure or an optimistic clean pass.

use std::ops::Range;

use brokk_bifrost::AnalyzerConfig;
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyIncompleteReason, PolicyRunCompletion, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer,
};

use crate::common::InlineTestProject;
use crate::value_flow_conformance::resolve_value_flow_conformance_case;
use crate::value_flow_scenarios::{
    BalancedSourceCallShape, balanced_source_call_scenario_entries, c_balanced_source_call_shape,
    cpp_balanced_source_call_shape, csharp_balanced_source_call_shape,
    go_balanced_source_call_shape, java_balanced_source_call_shape,
    javascript_balanced_source_call_shape, kotlin_balanced_source_call_shape,
    php_balanced_source_call_shape, python_balanced_source_call_shape,
    ruby_balanced_source_call_shape, rust_balanced_source_call_shape,
    scala_balanced_source_call_shape, typescript_balanced_source_call_shape,
    with_balanced_source_call_negative, with_balanced_source_call_positive,
};

/// The DataFlowBench `core-direct.rqlp` policy shape: structured call
/// selectors, a return-value source binding, an argument-0 sink binding, and
/// optimistic unmodeled-call behavior.
fn core_direct_policy(id: &str) -> String {
    format!(
        r#"(policy
  :schema-version 1
  :id "{id}"
  :name "DataFlowBench controlled direct taint"
  :message "Controlled input reaches the direct benchmark sink"
  :severity warning
  :analysis (analysis :type taint :mode may
    :call-modeling (call-modeling :unmodeled optimistic)
    :sources (endpoint-set :entries [(source :id input :display-name "benchmark input" :categories [input.user-controlled] :selector (rql :schema-version 1 (call :callee (name "dfb_source"))) :bind return-value :labels [attacker-controlled])])
    :sinks (endpoint-set :entries [(sink :id sink :display-name "benchmark sink" :categories [data.sensitive] :selector (rql :schema-version 1 (call :callee (name "dfb_sink"))) :dangerous-operand (argument :index 0) :accepts [attacker-controlled])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Test" :id "BROAD-TAINT")))"#
    )
}

fn evaluate_balanced(
    shape: &BalancedSourceCallShape,
    source: &str,
    id: &str,
) -> PolicyBatchOutcome {
    let project = InlineTestProject::with_language(shape.language)
        .file(shape.path, source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let input = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new(format!("test:{id}.rqlp")),
        core_direct_policy(id),
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed evaluation date"),
    );
    evaluate_policy_inputs_with_analyzer(project.root(), &input, &workspace, &options, None)
        .expect("production taint evaluation")
}

/// The semantic observation points the production route bound, as byte spans.
fn bound_endpoint_spans(outcome: &PolicyBatchOutcome) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let [retained] = outcome.taint_analysis_results() else {
        panic!(
            "expected one retained production analysis, got {}; report={:#?}",
            outcome.taint_analysis_results().len(),
            outcome.report()
        );
    };
    let value_flow = retained.plan().value_flow();
    let span = |site: &brokk_bifrost::analyzer::semantic::SemanticLocator| {
        let anchor = site.anchor().span();
        anchor.start_byte() as usize..anchor.end_byte() as usize
    };
    let mut sources = value_flow
        .sources()
        .map(|(_, spec)| span(spec.key().site()))
        .collect::<Vec<_>>();
    let mut sinks = value_flow
        .sinks()
        .map(|(_, spec)| span(spec.key().site()))
        .collect::<Vec<_>>();
    sources.sort_by_key(|range| (range.start, range.end));
    sinks.sort_by_key(|range| (range.start, range.end));
    (sources, sinks)
}

fn assert_completion(
    shape: &BalancedSourceCallShape,
    outcome: &PolicyBatchOutcome,
    complete: bool,
) {
    let run = &outcome.report().runs()[0];
    if complete {
        assert!(
            matches!(run.completion(), PolicyRunCompletion::Complete),
            "{} completion: {:?}, diagnostics: {:?}",
            shape.name,
            run.completion(),
            run.diagnostics()
        );
    } else {
        assert!(
            matches!(
                run.completion(),
                PolicyRunCompletion::Inconclusive { reasons }
                    if reasons.as_slice() == [PolicyIncompleteReason::PartialDiscovery]
            ),
            "{} completion: {:?}, diagnostics: {:?}",
            shape.name,
            run.completion(),
            run.diagnostics()
        );
    }
}

/// The direct route's plan endpoint observation spans for the same case.
fn direct_endpoint_spans(
    case: &crate::value_flow_conformance::ValueFlowConformanceCase<'_>,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let resolved = resolve_value_flow_conformance_case(case);
    let span = |site: &brokk_bifrost::analyzer::semantic::SemanticLocator| {
        let anchor = site.anchor().span();
        anchor.start_byte() as usize..anchor.end_byte() as usize
    };
    let mut sources = resolved
        .plan
        .sources()
        .map(|(_, spec)| span(spec.key().site()))
        .collect::<Vec<_>>();
    let mut sinks = resolved
        .plan
        .sinks()
        .map(|(_, spec)| span(spec.key().site()))
        .collect::<Vec<_>>();
    sources.sort_by_key(|range| (range.start, range.end));
    sinks.sort_by_key(|range| (range.start, range.end));
    (sources, sinks)
}

fn assert_balanced_positive_policy(shape: &BalancedSourceCallShape) {
    let outcome = evaluate_balanced(
        shape,
        shape.positive,
        &format!("test.issue-1951.{}.positive", shape.name),
    );
    assert_completion(shape, &outcome, shape.positive_result_complete);
    let run = &outcome.report().runs()[0];
    assert_eq!(
        run.findings().len(),
        1,
        "{} positive findings: {:?}",
        shape.name,
        run.diagnostics()
    );
    let findings = outcome.taint_findings();
    assert_eq!(findings.len(), 1, "{} public findings", shape.name);
    assert_eq!(
        findings[0].origins.len(),
        1,
        "{} one source origin",
        shape.name
    );
    assert!(
        !findings[0].witnesses.is_empty(),
        "{} finding retains a witness",
        shape.name
    );
    for witness in &findings[0].witnesses {
        assert!(
            !witness.truncated,
            "{} witness must not be truncated: {witness:#?}",
            shape.name
        );
        assert_eq!(witness.omitted_steps_lower_bound, 0, "{}", shape.name);
    }

    // Route parity: the production policy observed exactly the program
    // points the direct plan observes.
    let (policy_sources, policy_sinks) = bound_endpoint_spans(&outcome);
    let (direct_sources, direct_sinks) =
        with_balanced_source_call_positive(shape, direct_endpoint_spans);
    assert_eq!(
        policy_sources, direct_sources,
        "{} production and direct source observation spans",
        shape.name
    );
    assert_eq!(
        policy_sinks, direct_sinks,
        "{} production and direct sink observation spans",
        shape.name
    );
}

fn assert_balanced_negative_policy(shape: &BalancedSourceCallShape) {
    let outcome = evaluate_balanced(
        shape,
        shape.negative,
        &format!("test.issue-1951.{}.negative", shape.name),
    );
    assert_completion(shape, &outcome, shape.negative_result_complete);
    let run = &outcome.report().runs()[0];
    assert!(
        run.findings().is_empty(),
        "{} negative findings: {:?}",
        shape.name,
        run.findings()
    );
    assert!(
        outcome.taint_findings().is_empty(),
        "{} negative public findings",
        shape.name
    );

    let (policy_sources, policy_sinks) = bound_endpoint_spans(&outcome);
    let (direct_sources, direct_sinks) =
        with_balanced_source_call_negative(shape, direct_endpoint_spans);
    assert_eq!(
        policy_sources, direct_sources,
        "{} production and direct source observation spans",
        shape.name
    );
    assert_eq!(
        policy_sinks, direct_sinks,
        "{} production and direct sink observation spans",
        shape.name
    );
}

macro_rules! define_policy_balanced_source_call_tests {
    ($(($name:ident, $shape:ident, $direct_positive:ident, $direct_negative:ident, $public_positive:ident, $public_negative:ident),)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn balanced_positive_policy() {
                    assert_balanced_positive_policy(&$shape());
                }

                #[test]
                fn balanced_negative_policy() {
                    assert_balanced_negative_policy(&$shape());
                }
            }
        )*
    };
}
balanced_source_call_scenario_entries!(define_policy_balanced_source_call_tests);
