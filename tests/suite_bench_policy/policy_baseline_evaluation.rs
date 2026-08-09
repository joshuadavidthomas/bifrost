//! Bulk baseline acceptance semantics against the library entry point (#1881).

use std::fs;
use std::path::{Path, PathBuf};

use crate::common::InlineTestProject;
use brokk_bifrost::Language;
use brokk_bifrost::policy::{
    POLICY_EXIT_CLEAN, POLICY_EXIT_FINDING, POLICY_EXIT_UNRELIABLE, PolicyBaselineDocument,
    PolicyBaselineMatchState, PolicyBatchOutcome, PolicyEvaluationOptions, PolicyFailOn,
    PolicyReportDiagnosticCode, PolicySuppressionPolicyHashState, evaluate_policy_files,
};
use serde_json::{Value, json};

const POLICY_PATH: &str = "policies/dynamic-eval.rqlp";
const DYNAMIC_EVAL_POLICY: &str =
    include_str!("../fixtures/policy-cli/project/policies/dynamic-eval.rqlp");

fn project(source: &str) -> crate::common::BuiltInlineTestProject {
    InlineTestProject::with_language(Language::Python)
        .file("src/app.py", source)
        .file(POLICY_PATH, DYNAMIC_EVAL_POLICY)
        .build()
}

fn evaluate(root: &Path, fail_on: PolicyFailOn) -> PolicyBatchOutcome {
    evaluate_policy_files(
        root,
        &[PathBuf::from(POLICY_PATH)],
        &PolicyEvaluationOptions::new("2026-08-08".parse().expect("fixed test date"))
            .with_fail_on(fail_on),
    )
    .expect("policy evaluation")
}

fn write_baseline(root: &Path, document: &str) {
    let path = root.join(".bifrost/baseline.json");
    fs::create_dir_all(path.parent().expect("baseline parent")).expect("create .bifrost");
    fs::write(path, document).expect("write baseline");
}

/// Accept the current findings the way `--accept-current` does: from the
/// completed report, strong unclaimed identities only.
fn accept_current(root: &Path) -> PolicyBaselineDocument {
    let outcome = evaluate(root, PolicyFailOn::Never);
    assert_eq!(outcome.exit_status(), POLICY_EXIT_CLEAN);
    let (document, weak_excluded) = PolicyBaselineDocument::from_completed_report(
        outcome.report(),
        "Onboarding acceptance",
        Some("platform-team"),
        "2026-08-08".parse().expect("fixed acceptance date"),
    )
    .expect("baseline generation");
    assert_eq!(weak_excluded, 0);
    write_baseline(root, &document.to_canonical_json());
    document
}

#[test]
fn accepted_findings_stay_reported_with_decisions_and_stop_gating() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    let baseline = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(baseline.exit_status(), POLICY_EXIT_FINDING);
    assert!(baseline.report().baseline().is_none());
    let encoded = serde_json::to_value(baseline.report()).expect("report JSON");
    assert!(
        encoded.get("baseline").is_none(),
        "no baseline key without a document"
    );
    assert!(
        encoded["runs"][0]["findings"][0].get("baseline").is_none(),
        "no per-finding baseline key without a document"
    );

    let document = accept_current(project.root());
    assert_eq!(document.entry_count(), 1);

    let accepted = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(accepted.exit_status(), POLICY_EXIT_CLEAN);
    assert_eq!(accepted.report().schema_version(), 3);
    let review = accepted.report().baseline().expect("baseline review");
    assert_eq!(review.document_path(), ".bifrost/baseline.json");
    assert_eq!(review.reason(), "Onboarding acceptance");
    assert_eq!(review.accepted_by(), Some("platform-team"));
    assert_eq!(review.entry_count(), 1);
    assert_eq!(review.applied_count(), 1);
    assert_eq!(review.drifted_count(), 0);
    assert_eq!(review.stale_count(), 0);
    assert_eq!(review.result_omitted_count(), 0);
    assert!(
        review.entries().is_empty(),
        "applied-matching is not notable"
    );
    assert!(!review.entries_truncated());

    let finding = &accepted.report().runs()[0].findings()[0];
    let decision = finding.baseline().expect("baseline decision");
    assert_eq!(decision.reason(), "Onboarding acceptance");
    assert_eq!(
        decision.policy_hash_state(),
        PolicySuppressionPolicyHashState::Matching
    );

    // A new finding introduced after acceptance still gates.
    fs::write(
        project.root().join("src/extra.py"),
        "def run_more(user_code):\n    return eval(user_code)\n",
    )
    .expect("new offending source");
    let regressed = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(regressed.exit_status(), POLICY_EXIT_FINDING);
    let (baselined, gating): (Vec<_>, Vec<_>) = regressed.report().runs()[0]
        .findings()
        .iter()
        .partition(|finding| finding.baseline().is_some());
    assert_eq!(baselined.len(), 1);
    assert_eq!(gating.len(), 1);
    assert_eq!(gating[0].primary().path(), "src/extra.py");
}

