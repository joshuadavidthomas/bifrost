//! Issue #1987: JS/TS receiverless local calls bind completely, and only
//! genuinely dynamic call shapes keep the production run inconclusive.
//!
//! The positive control proves the exact local-call case completes on the
//! production `.rqlp` route. Each near miss changes one call fact -- a
//! reassigned function name, an unknown-receiver call, a computed member
//! call, or competing direct imports -- and must stay honestly incomplete.

use brokk_bifrost::AnalyzerConfig;
use brokk_bifrost::Language;
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyRunCompletion, PolicySourceIdentity, evaluate_policy_inputs_with_analyzer,
};

use crate::common::InlineTestProject;

/// The DataFlowBench `core-direct.rqlp` policy shape from issue #1951.
fn core_direct_policy(id: &str) -> String {
    policy_with_call_modeling(id, "optimistic")
}

fn policy_with_call_modeling(id: &str, unmodeled: &str) -> String {
    format!(
        r#"(policy
  :schema-version 1
  :id "{id}"
  :name "Receiverless call binding"
  :message "Controlled input reaches the direct benchmark sink"
  :severity warning
  :analysis (analysis :type taint :mode may
    :call-modeling (call-modeling :unmodeled {unmodeled})
    :sources (endpoint-set :entries [(source :id input :display-name "benchmark input" :categories [input.user-controlled] :selector (rql :schema-version 1 (call :callee (name "dfb_source"))) :bind return-value :labels [attacker-controlled])])
    :sinks (endpoint-set :entries [(sink :id sink :display-name "benchmark sink" :categories [data.sensitive] :selector (rql :schema-version 1 (call :callee (name "dfb_sink"))) :dangerous-operand (argument :index 0) :accepts [attacker-controlled])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Test" :id "BROAD-TAINT")))"#
    )
}

fn evaluate(files: &[(&str, &str)], id: &str) -> PolicyBatchOutcome {
    evaluate_with_policy(files, id, core_direct_policy(id))
}

fn evaluate_with_policy(files: &[(&str, &str)], id: &str, policy: String) -> PolicyBatchOutcome {
    let mut project = InlineTestProject::with_language(Language::JavaScript);
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let input = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new(format!("test:{id}.rqlp")),
        policy,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 12).expect("fixed evaluation date"),
    );
    evaluate_policy_inputs_with_analyzer(project.root(), &input, &workspace, &options, None)
        .expect("production taint evaluation")
}

fn assert_complete_with_one_finding(outcome: &PolicyBatchOutcome, name: &str) {
    let run = &outcome.report().runs()[0];
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Complete),
        "{name} completion: {:?}, diagnostics: {:?}",
        run.completion(),
        run.diagnostics()
    );
    assert_eq!(
        run.findings().len(),
        1,
        "{name} findings: {:?}",
        run.diagnostics()
    );
    let findings = outcome.taint_findings();
    assert_eq!(findings.len(), 1, "{name} public findings");
    for witness in &findings[0].witnesses {
        assert!(!witness.truncated, "{name} witness truncated: {witness:#?}");
    }
}

fn assert_inconclusive(outcome: &PolicyBatchOutcome, name: &str) {
    let run = &outcome.report().runs()[0];
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Inconclusive { .. }),
        "{name} completion: {:?}, diagnostics: {:?}",
        run.completion(),
        run.diagnostics()
    );
}

/// Positive control: exact receiverless calls to local functions complete
/// with one definite finding.
#[test]
fn proven_local_function_calls_complete() {
    let outcome = evaluate(
        &[(
            "app.js",
            r#"
function dfb_source() {
  return "tainted";
}

function dfb_sink(value) {}

function run() {
  dfb_sink(dfb_source());
}
"#,
        )],
        "test.issue-1987.local",
    );
    assert_complete_with_one_finding(&outcome, "proven-local");
}

/// A module-level reassignment makes the callee binding genuinely dynamic.
#[test]
fn reassigned_function_name_stays_inconclusive() {
    let outcome = evaluate(
        &[(
            "app.js",
            r#"
function dfb_source() {
  return "tainted";
}

function dfb_sink(value) {}

dfb_sink = function (value) {};

function run() {
  dfb_sink(dfb_source());
}
"#,
        )],
        "test.issue-1987.reassigned",
    );
    assert_inconclusive(&outcome, "reassigned-name");
}

/// A property call on an unknown receiver keeps a real dispatch gap.
#[test]
fn unknown_receiver_call_stays_inconclusive() {
    let outcome = evaluate(
        &[(
            "app.js",
            r#"
function dfb_source() {
  return "tainted";
}

function run(handlers) {
  handlers.dfb_sink(dfb_source());
}
"#,
        )],
        "test.issue-1987.unknown-receiver",
    );
    assert_inconclusive(&outcome, "unknown-receiver");
}

const COMPUTED_MEMBER_SOURCE: &str = r#"
function dfb_source() {
  return "tainted";
}

function dfb_sink(value) {}

function run(table) {
  table["dfb_sink"](dfb_source());
}
"#;

/// A computed member call selects no structural sink row: the name selector
/// cannot see through `table["dfb_sink"]`. The unresolved call itself is
/// covered by the policy's declared unmodeled-call semantics, so the run
/// completes clean under both optimistic and paranoid modeling. The match is
/// name-based; the clean result identifies no structural sink, and it is not
/// proof about runtime targets of the computed call.
#[test]
fn computed_member_call_selects_no_sink_and_completes_clean() {
    for unmodeled in ["optimistic", "paranoid"] {
        let id = format!("test.issue-1987.computed-member-{unmodeled}");
        let outcome = evaluate_with_policy(
            &[("app.js", COMPUTED_MEMBER_SOURCE)],
            &id,
            policy_with_call_modeling(&id, unmodeled),
        );
        let run = &outcome.report().runs()[0];
        assert!(
            matches!(run.completion(), PolicyRunCompletion::Complete),
            "computed-member {unmodeled} completion: {:?}",
            run.completion()
        );
        assert!(
            run.findings().is_empty(),
            "computed-member {unmodeled} findings: {:?}",
            run.findings()
        );
    }
}

/// Competing direct imports of the callee name keep the call ambiguous.
#[test]
fn ambiguous_import_stays_inconclusive() {
    let outcome = evaluate(
        &[
            (
                "app.js",
                r#"
import { dfb_sink } from "./first.js";
import { dfb_sink } from "./second.js";

function dfb_source() {
  return "tainted";
}

export function run() {
  dfb_sink(dfb_source());
}
"#,
            ),
            ("first.js", "export function dfb_sink(value) {}\n"),
            ("second.js", "export function dfb_sink(value) {}\n"),
        ],
        "test.issue-1987.ambiguous-import",
    );
    assert_inconclusive(&outcome, "ambiguous-import");
}
