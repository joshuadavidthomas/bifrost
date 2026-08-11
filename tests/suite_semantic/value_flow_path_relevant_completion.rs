//! Path-relevant value-flow completion for the balanced scalar scenario
//! (#1952).
//!
//! Each language fixture carries the smallest balanced pair from the issue:
//! a positive whose source-call result reaches sink argument 0, and a negative
//! whose source-call result is unused while the sink receives a constant. The
//! completing languages (Python, Java) omit at least
//! `StaticMemory`, so a completed run here proves that an unrelated missing
//! memory capability no longer opens the selected scalar path. Two cases pin
//! the opposite direction: a Python attribute store on the path, and the C
//! initializer-to-object transfer the C/C++ adapter does not represent yet.
//! Both keep a typed incomplete result, the plan retains the first cause, and
//! neither is weakened into a clean negative.

use brokk_bifrost::analyzer::dataflow::{DataflowRequest, SemanticInputStatus, SolverBudget};
use brokk_bifrost::analyzer::semantic::{
    CancellationToken, DispatchOracle, EvidenceCompleteness, OracleCallContext, ProcedureHandle,
    ProofStatus, SemanticBudget, SemanticCapability, SemanticRequest, ValueFlowOracle,
};
use brokk_bifrost::analyzer::value_flow::{
    ValueFlowCarrier, ValueFlowEventKey, ValueFlowEventKind, ValueFlowIncompleteCause,
    ValueFlowInput, ValueFlowMayStatus, ValueFlowObservationPhase, ValueFlowPlan,
    ValueFlowSinkOutcome, ValueFlowSinkSpec, ValueFlowSourceSpec, ValueFlowSummaryResult,
    solve_value_flow_with_summaries,
};
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};

use crate::common::{InlineTestProject, semantic_graph::SemanticGraph};

// The interleaved `noop()` call pins the resolved-call bypass: a caller-side
// fact must survive a fully resolved call on its way to the sink. That edge
// used to exist only as a side effect of blanket dispatch-gap boundaries.
const PYTHON_SOURCE: &str = r#"
def source():
    return "tainted"

def sink(value):
    pass

def noop():
    pass

def positive():
    data = source()
    noop()
    sink(data)

def negative():
    unused = source()
    sink("constant")
"#;

const PYTHON_RELEVANT_GAP_SOURCE: &str = r#"
def source():
    return "tainted"

def sink(value):
    pass

def relevant(holder):
    holder.slot = source()
    sink(holder.slot)
"#;

const JAVA_SOURCE: &str = r#"
final class BalancedFlowFixture {
  static String source() {
    return "tainted";
  }

  static void sink(String value) {}

  static void positive() {
    String data = source();
    sink(data);
  }

  static void negative() {
    String unused = source();
    sink("constant");
  }
}
"#;

const C_BALANCED_SOURCE: &str = r#"
const char *source(void) {
    return "tainted";
}

void sink(const char *value) {}

void positive(void) {
    const char *data = source();
    sink(data);
}

void negative(void) {
    const char *unused = source();
    sink("constant");
}
"#;

struct BalancedCase {
    _project: crate::common::BuiltInlineTestProject,
    analyzer: WorkspaceAnalyzer,
    root: ProcedureHandle,
    plan: ValueFlowPlan,
}

fn procedure_named(graph: &SemanticGraph, name: &str) -> ProcedureHandle {
    let procedure = graph
        .artifact()
        .procedures()
        .iter()
        .find(|procedure| {
            procedure
                .locator()
                .declaration()
                .segments()
                .last()
                .and_then(|segment| segment.name())
                == Some(name)
        })
        .unwrap_or_else(|| panic!("missing procedure {name}"));
    graph
        .artifact()
        .procedure_handle(procedure.id())
        .expect("selected procedure remains live")
}

fn procedure_name(procedure: &ProcedureHandle) -> String {
    procedure
        .semantics()
        .locator()
        .declaration()
        .segments()
        .last()
        .and_then(|segment| segment.name())
        .unwrap_or_default()
        .to_owned()
}

