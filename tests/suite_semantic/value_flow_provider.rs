//! Behavior tests for the demand value-flow provider (Stage C foundation).
//!
//! These prove the advertised interface end to end against real materialized
//! procedures: a snapshot materialized through the provider matches the raw
//! oracle, a repeat is a cache hit that charges no budget, and the cache is
//! content addressed so a source edit misses.

use std::collections::BTreeSet;
use std::sync::Arc;

use brokk_bifrost::analyzer::semantic::{
    CancellationToken, DispatchOracle, OracleCallContext, ProcedureHandle, SemanticArtifact,
    SemanticBudget, SemanticOutcome, SemanticRequest, SemanticWork, ValueFlowOracle,
    ValueFlowSnapshot,
};
use brokk_bifrost::analyzer::value_flow::{
    ValueFlowCache, ValueFlowProvider, WorkspaceValueFlowProvider,
};
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};

use crate::common::{InlineTestProject, semantic_graph::SemanticGraph};

const FIXTURE_PATH: &str = "src/flow.ts";

const SOURCE: &str = "\
function leaf(value: number): number {
  return value;
}
export function caller(): number {
  return leaf(1);
}
";

// A different `leaf` body, so the artifact's content fingerprint changes while
// the workspace root, path, and procedure name stay identical.
const SOURCE_EDITED: &str = "\
function leaf(value: number): number {
  return value + 0;
}
export function caller(): number {
  return leaf(1);
}
";

fn procedure_named(artifact: &Arc<SemanticArtifact>, name: &str) -> ProcedureHandle {
    let procedure = artifact
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
    artifact
        .procedure_handle(procedure.id())
        .expect("selected procedure remains live")
}

/// Compare two value-flow snapshots by their durable content, ignoring the
/// per-call provenance arena identity. Two independent oracle calls build
/// separate arenas, so `OracleRelationHandle` identity differs even when the
/// value flow is identical.
fn assert_same_flow(actual: &ValueFlowSnapshot, expected: &ValueFlowSnapshot) {
    assert_eq!(actual.procedure(), expected.procedure(), "same procedure");
    assert_eq!(actual.coverage(), expected.coverage(), "same coverage");
    assert_eq!(
        actual.relations().len(),
        expected.relations().len(),
        "same relation count"
    );
    for (actual, expected) in actual.relations().iter().zip(expected.relations()) {
        assert_eq!(actual.point, expected.point);
        assert_eq!(actual.event_index, expected.event_index);
        assert_eq!(actual.kind, expected.kind);
        assert_eq!(actual.source, expected.source);
        assert_eq!(actual.target, expected.target);
        assert_eq!(actual.proof, expected.proof);
        assert_eq!(actual.completeness, expected.completeness);
    }
}

fn complete_snapshot(
    provider: &WorkspaceValueFlowProvider<'_>,
    procedure: &ProcedureHandle,
    budget: &mut SemanticBudget,
    cancellation: &CancellationToken,
) -> ValueFlowSnapshot {
    let outcome = provider
        .procedure_snapshot(
            procedure,
            &OracleCallContext::empty(),
            &mut SemanticRequest::new(budget, cancellation),
        )
        .expect("value-flow snapshot materialization");
    match outcome {
        SemanticOutcome::Complete { value, .. } => value,
        other => panic!("leaf procedure snapshot must be complete: {other:?}"),
    }
}

#[test]
fn provider_snapshot_matches_oracle_and_repeat_is_a_cache_hit() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file(FIXTURE_PATH, SOURCE)
        .build();
    let analyzer = project.workspace_analyzer(AnalyzerConfig::default());
    let graph = SemanticGraph::materialize(&project, &analyzer, FIXTURE_PATH);
    let leaf = procedure_named(graph.artifact(), "leaf");

    let cache = ValueFlowCache::default();
    let provider = WorkspaceValueFlowProvider::new(&analyzer, cache.clone());
    let cancellation = CancellationToken::default();

    // First materialization misses the cache and runs the oracle.
    let mut first_budget = SemanticBudget::default();
    let first = complete_snapshot(&provider, &leaf, &mut first_budget, &cancellation);
    assert_eq!(cache.snapshot_misses(), 1);
    assert_eq!(cache.snapshot_hits(), 0);
    assert_ne!(
        first_budget.used(),
        SemanticWork::default(),
        "a cache miss must charge the oracle's semantic work"
    );

    // It matches the value flow the raw oracle would return.
    let mut oracle_budget = SemanticBudget::default();
    let oracle_outcome = analyzer
        .semantic_oracle_provider()
        .procedure_relations(
            &leaf,
            &OracleCallContext::empty(),
            &mut SemanticRequest::new(&mut oracle_budget, &cancellation),
        )
        .expect("raw oracle value-flow projection");
    let oracle_snapshot = oracle_outcome
        .available_value()
        .expect("raw oracle retains a value-flow snapshot");
    assert_same_flow(&first, oracle_snapshot);

    // The second materialization is a cache hit that charges no new budget.
    let mut second_budget = SemanticBudget::default();
    let second = complete_snapshot(&provider, &leaf, &mut second_budget, &cancellation);
    assert_eq!(cache.snapshot_hits(), 1);
    assert_eq!(cache.snapshot_misses(), 1);
    assert_eq!(
        second, first,
        "the cache hit reuses the exact retained snapshot"
    );
    assert_eq!(
        second_budget.used(),
        SemanticWork::default(),
        "a cache hit must not recharge the semantic budget"
    );
}

