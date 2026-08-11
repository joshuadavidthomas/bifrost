//! Issue #1954: preserve taint witness truncation causes and reconstruct a
//! complete witness for the direct nested call `sink_one(source_one())`.
//!
//! The production policy route retains complete witnesses for this fixture,
//! but the one-alternative best-effort retention marks sibling alternatives as
//! truncated. That orthogonal signal must not project the retained witness,
//! the finding, or the collection as truncated, and real reconstruction or
//! collection exhaustion must keep its exact typed cause.

use crate::common::InlineTestProject;
use brokk_bifrost::AnalyzerConfig;
use brokk_bifrost::CancellationToken;
use brokk_bifrost::Language;
use brokk_bifrost::analyzer::dataflow::{
    DataflowRequest, SolverBudget, WitnessReconstructionLimits, WitnessRetentionLimits,
    WitnessTruncationCause,
};
use brokk_bifrost::analyzer::semantic::SemanticBudget;
use brokk_bifrost::analyzer::structural::CodeQueryFlowWitnessStepKind;
use brokk_bifrost::analyzer::taint::{
    TaintFindingCollectionLimits, TaintWitnessTruncationCause, collect_taint_findings_with_limits,
    solve_taint_batch_with_witnesses,
};
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicySourceIdentity, evaluate_policy_inputs_with_analyzer, write_policy_json,
};

const NESTED_SOURCE: &str = r#"
def source_one():
    return "one"

def sink_one(value):
    pass

def run():
    sink_one(source_one())
"#;

fn direct_policy(id: &str) -> String {
    format!(
        r#"(policy
          :schema-version 1
          :id "{id}"
          :name "Direct nested call witness"
          :message "tainted value reached sink_one"
          :severity warning
          :analysis (analysis
            :type taint
            :mode may
            :call-modeling (call-modeling :unmodeled optimistic)
            :sources (endpoint-set :entries [
              (source :id first :display-name "first source" :categories [input.user]
                :selector (rql :schema-version 1
                  (language python (call :callee (name "source_one"))))
                :bind return-value :labels [untrusted])])
            :sinks (endpoint-set :entries [
              (sink :id first-store :display-name "first sink" :categories [data.sensitive]
                :selector (rql :schema-version 1
                  (language python (call :callee (name "sink_one"))))
                :dangerous-operand (argument :index 0) :accepts [untrusted])]))
          :classification (classification
            :fallback (classification-id :taxonomy "Test" :id "BROAD-TAINT")))"#
    )
}

fn evaluate_nested(
    project: &crate::common::BuiltInlineTestProject,
) -> (
    PolicyBatchOutcome,
    brokk_bifrost::analyzer::WorkspaceAnalyzer,
) {
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let input = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:issue-1954.rqlp"),
        direct_policy("issue-1954-direct"),
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed evaluation date"),
    );
    let outcome =
        evaluate_policy_inputs_with_analyzer(project.root(), &input, &workspace, &options, None)
            .expect("production taint evaluation");
    (outcome, workspace)
}

fn nested_project() -> crate::common::BuiltInlineTestProject {
    InlineTestProject::with_language(Language::Python)
        .file("app.py", NESTED_SOURCE)
        .build()
}

/// Every call step must be closed by a return step of the same call site, and
/// no return may appear without its call. Continuation and intraprocedural
/// edges do not touch the stack.
fn assert_matched_calls_and_returns(
    witness: &brokk_bifrost::analyzer::structural::CodeQueryTaintWitness,
) {
    let mut stack = Vec::new();
    for step in &witness.steps {
        let CodeQueryFlowWitnessStepKind::Edge { edge_kind } = step.kind else {
            continue;
        };
        match edge_kind {
            "call_to_entry" => stack.push(step.origin.clone()),
            "normal_return" | "exceptional_return" => {
                let call_origin = stack
                    .pop()
                    .unwrap_or_else(|| panic!("unmatched return step in witness: {witness:#?}"));
                assert_eq!(
                    call_origin, step.origin,
                    "return step must close the call of the same call site"
                );
            }
            _ => {}
        }
    }
    assert!(
        stack.is_empty(),
        "witness ends inside an unreturned call: {witness:#?}"
    );
}