#[test]
fn policy_edit_marks_entries_drifted_without_reactivating() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    accept_current(project.root());

    let changed_policy = DYNAMIC_EVAL_POLICY
        .replace(
            ":message \"Dynamic evaluation is forbidden\"",
            ":message \"Dynamic evaluation requires review\"",
        )
        .replace(":severity warning", ":severity error");
    assert_ne!(changed_policy, DYNAMIC_EVAL_POLICY);
    fs::write(project.root().join(POLICY_PATH), changed_policy).expect("edit policy");

    let drifted = evaluate(project.root(), PolicyFailOn::Error);
    assert_eq!(drifted.exit_status(), POLICY_EXIT_CLEAN);
    let review = drifted.report().baseline().expect("baseline review");
    assert_eq!(review.applied_count(), 1);
    assert_eq!(review.drifted_count(), 1);
    assert_eq!(review.entries().len(), 1, "a drifted entry is notable");
    let entry = &review.entries()[0];
    assert_eq!(entry.match_state(), PolicyBaselineMatchState::StrongFinding);
    assert_eq!(
        entry.policy_hash_state(),
        PolicySuppressionPolicyHashState::Drifted
    );
    assert!(entry.applied());
    assert_eq!(
        drifted.report().runs()[0].findings()[0]
            .baseline()
            .expect("still applied")
            .policy_hash_state(),
        PolicySuppressionPolicyHashState::Drifted
    );
}

#[test]
fn source_edit_re_keys_the_finding_and_marks_the_entry_stale() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    accept_current(project.root());

    fs::write(
        project.root().join("src/app.py"),
        "def run(user_code):\n    return eval(user_code.strip())\n",
    )
    .expect("edit selected source");
    let changed = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(changed.exit_status(), POLICY_EXIT_FINDING);
    assert!(
        changed.report().runs()[0].findings()[0]
            .baseline()
            .is_none()
    );
    let review = changed.report().baseline().expect("baseline review");
    assert_eq!(review.applied_count(), 0);
    assert_eq!(review.stale_count(), 1);
    let entry = &review.entries()[0];
    assert_eq!(entry.match_state(), PolicyBaselineMatchState::FindingAbsent);
    assert!(entry.stale());
    assert!(!entry.applied());
}

#[test]
fn suppression_claims_the_finding_before_the_baseline() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    let baseline = evaluate(project.root(), PolicyFailOn::Warning);
    let rule = &baseline.report().rules()[0];
    let finding = &baseline.report().runs()[0].findings()[0];
    let (policy_id, policy_hash, finding_id) = (
        rule.policy_id().as_str().to_string(),
        rule.policy_hash().to_string(),
        finding.id().to_string(),
    );
    accept_current(project.root());

    let suppression_path = project.root().join(".bifrost/suppressions.json");
    fs::write(
        &suppression_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "suppressions": [{
                "policy_id": policy_id,
                "finding_id": finding_id,
                "identity_stability": "strong",
                "status": "accepted",
                "reason": "Reviewed compatibility boundary",
                "policy_hash_at_acceptance": policy_hash,
                "accepted_by": "security-review",
                "accepted_at": "2026-08-01",
                "expires_at": null,
            }],
        }))
        .expect("suppression JSON"),
    )
    .expect("write suppressions");

    let outcome = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(outcome.exit_status(), POLICY_EXIT_CLEAN);
    let finding = &outcome.report().runs()[0].findings()[0];
    assert!(finding.suppression().is_some());
    assert!(
        finding.baseline().is_none(),
        "a suppressed finding is not claimed by the baseline"
    );
    let review = outcome.report().baseline().expect("baseline review");
    assert_eq!(review.applied_count(), 0);
    assert_eq!(review.claimed_count(), 1);
    let entry = &review.entries()[0];
    assert_eq!(
        entry.match_state(),
        PolicyBaselineMatchState::FindingClaimed
    );
    assert!(!entry.stale(), "a claimed finding is present, not stale");
}

#[test]
fn malformed_or_unselected_baselines_have_typed_outcomes() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    write_baseline(project.root(), "{ not json");
    let malformed = evaluate(project.root(), PolicyFailOn::Warning);
    assert_eq!(malformed.exit_status(), POLICY_EXIT_UNRELIABLE);
    assert!(malformed.report().baseline().is_none());
    assert!(malformed.report().runs()[0].findings().len() == 1);
    assert!(
        malformed.report().diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == PolicyReportDiagnosticCode::BaselineLoadFailed
        })
    );

    // An entry for a policy outside the selection is audited, never claimed.
    write_baseline(
        project.root(),
        &json!({
            "schema_version": 1,
            "reason": "Onboarding acceptance",
            "accepted_at": "2026-08-08",
            "policies": [{
                "policy_id": "test.not-selected",
                "finding_ids": ["1".repeat(64)],
            }],
        })
        .to_string(),
    );
    let unselected = evaluate(project.root(), PolicyFailOn::Never);
    let review = unselected.report().baseline().expect("baseline review");
    assert_eq!(review.policy_not_evaluated_count(), 1);
    assert_eq!(
        review.entries()[0].match_state(),
        PolicyBaselineMatchState::PolicyNotEvaluated
    );
    assert_eq!(
        review.entries()[0].policy_hash_state(),
        PolicySuppressionPolicyHashState::Unknown
    );
    assert!(!review.entries()[0].stale());
}