#[test]
fn provider_snapshot_cache_is_content_keyed() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file(FIXTURE_PATH, SOURCE)
        .build();
    let analyzer = project.workspace_analyzer(AnalyzerConfig::default());
    let graph = SemanticGraph::materialize(&project, &analyzer, FIXTURE_PATH);
    let leaf = procedure_named(graph.artifact(), "leaf");

    let cache = ValueFlowCache::default();
    let provider = WorkspaceValueFlowProvider::new(&analyzer, cache.clone());
    let cancellation = CancellationToken::default();

    let mut warm_budget = SemanticBudget::default();
    let _ = complete_snapshot(&provider, &leaf, &mut warm_budget, &cancellation);
    let mut repeat_budget = SemanticBudget::default();
    let _ = complete_snapshot(&provider, &leaf, &mut repeat_budget, &cancellation);
    assert_eq!(cache.snapshot_misses(), 1);
    assert_eq!(cache.snapshot_hits(), 1);

    // Change the source content of the same file at the same path and root.
    let file = project.file(FIXTURE_PATH);
    file.write(SOURCE_EDITED).expect("rewrite fixture source");
    let updated = analyzer.update(&BTreeSet::from([file]));
    let updated_graph = SemanticGraph::materialize(&project, &updated, FIXTURE_PATH);
    let updated_leaf = procedure_named(updated_graph.artifact(), "leaf");

    // The same shared cache, a new analyzer generation. The edited content
    // yields a different artifact fingerprint, so the key differs and the
    // lookup misses instead of reusing the stale entry.
    let updated_provider = WorkspaceValueFlowProvider::new(&updated, cache.clone());
    let mut edited_budget = SemanticBudget::default();
    let _ = complete_snapshot(
        &updated_provider,
        &updated_leaf,
        &mut edited_budget,
        &cancellation,
    );
    assert_eq!(
        cache.snapshot_misses(),
        2,
        "edited content must produce a different content key and miss"
    );
    assert_eq!(
        cache.snapshot_hits(),
        1,
        "the edited content must not reuse the stale entry"
    );
}

#[test]
fn provider_call_bindings_match_oracle_and_repeat_is_a_cache_hit() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file(FIXTURE_PATH, SOURCE)
        .build();
    let analyzer: WorkspaceAnalyzer = project.workspace_analyzer(AnalyzerConfig::default());
    let graph = SemanticGraph::materialize(&project, &analyzer, FIXTURE_PATH);
    let caller = procedure_named(graph.artifact(), "caller");

    let cache = ValueFlowCache::default();
    let provider = WorkspaceValueFlowProvider::new(&analyzer, cache.clone());
    let oracle = analyzer.semantic_oracle_provider();
    let cancellation = CancellationToken::default();

    let call_id = caller
        .semantics()
        .call_sites()
        .first()
        .expect("caller has one call site")
        .id;
    let call = caller
        .call_site_handle(call_id)
        .expect("live caller owns its call site");

    let mut dispatch_budget = SemanticBudget::default();
    let dispatch_outcome = oracle
        .resolve_call(
            &call,
            &mut SemanticRequest::new(&mut dispatch_budget, &cancellation),
        )
        .expect("call dispatch");
    let dispatch = dispatch_outcome
        .available_value()
        .expect("dispatch retains a result");
    let candidate = dispatch
        .candidates()
        .first()
        .expect("the direct call resolves to at least one candidate");

    // First binding misses the cache and runs the oracle.
    let mut first_budget = SemanticBudget::default();
    let first_outcome = provider
        .call_bindings(
            &call,
            candidate,
            &OracleCallContext::empty(),
            &mut SemanticRequest::new(&mut first_budget, &cancellation),
        )
        .expect("call bindings materialization");
    let SemanticOutcome::Complete { value: first, .. } = first_outcome else {
        panic!("direct leaf call bindings must be complete: {first_outcome:?}");
    };
    assert_eq!(cache.binding_misses(), 1);
    assert_eq!(cache.binding_hits(), 0);

    // It matches what the raw oracle would return.
    let mut oracle_budget = SemanticBudget::default();
    let oracle_outcome = oracle
        .call_bindings(
            &call,
            candidate,
            &OracleCallContext::empty(),
            &mut SemanticRequest::new(&mut oracle_budget, &cancellation),
        )
        .expect("raw oracle call bindings");
    let oracle_bindings = oracle_outcome
        .available_value()
        .expect("raw oracle retains call bindings");
    assert_eq!(first.bindings().len(), oracle_bindings.bindings().len());
    assert_eq!(first.callee(), oracle_bindings.callee());
    assert_eq!(first.call(), oracle_bindings.call());

    // The second binding is a cache hit that charges no new budget.
    let mut second_budget = SemanticBudget::default();
    let second_outcome = provider
        .call_bindings(
            &call,
            candidate,
            &OracleCallContext::empty(),
            &mut SemanticRequest::new(&mut second_budget, &cancellation),
        )
        .expect("cached call bindings");
    let SemanticOutcome::Complete { value: second, .. } = second_outcome else {
        panic!("cached call bindings must be complete: {second_outcome:?}");
    };
    assert_eq!(cache.binding_hits(), 1);
    assert_eq!(cache.binding_misses(), 1);
    assert_eq!(
        second, first,
        "the cache hit reuses the exact retained bindings"
    );
    assert_eq!(
        second_budget.used(),
        SemanticWork::default(),
        "a cache hit must not recharge the semantic budget"
    );
}
