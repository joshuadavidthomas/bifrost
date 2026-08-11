//! Issue #1953: bind Ruby policy selector rows to semantic call identities.
//!
//! The Ruby semantic adapter anchors a procedure locator over the `def`
//! header only, so the old enclosing-procedure narrowing by anchor
//! containment never found the procedure that owns a body span. Binding now
//! considers every procedure in the file artifact and decides by the call
//! site's own source anchor. These tests pin that identity for parenthesized
//! and unparenthesized Ruby call forms, nested calls, near misses, and the
//! typed results for rows the adapter cannot represent.
//!
//! Ruby value-position bare calls (`dfb_sink(dfb_source)` and
//! `value = dfb_source`) produce no structural call row today, so a call
//! selector cannot find them; #1956 owns that selection gap. The tests here
//! pin the current honest zero-selection outcome for those forms.

use crate::common::InlineTestProject;
use brokk_bifrost::AnalyzerConfig;
use brokk_bifrost::Language;
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyIncompleteReason, PolicyRunCompletion, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer,
};
use std::ops::Range;

fn ruby_policy(id: &str, source_selector: &str) -> String {
    format!(
        r#"(policy
          :schema-version 1
          :id "{id}"
          :name "Ruby selector-row semantic call binding"
          :message "tainted value reached dfb_sink"
          :severity warning
          :analysis (analysis
            :type taint
            :mode may
            :call-modeling (call-modeling :unmodeled optimistic)
            :sources (endpoint-set :entries [
              (source :id dfb-source :display-name "dfb_source" :categories [input.user]
                :selector (rql :schema-version 1 (language ruby {source_selector}))
                :bind return-value :labels [untrusted])])
            :sinks (endpoint-set :entries [
              (sink :id dfb-sink :display-name "dfb_sink" :categories [data.sensitive]
                :selector (rql :schema-version 1
                  (language ruby (call :callee (name "dfb_sink"))))
                :dangerous-operand (argument :index 0) :accepts [untrusted])]))
          :classification (classification
            :fallback (classification-id :taxonomy "Test" :id "BROAD-TAINT")))"#
    )
}

fn call_source_policy(id: &str) -> String {
    ruby_policy(id, r#"(call :callee (name "dfb_source"))"#)
}

fn evaluate_ruby(source: &str, policy_source: &str) -> PolicyBatchOutcome {
    let project = InlineTestProject::with_language(Language::Ruby)
        .file("app.rb", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let input = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:issue-1953.rqlp"),
        policy_source,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed evaluation date"),
    );
    evaluate_policy_inputs_with_analyzer(project.root(), &input, &workspace, &options, None)
        .expect("production taint evaluation")
}

fn propagation_solves(outcome: &PolicyBatchOutcome) -> Option<u64> {
    outcome.report().runs()[0]
        .work()
        .metrics()
        .iter()
        .find(|metric| metric.name() == "taint.propagation_solves")
        .map(|metric| metric.value())
}

/// The semantic identities the compiled plan bound, before the solver runs.
///
/// Each endpoint span is the source anchor of the program point the plan
/// observes, which for a call binding is the call expression itself.
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

fn span_of(source: &str, needle: &str) -> Range<usize> {
    let start = source.find(needle).expect("fixture contains the needle");
    assert_eq!(
        source[start + 1..].find(needle),
        None,
        "needle {needle:?} must be unique in the fixture"
    );
    start..start + needle.len()
}

/// Every selected row bound exactly and the solve ran. Ruby bare and
/// receiver calls carry a dynamic-dispatch semantic gap, so the solved
/// report stays honestly partial: completion is inconclusive
/// partial-discovery, never a binding capability failure.
fn assert_reached_propagation(outcome: &PolicyBatchOutcome) {
    let run = &outcome.report().runs()[0];
    assert!(
        matches!(
            run.completion(),
            PolicyRunCompletion::Inconclusive { reasons }
                if reasons.as_slice() == [PolicyIncompleteReason::PartialDiscovery]
        ),
        "completion: {:?}, diagnostics: {:?}",
        run.completion(),
        run.diagnostics()
    );
    assert!(run.diagnostics().is_empty(), "{:?}", run.diagnostics());
    assert_eq!(propagation_solves(outcome), Some(1), "no propagation solve");
}

fn assert_capability_incomplete(outcome: &PolicyBatchOutcome, expected_message_part: &str) {
    let run = &outcome.report().runs()[0];
    assert!(
        matches!(
            run.completion(),
            PolicyRunCompletion::Inconclusive { reasons }
                if reasons.contains(&brokk_bifrost::policy::PolicyIncompleteReason::CapabilityIncomplete)
        ),
        "completion: {:?}",
        run.completion()
    );
    assert!(
        run.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains(expected_message_part)),
        "diagnostics: {:?}",
        run.diagnostics()
    );
    assert!(run.findings().is_empty());
    assert_eq!(propagation_solves(outcome), None, "must stop before solve");
}

/// The parenthesized source form binds both endpoints to their exact
/// semantic calls and the positive reaches propagation with a finding.
#[test]
fn parenthesized_source_call_binds_exactly_and_produces_the_finding() {
    let source = r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  dfb_sink(dfb_source())
end
"#;
    let outcome = evaluate_ruby(source, &call_source_policy("test.issue-1953.parenthesized"));
    assert_reached_propagation(&outcome);
    let (sources, sinks) = bound_endpoint_spans(&outcome);
    assert_eq!(sources, vec![span_of(source, "dfb_source()")]);
    assert_eq!(sinks, vec![span_of(source, "dfb_sink(dfb_source())")]);
    let run = &outcome.report().runs()[0];
    assert_eq!(run.findings().len(), 1, "{:?}", run.diagnostics());
    assert_eq!(outcome.taint_findings().len(), 1);
}