/// Discover snapshots and call bindings from the root exactly the way the
/// production policy compiler does, and bind the source call's result value
/// and the sink call's argument 0 as the demanded endpoints.
fn balanced_case(language: Language, file: &str, source: &str, root_name: &str) -> BalancedCase {
    let project = InlineTestProject::with_language(language)
        .file(file, source)
        .build();
    let analyzer = project.workspace_analyzer(AnalyzerConfig {
        parallelism: Some(1),
        ..AnalyzerConfig::default()
    });
    let graph = SemanticGraph::materialize(&project, &analyzer, file);
    let root = procedure_named(&graph, root_name);

    let oracle = analyzer.semantic_oracle_provider();
    let cancellation = CancellationToken::default();
    let mut budget = SemanticBudget::default();
    let context = OracleCallContext::empty();

    let mut pending = vec![root.clone()];
    let mut seen = Vec::new();
    let mut snapshots = Vec::new();
    let mut bindings = Vec::new();
    let mut source_endpoint = None;
    let mut sink_endpoint = None;
    while let Some(procedure) = pending.pop() {
        if seen.contains(&procedure) {
            continue;
        }
        seen.push(procedure.clone());
        let outcome = oracle
            .procedure_relations(
                &procedure,
                &context,
                &mut SemanticRequest::new(&mut budget, &cancellation),
            )
            .expect("procedure snapshot");
        let status = SemanticInputStatus::from_outcome(&outcome);
        let snapshot = outcome
            .available_value()
            .expect("snapshot remains available")
            .clone();
        snapshots.push(ValueFlowInput::new(snapshot, status));

        for call_row in procedure.semantics().call_sites() {
            let call = procedure
                .call_site_handle(call_row.id)
                .expect("live call-site handle");
            let dispatch = oracle
                .resolve_call(&call, &mut SemanticRequest::new(&mut budget, &cancellation))
                .expect("call dispatch");
            let dispatch_status = SemanticInputStatus::from_outcome(&dispatch);
            let Some(dispatch) = dispatch.available_value() else {
                continue;
            };
            for candidate in dispatch.candidates() {
                let callee_name = procedure_name(candidate.target());
                if procedure == root && callee_name == "source" {
                    let result = call_row.result.expect("source call has a result");
                    let point = call_row
                        .normal_continuation
                        .target()
                        .expect("source call has a normal continuation");
                    source_endpoint = Some((
                        procedure
                            .point_handle(point)
                            .expect("continuation point is scoped"),
                        ValueFlowCarrier::Value(
                            procedure.value_handle(result).expect("scoped result value"),
                        ),
                    ));
                }
                if procedure == root && callee_name == "sink" {
                    let argument = call_row
                        .arguments
                        .first()
                        .expect("sink call has an argument")
                        .value;
                    sink_endpoint = Some((
                        procedure
                            .point_handle(call_row.point)
                            .expect("call point is scoped"),
                        ValueFlowCarrier::Value(
                            procedure
                                .value_handle(argument)
                                .expect("scoped argument value"),
                        ),
                    ));
                }
                let outcome = oracle
                    .call_bindings(
                        &call,
                        candidate,
                        &context,
                        &mut SemanticRequest::new(&mut budget, &cancellation),
                    )
                    .expect("call bindings");
                let status = dispatch_status.merge(SemanticInputStatus::from_outcome(&outcome));
                if let Some(binding) = outcome.available_value().cloned() {
                    bindings.push(ValueFlowInput::new(binding, status));
                    pending.push(candidate.target().clone());
                }
            }
        }
    }

    let (source_point, source_carrier) = source_endpoint.expect("root calls source");
    let (sink_point, sink_carrier) = sink_endpoint.expect("root calls sink");
    let sources = vec![ValueFlowSourceSpec::new(
        ValueFlowEventKey::at_point(&source_point, 0, ValueFlowEventKind::Source)
            .expect("stable source event"),
        source_point,
        ValueFlowObservationPhase::BeforeEffects,
        source_carrier,
        ProofStatus::Proven,
        EvidenceCompleteness::Complete,
    )];
    let sinks = vec![ValueFlowSinkSpec::new(
        ValueFlowEventKey::at_point(&sink_point, 0, ValueFlowEventKind::Sink)
            .expect("stable sink event"),
        sink_point,
        ValueFlowObservationPhase::BeforeEffects,
        sink_carrier,
        ProofStatus::Proven,
        EvidenceCompleteness::Complete,
    )];
    let plan = ValueFlowPlan::try_new(root.clone(), snapshots, bindings, sources, sinks)
        .expect("balanced value-flow plan");
    BalancedCase {
        _project: project,
        analyzer,
        root,
        plan,
    }
}

fn solve(case: &BalancedCase) -> ValueFlowSummaryResult {
    let cancellation = CancellationToken::default();
    let mut solver_budget = SolverBudget::default();
    let mut semantic_budget = SemanticBudget::default();
    solve_value_flow_with_summaries(
        &case.root,
        &case.analyzer.icfg_provider(),
        &case.plan,
        &mut semantic_budget,
        &mut DataflowRequest::new(&mut solver_budget, &cancellation),
    )
    .expect("balanced value-flow solve")
}

fn assert_complete_positive(language: Language, file: &str, source: &str) {
    let case = balanced_case(language, file, source, "positive");
    assert!(
        !case
            .root
            .artifact()
            .capabilities()
            .is_available(SemanticCapability::StaticMemory),
        "the fixture language must omit StaticMemory for the unrelated-capability premise"
    );
    assert_eq!(
        case.plan.discovery_status(),
        SemanticInputStatus::Complete,
        "positive discovery status; first cause: {:?}; root gaps: {:?}",
        case.plan.first_incomplete_cause(),
        case.root.semantics().gaps(),
    );
    assert!(case.plan.discovery_complete(), "positive discovery");
    assert!(
        case.plan.first_incomplete_cause().is_none(),
        "a complete discovery retains no cause: {:?}",
        case.plan.first_incomplete_cause()
    );
    let result = solve(&case);
    assert!(
        result.is_complete(),
        "positive run completes; boundaries: {:?}; unproven edges: {:?}; partial edges: {:?}",
        result.result().coverage().boundaries(),
        result.result().coverage().unproven_edges(),
        result.result().coverage().partial_edges(),
    );
    let (sink_id, _) = case.plan.sinks().next().expect("one sink");
    match result.sink_outcome(sink_id) {
        ValueFlowSinkOutcome::Reached(meetings) => {
            assert!(
                meetings
                    .iter()
                    .any(|meeting| meeting.may_status() == ValueFlowMayStatus::Proven),
                "the balanced positive must retain a proven meeting"
            );
        }
        other => panic!("positive sink outcome must be Reached, got {other:?}"),
    }
}