#[test]
fn generation_excludes_suppressed_findings_and_counts_weak_ones() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    let baseline = evaluate(project.root(), PolicyFailOn::Warning);
    let rule = &baseline.report().rules()[0];
    let finding = &baseline.report().runs()[0].findings()[0];
    let suppression_path = project.root().join(".bifrost/suppressions.json");
    fs::create_dir_all(suppression_path.parent().unwrap()).unwrap();
    fs::write(
        &suppression_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "suppressions": [{
                "policy_id": rule.policy_id().as_str(),
                "finding_id": finding.id().to_string(),
                "identity_stability": "strong",
                "status": "accepted",
                "reason": "Reviewed compatibility boundary",
                "policy_hash_at_acceptance": rule.policy_hash().to_string(),
                "accepted_by": "security-review",
                "accepted_at": "2026-08-01",
                "expires_at": null,
            }],
        }))
        .expect("suppression JSON"),
    )
    .expect("write suppressions");

    let outcome = evaluate(project.root(), PolicyFailOn::Never);
    assert_eq!(outcome.exit_status(), POLICY_EXIT_CLEAN);
    let (document, weak_excluded) = PolicyBaselineDocument::from_completed_report(
        outcome.report(),
        "Onboarding acceptance",
        None,
        "2026-08-08".parse().expect("fixed acceptance date"),
    )
    .expect("baseline generation");
    assert_eq!(weak_excluded, 0);
    assert_eq!(
        document.entry_count(),
        0,
        "a suppressed finding stays governed by its suppression"
    );
}

#[test]
fn baseline_composes_with_diff_mode_gating() {
    // Committed content carries one accepted finding; a working-tree edit
    // introduces a second one. With both a baseline and a diff base, only the
    // new unclaimed finding gates, and it gates exactly once.
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    accept_current(project.root());
    crate::common::init_git_repo_with_identity(project.root());
    crate::common::run_git(project.root(), &["add", "."]);
    crate::common::run_git(project.root(), &["commit", "-m", "base"]);

    fs::write(
        project.root().join("src/extra.py"),
        "def run_more(user_code):\n    return eval(user_code)\n",
    )
    .expect("new offending source");

    let options = PolicyEvaluationOptions::new("2026-08-08".parse().expect("fixed test date"))
        .with_fail_on(PolicyFailOn::Warning)
        .with_diff_base("HEAD".to_string());
    let outcome = evaluate_policy_files(project.root(), &[PathBuf::from(POLICY_PATH)], &options)
        .expect("diff evaluation");
    assert_eq!(outcome.exit_status(), POLICY_EXIT_FINDING);
    let report = serde_json::to_value(outcome.report()).expect("report JSON");
    assert_eq!(report["diff"]["new_count"], 1);
    assert_eq!(report["diff"]["persisting_count"], 1);
    assert_eq!(report["baseline"]["applied_count"], 1);
    let findings = report["runs"][0]["findings"].as_array().expect("findings");
    let baselined = findings
        .iter()
        .filter(|finding| finding.get("baseline").is_some())
        .collect::<Vec<_>>();
    assert_eq!(baselined.len(), 1);
    assert_eq!(baselined[0]["diff"]["disposition"], "persisting");

    // Removing the new finding restores a clean diff run under the baseline.
    fs::remove_file(project.root().join("src/extra.py")).expect("remove new source");
    let clean = evaluate_policy_files(
        project.root(),
        &[PathBuf::from(POLICY_PATH)],
        &PolicyEvaluationOptions::new("2026-08-08".parse().expect("fixed test date"))
            .with_fail_on(PolicyFailOn::Warning)
            .with_diff_base("HEAD".to_string()),
    )
    .expect("clean diff evaluation");
    assert_eq!(clean.exit_status(), POLICY_EXIT_CLEAN);
}

#[test]
fn baseline_json_shape_is_deterministic_across_runs() {
    let project = project("def run(user_code):\n    return eval(user_code)\n");
    accept_current(project.root());
    let first = evaluate(project.root(), PolicyFailOn::Warning);
    let second = evaluate(project.root(), PolicyFailOn::Warning);
    let scrub = |outcome: &PolicyBatchOutcome| -> Value {
        let mut value = serde_json::to_value(outcome.report()).expect("report JSON");
        value
            .as_object_mut()
            .expect("report object")
            .remove("execution");
        for run in value["runs"].as_array_mut().expect("runs") {
            run.as_object_mut().expect("run object").remove("work");
        }
        value
    };
    assert_eq!(scrub(&first), scrub(&second));
    assert_eq!(
        scrub(&first)["baseline"]["accepted_at"],
        json!("2026-08-08")
    );
}