#[test]
fn direct_nested_call_witness_is_complete_and_untruncated() {
    let project = nested_project();
    let (outcome, _workspace) = evaluate_nested(&project);

    let [retained] = outcome.taint_analysis_results() else {
        panic!("expected one retained production analysis");
    };
    for finding in retained.report().findings() {
        let origins = finding.origins();
        assert!(!origins.witness_truncated(), "{origins:#?}");
        assert_eq!(origins.witness_truncation_cause(), None);
        assert!(!origins.witness_unavailable());
        assert!(!origins.origin_truncated());
    }

    let findings = outcome.taint_findings();
    let [finding] = findings else {
        panic!("expected one public finding, got {findings:#?}");
    };
    assert_eq!(finding.origins.len(), 1, "one source origin");
    assert!(!finding.origins_truncated);
    assert!(!finding.witnesses_truncated, "{finding:#?}");
    assert!(!finding.witnesses.is_empty());
    for witness in &finding.witnesses {
        assert!(!witness.truncated, "{witness:#?}");
        assert_eq!(witness.omitted_steps_lower_bound, 0);
        assert!(!witness.retention_truncated);
        assert_eq!(witness.truncation_cause, None);
        assert!(!witness.steps.is_empty());
        assert_matched_calls_and_returns(witness);

        let mut seen = Vec::new();
        for step in &witness.steps {
            assert!(
                !seen.contains(&step),
                "witness repeats a step (artificial cycle): {witness:#?}"
            );
            seen.push(step);
        }
    }

    // The policy projection must agree: no witness_truncated incomplete
    // reason and no fabricated omitted step.
    for run in outcome.report().runs() {
        for policy_finding in run.findings() {
            for witness in policy_finding.witnesses() {
                assert!(!witness.truncated(), "{witness:#?}");
                assert_eq!(witness.omitted_steps_lower_bound(), 0);
            }
        }
    }
    let mut rendered = Vec::new();
    write_policy_json(outcome.report(), &mut rendered, usize::MAX).expect("policy JSON rendering");
    let rendered = String::from_utf8(rendered).expect("utf-8 policy JSON");
    assert!(
        !rendered.contains("witness_truncated"),
        "policy JSON must not report witness truncation for a complete witness"
    );
}

#[test]
fn alternatives_remain_reported_without_witness_truncation() {
    let project = nested_project();
    let (outcome, _workspace) = evaluate_nested(&project);
    let [finding] = outcome.taint_findings() else {
        panic!("expected one public finding");
    };
    // Production retention keeps one alternative per quality, and competing
    // same-quality derivations exist (continuation edge versus matched
    // call/return), so the orthogonal signal stays visible while the witness
    // itself stays complete.
    assert!(
        finding
            .witnesses
            .iter()
            .all(|witness| witness.alternatives_truncated && !witness.truncated),
        "{:#?}",
        finding.witnesses
    );
}

#[test]
fn reconstruction_and_collection_limits_keep_their_exact_cause() {
    let project = nested_project();
    let (outcome, workspace) = evaluate_nested(&project);
    let [retained] = outcome.taint_analysis_results() else {
        panic!("expected one retained production analysis");
    };
    let plan = retained.plan();
    let provider = workspace.icfg_provider();

    let solve = || {
        let mut budget = SolverBudget::default();
        let cancellation = CancellationToken::new();
        let mut request = DataflowRequest::new(&mut budget, &cancellation);
        let mut semantic_budget = SemanticBudget::default();
        solve_taint_batch_with_witnesses(
            plan.value_flow().root(),
            &provider,
            plan,
            WitnessRetentionLimits::new(4).expect("strict retention"),
            &mut semantic_budget,
            &mut request,
        )
        .expect("strict-retention taint solve")
    };

    // A two-step reconstruction limit cannot hold the nine-step witness: the
    // exact reconstruction cause must survive into the origin status.
    let report = collect_taint_findings_with_limits(
        plan,
        solve(),
        8,
        WitnessReconstructionLimits::new(2, 4_096).expect("tiny step limit"),
        TaintFindingCollectionLimits::new(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
        )
        .expect("unbounded collection"),
    )
    .expect("collection under tiny reconstruction limits");
    let statuses = report
        .findings()
        .iter()
        .map(|finding| finding.origins())
        .filter(|origins| origins.witness_truncated())
        .collect::<Vec<_>>();
    assert!(!statuses.is_empty(), "{report:#?}");
    for origins in statuses {
        assert_eq!(
            origins.witness_truncation_cause(),
            Some(TaintWitnessTruncationCause::Reconstruction(
                WitnessTruncationCause::ReconstructionStepLimit
            )),
        );
    }

    // A one-witness collection budget refuses later reconstructions: the
    // collection cause must survive and the report must stay incomplete.
    let report = collect_taint_findings_with_limits(
        plan,
        solve(),
        8,
        WitnessReconstructionLimits::default(),
        TaintFindingCollectionLimits::new(usize::MAX, 1, usize::MAX, usize::MAX, usize::MAX)
            .expect("one-witness collection budget"),
    )
    .expect("collection under one-witness budget");
    assert!(!report.is_complete());
    assert!(
        report.findings().iter().any(|finding| {
            finding.origins().witness_truncation_cause()
                == Some(TaintWitnessTruncationCause::CollectionBudget)
        }),
        "{report:#?}"
    );
}