fn assert_complete_negative(language: Language, file: &str, source: &str) {
    let case = balanced_case(language, file, source, "negative");
    assert_eq!(
        case.plan.discovery_status(),
        SemanticInputStatus::Complete,
        "negative discovery status; first cause: {:?}; root gaps: {:?}",
        case.plan.first_incomplete_cause(),
        case.root.semantics().gaps(),
    );
    assert!(case.plan.discovery_complete(), "negative discovery");
    let result = solve(&case);
    assert!(
        result.is_complete(),
        "the balanced negative must complete so its absence of findings is meaningful"
    );
    let (sink_id, _) = case.plan.sinks().next().expect("one sink");
    assert_eq!(
        result.sink_outcome(sink_id),
        ValueFlowSinkOutcome::NotReached,
        "negative sink outcome"
    );
}

#[test]
fn python_balanced_positive_completes_with_a_proven_meeting() {
    assert_complete_positive(Language::Python, "app.py", PYTHON_SOURCE);
}

#[test]
fn python_balanced_negative_completes_without_a_finding() {
    assert_complete_negative(Language::Python, "app.py", PYTHON_SOURCE);
}

#[test]
fn java_balanced_positive_completes_with_a_proven_meeting() {
    assert_complete_positive(Language::Java, "src/BalancedFlowFixture.java", JAVA_SOURCE);
}

#[test]
fn java_balanced_negative_completes_without_a_finding() {
    assert_complete_negative(Language::Java, "src/BalancedFlowFixture.java", JAVA_SOURCE);
}

/// The C/C++ adapter does not represent initializer-to-object value transfer
/// yet, and it says so with a typed `Assignments` gap on the declaration. That
/// gap is path-relevant here -- the positive path runs through the initializer
/// -- so both balanced C cases must stay typed incomplete, and the negative
/// must not be weakened into a clean verdict.
#[test]
fn c_initializer_gap_keeps_both_balanced_cases_typed_incomplete() {
    for root in ["positive", "negative"] {
        let case = balanced_case(Language::Cpp, "src/balanced.c", C_BALANCED_SOURCE, root);
        assert!(
            !case.plan.discovery_complete(),
            "{root}: the initializer gap must keep discovery incomplete"
        );
        let cause = case
            .plan
            .first_incomplete_cause()
            .expect("an incomplete discovery retains its first cause");
        assert!(
            matches!(cause, ValueFlowIncompleteCause::Snapshot { .. }),
            "{root}: the first cause is the flow procedure snapshot: {cause:?}"
        );
        let result = solve(&case);
        assert!(!result.is_complete(), "{root}: the run must not complete");
        let (sink_id, _) = case.plan.sinks().next().expect("one sink");
        assert!(
            !matches!(
                result.sink_outcome(sink_id),
                ValueFlowSinkOutcome::NotReached
            ),
            "{root}: an incomplete run must not report a clean negative"
        );
    }
}

/// An attribute store on the selected path is a path-relevant gap: the flow
/// procedure's snapshot stays typed non-complete, the plan retains it as the
/// first cause, and the sink outcome stays inconclusive rather than becoming
/// an optimistic clean negative.
#[test]
fn python_attribute_store_on_the_path_keeps_a_typed_incomplete_result() {
    let case = balanced_case(
        Language::Python,
        "app.py",
        PYTHON_RELEVANT_GAP_SOURCE,
        "relevant",
    );
    assert!(
        !case.plan.discovery_complete(),
        "a path-relevant gap must keep discovery incomplete"
    );
    let cause = case
        .plan
        .first_incomplete_cause()
        .expect("an incomplete discovery retains its first cause");
    assert!(
        matches!(cause, ValueFlowIncompleteCause::Snapshot { .. }),
        "the first cause is the flow procedure's snapshot: {cause:?}"
    );
    assert!(
        cause.status().is_some_and(|status| !status.is_complete()),
        "the cause keeps the typed input status: {cause:?}"
    );
    let result = solve(&case);
    assert!(!result.is_complete(), "the run must not complete");
    let (sink_id, _) = case.plan.sinks().next().expect("one sink");
    assert!(
        !matches!(
            result.sink_outcome(sink_id),
            ValueFlowSinkOutcome::NotReached
        ),
        "an incomplete run must not report a clean negative"
    );
}