/// The unparenthesized statement-position source call binds to its exact
/// semantic call. The unused source result and the constant sink argument
/// reach propagation and stay clean.
#[test]
fn unparenthesized_statement_call_with_constant_sink_reaches_propagation_clean() {
    let source = r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  dfb_source
  dfb_sink("clean")
end
"#;
    let outcome = evaluate_ruby(source, &call_source_policy("test.issue-1953.statement"));
    assert_reached_propagation(&outcome);
    let (sources, sinks) = bound_endpoint_spans(&outcome);
    let statement = span_of(source, "dfb_source\n  dfb_sink");
    assert_eq!(
        sources,
        vec![statement.start..statement.start + "dfb_source".len()]
    );
    assert_eq!(sinks, vec![span_of(source, "dfb_sink(\"clean\")")]);
    let run = &outcome.report().runs()[0];
    assert!(run.findings().is_empty(), "{:?}", run.findings());
    assert!(outcome.taint_findings().is_empty());
}

/// A sink row that contains two nested calls binds to the exact outer call,
/// and the nested source row binds to the exact inner call. Neither row is
/// reported ambiguous.
#[test]
fn nested_calls_bind_each_selector_row_to_its_exact_semantic_call() {
    let source = r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def wrap(value)
  value
end

def run
  dfb_sink(wrap(dfb_source()))
end
"#;
    let outcome = evaluate_ruby(source, &call_source_policy("test.issue-1953.nested"));
    assert_reached_propagation(&outcome);
    let (sources, sinks) = bound_endpoint_spans(&outcome);
    assert_eq!(sources, vec![span_of(source, "dfb_source()")]);
    assert_eq!(sinks, vec![span_of(source, "dfb_sink(wrap(dfb_source()))")]);
    let run = &outcome.report().runs()[0];
    assert_eq!(run.findings().len(), 1, "{:?}", run.diagnostics());
}

/// A same-named method call on another receiver binds to its own exact
/// semantic call and does not contribute a flow; only the free-function
/// source that feeds the sink produces the finding.
#[test]
fn same_named_method_on_another_receiver_binds_separately() {
    let source = r#"
class Helper
  def dfb_source
    "unrelated"
  end
end

def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  helper = Helper.new
  helper.dfb_source
  dfb_sink(dfb_source())
end
"#;
    let outcome = evaluate_ruby(source, &call_source_policy("test.issue-1953.receiver"));
    assert_reached_propagation(&outcome);
    let (sources, sinks) = bound_endpoint_spans(&outcome);
    assert_eq!(
        sources,
        vec![
            span_of(source, "helper.dfb_source"),
            span_of(source, "dfb_source()"),
        ]
    );
    assert_eq!(sinks, vec![span_of(source, "dfb_sink(dfb_source())")]);
    let run = &outcome.report().runs()[0];
    assert_eq!(run.findings().len(), 1, "{:?}", run.diagnostics());
    let brokk_bifrost::policy::PolicyFindingEvidence::Taint { evidence } =
        run.findings()[0].evidence()
    else {
        panic!("expected taint evidence");
    };
    assert_eq!(evidence.origins().len(), 1, "{:?}", evidence.origins());
}

/// A selected call row the semantic adapter does not model as a call site
/// (the read side of an attribute or-assignment) stays a typed capability
/// result instead of guessing a nearby call.
#[test]
fn call_row_without_a_semantic_call_site_stays_a_typed_capability_result() {
    let source = r#"
def dfb_source
  "tainted"
end

def run
  self.dfb_sink ||= dfb_source()
end
"#;
    let outcome = evaluate_ruby(source, &call_source_policy("test.issue-1953.unrepresented"));
    assert_capability_incomplete(&outcome, "does not identify a semantic call site");
}

/// A selector row that spans two sibling calls without being a call itself
/// stays a typed capability result: binding refuses to guess between the
/// contained calls.
#[test]
fn selector_row_containing_sibling_calls_stays_typed_not_guessed() {
    let source = r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  pair = [dfb_source(), dfb_source]
  dfb_sink(pair)
end
"#;
    let outcome = evaluate_ruby(
        source,
        &ruby_policy("test.issue-1953.sibling-span", "(assignment)"),
    );
    assert_capability_incomplete(&outcome, "does not identify a semantic call site");
}

/// Ruby bare calls in value positions produce no structural call row yet
/// (#1956), so the source selection is honestly empty and the run compiles
/// clean without a propagation solve. This pins the current boundary; when
/// #1956 lands, these forms must move to the exact-binding positives above.
#[test]
fn value_position_bare_calls_are_not_selectable_yet() {
    for source in [
        r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  dfb_sink(dfb_source)
end
"#,
        r#"
def dfb_source
  "tainted"
end

def dfb_sink(value)
end

def run
  value = dfb_source
  dfb_sink(value)
end
"#,
    ] {
        let outcome = evaluate_ruby(
            source,
            &call_source_policy("test.issue-1953.value-position"),
        );
        let run = &outcome.report().runs()[0];
        assert!(
            matches!(run.completion(), PolicyRunCompletion::Complete),
            "completion: {:?}, diagnostics: {:?}",
            run.completion(),
            run.diagnostics()
        );
        assert!(run.findings().is_empty());
        assert_eq!(
            propagation_solves(&outcome),
            None,
            "empty source selection must not reach a solve"
        );
        assert!(outcome.taint_analysis_results().is_empty());
    }
}
