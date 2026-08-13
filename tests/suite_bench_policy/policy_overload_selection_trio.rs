//! The #1478 Milestone 5 policy trio: finding, clean, unreliable.
//!
//! This is the issue's observable acceptance stated as executable policy. One
//! invariant -- a call site must select exactly one callable its own
//! applicability check accepted -- reports a seeded overload defect as a single
//! multi-location finding naming the call site *and* both competing callable
//! declarations, is clean on the corrected fixture, and reports the same run
//! `unreliable` rather than clean when the argument list it counts is one the
//! analyzer could not read.
//!
//! The third result is the one that is easy to get wrong. A macro-expanded C
//! argument list parses as an ordinary call, so the only honest signal is the
//! mandatory call-shape row's `coverage`; the derivation deliberately emits no
//! argument rows for such a site (#1478 Milestone 1), and a cardinality
//! assertion that counted those absent rows would pass. Binding the mandatory
//! shape row is what keeps that from happening, and the last test proves the
//! same policy still concludes on a shape the analyzer can read.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::common::InlineTestProject;
use brokk_bifrost::policy::{
    CatalogRegistryLimits, DefaultPolicyEvaluator, HumanRenderColor, HumanRenderDetail,
    HumanRenderOptions, PolicyBudget, PolicyEvaluationContext, PolicyEvaluationDate,
    PolicyEvaluationOptions, PolicyEvaluator, PolicyFindingEvidence, PolicyIncompleteReason,
    PolicyLocationRelationship, PolicyRegistry, PolicyRegistryLimits, PolicyRun,
    PolicyRunCompletion, PolicySourceIdentity, SarifToolIdentity, TaintCatalogRegistry,
    evaluate_policy_files, write_policy_human, write_policy_json, write_policy_sarif,
};
use serde_json::Value;

/// The invariant the issue exists to make writable: for one call site, exactly
/// one candidate is both what the resolver selected and what the resolver's own
/// applicability check accepted.
///
/// The third binding and its join are what turn the finding into a
/// multi-location one. `candidate_id` carries the candidate declaration's
/// identity, so an applicability row joins to the `callable_signature` row of
/// the very callable it judged, and the violated group's contributing rows then
/// include each competing declaration at its own source range.
const UNIQUE_APPLICABLE_OVERLOAD: &str = r#"(policy
  :id "test.overload.unique-applicable"
  :name "Every call selects exactly one applicable overload"
  :message "a call site must select exactly one callable its own applicability check accepted"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query
      (rql (overload-selection (occurrences :role [member_position]))))
    (bind :name cand :query
      (rql (callable-applicability (occurrences :role [member_position]))))
    (bind :name sig :query
      (rql (callable-signature (enclosing-decl (language java (method :name "render"))))))
    (join :left site :right cand :on ((site_ast_id site_ast_id)))
    (join :left cand :right sig :on ((candidate_id declaration_id)))
    (group :name by-site :by (site.site_ast_id)
      (aggregate :name winners :op count
        :where ((cand.selected eq true) (cand.verdict eq applicable))))
    (assert :group by-site :value winners :cardinality (exactly 1))))"#;

/// Seeded defect: two overloads accept the same argument count, so the call
/// means either of them and the resolver keeps both. Java's own arity check
/// cannot separate them, which is precisely the wrong-overload risk the mined
/// bug-fix commits kept producing.
const SEEDED_AMBIGUOUS_OVERLOAD: &str = r#"package app;

public class Widget {
    int render(int width) { return width; }
    int render(String label) { return label.length(); }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#;

/// The same fixture with the ambiguity removed: the competitor now needs a
/// second argument, so exactly one overload accepts the call.
const CORRECTED_OVERLOAD: &str = r#"package app;

public class Widget {
    int render(int width) { return width; }
    int render(String label, int width) { return width; }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#;

fn java_or_cpp_workspace(
    path: &str,
    source: &str,
) -> (
    crate::common::BuiltInlineTestProject,
    brokk_bifrost::analyzer::WorkspaceAnalyzer,
) {
    let project = InlineTestProject::new().file(path, source).build();
    let workspace = brokk_bifrost::analyzer::WorkspaceAnalyzer::build(
        project.project_dyn(),
        brokk_bifrost::AnalyzerConfig::default(),
    );
    (project, workspace)
}

fn evaluate(source: &str, workspace: &brokk_bifrost::analyzer::WorkspaceAnalyzer) -> PolicyRun {
    let catalogs = Arc::new(TaintCatalogRegistry::new_without_workspace(
        CatalogRegistryLimits::default(),
    ));
    let mut registry =
        PolicyRegistry::new_without_workspace(catalogs, PolicyRegistryLimits::default());
    registry
        .register_policy_bytes(
            PolicySourceIdentity::new("test:overload-trio"),
            source.as_bytes(),
        )
        .expect("valid relational assertion policy");
    let policy = registry.policies().next().expect("one policy");
    DefaultPolicyEvaluator::new()
        .evaluate(
            policy,
            &PolicyEvaluationContext {
                analyzer: workspace.analyzer(),
                workspace: Some(workspace),
                cancellation: None,
                cvss_overlays: &[],
                organizational_risk: &[],
            },
            &mut PolicyBudget::default(),
        )
        .expect("relational assertion evaluation")
}

/// Finding. The violated site is one finding, and it names three places: the
/// call whose meaning is undecided, and both callable declarations that
/// competed for it. A finding that pointed only at the call would leave a
/// reader to find the overload set themselves.
#[test]
fn a_seeded_overload_defect_is_one_multi_location_finding() {
    let (_project, workspace) = java_or_cpp_workspace("app/Widget.java", SEEDED_AMBIGUOUS_OVERLOAD);
    let run = evaluate(UNIQUE_APPLICABLE_OVERLOAD, &workspace);

    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let finding = &run.findings()[0];

    assert_eq!(finding.primary().path(), "app/Widget.java");
    assert_eq!(
        finding
            .primary()
            .region()
            .expect("a relational violation anchors at its row's exact range")
            .start_line(),
        8,
        "the finding is anchored at the call, not at either declaration"
    );

    let PolicyFindingEvidence::Assertion { evidence } = finding.evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.expectation(), "(exactly 1)");
    assert_eq!(
        evidence.actual_count(),
        2,
        "both overloads were selected and applicable"
    );

    let lines = finding
        .related()
        .iter()
        .filter_map(|related| {
            related
                .location()
                .region()
                .map(|region| region.start_line())
        })
        .collect::<Vec<_>>();
    assert!(
        lines.contains(&4),
        "the selected callable is a location of the finding: {lines:?}"
    );
    assert!(
        lines.contains(&5),
        "the applicable competitor is a location of the finding: {lines:?}"
    );
    assert!(
        lines.contains(&8),
        "the call site is a location of the finding: {lines:?}"
    );
    assert!(
        finding
            .related()
            .iter()
            .any(|related| related.relationship() == PolicyLocationRelationship::Subject),
        "{:?}",
        finding.related()
    );
    assert!(!finding.related_truncated());
}

/// Clean. The corrected fixture satisfies the same invariant, and the pass is a
/// satisfied assertion rather than an absent group: inverting the cardinality
/// turns the same fixture into a finding, which is what proves the clean result
/// is not vacuous.
#[test]
fn the_corrected_overload_set_is_clean_and_provably_non_vacuous() {
    let (_project, workspace) = java_or_cpp_workspace("app/Widget.java", CORRECTED_OVERLOAD);
    let run = evaluate(UNIQUE_APPLICABLE_OVERLOAD, &workspace);
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());

    let inverted = UNIQUE_APPLICABLE_OVERLOAD.replace(
        ":value winners :cardinality (exactly 1)",
        ":value winners :cardinality (exactly 0)",
    );
    let run = evaluate(&inverted, &workspace);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[0].evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(
        evidence.actual_count(),
        1,
        "the group existed and its single winner was counted"
    );
}

/// An exact-cardinality assertion over an argument list, written against the
/// mandatory call-shape row that states how much of that list the analyzer
/// could read.
const HELPER_TAKES_TWO_ARGUMENTS: &str = r#"(policy
  :id "test.overload.two-arguments"
  :name "Every call of the helper passes two arguments"
  :message "the helper declares two parameters and every call must fill them"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name shape :query
      (rql (call-shape (language cpp (call :callee "CALLEE")))))
    (bind :name arg :query
      (rql (call-arguments (call-argument-groups
        (call-shape (language cpp (call :callee "CALLEE")))))))
    (join :left shape :right arg :on ((site_id site_id)))
    (group :name by-site :by (shape.site_id)
      (aggregate :name args :op count))
    (assert :group by-site :value args :cardinality (exactly 2))))"#;

/// A call written through a function-like macro of the same translation unit.
/// The argument list the callee actually receives is the macro body's, which
/// the analyzer cannot see from the site.
const CPP_MACRO_DERIVED: &str = r#"#define CALL_TWICE(a) helper(a, 1)
int helper(int a, int b) { return a + b; }
int caller() { return CALL_TWICE(2); }
"#;

/// Unreliable. The macro-derived site has no argument rows at all, so counting
/// them would pass the exact-cardinality assertion while proving nothing. The
/// run is inconclusive instead, and produces no finding either way -- an
/// unreadable shape is never a clean verdict and never a violation.
#[test]
fn a_macro_derived_argument_list_is_unreliable_rather_than_clean() {
    let (_project, workspace) = java_or_cpp_workspace("app.cpp", CPP_MACRO_DERIVED);
    let run = evaluate(
        &HELPER_TAKES_TWO_ARGUMENTS.replace("CALLEE", "CALL_TWICE"),
        &workspace,
    );
    match run.completion() {
        PolicyRunCompletion::Inconclusive { reasons } => assert!(
            reasons.contains(&PolicyIncompleteReason::CapabilityIncomplete),
            "{reasons:?}"
        ),
        other => panic!("a macro-derived shape must never conclude: {other:?}"),
    }
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// The same policy still concludes where the shape is readable, which is what
/// makes the unreliable result above a statement about that one site rather
/// than about the policy. A call that fills the list is clean; a call that does
/// not is a finding.
#[test]
fn the_same_assertion_concludes_on_a_readable_argument_list() {
    let policy = HELPER_TAKES_TWO_ARGUMENTS.replace("CALLEE", "helper");

    let (_project, workspace) = java_or_cpp_workspace(
        "app.cpp",
        "int helper(int a, int b) { return a + b; }\nint caller() { return helper(2, 1); }\n",
    );
    let run = evaluate(&policy, &workspace);
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());

    let (_project, workspace) = java_or_cpp_workspace(
        "app.cpp",
        "int helper(int a, int b) { return a + b; }\nint caller() { return helper(2); }\n",
    );
    let run = evaluate(&policy, &workspace);
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[0].evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.actual_count(), 1);
}

/// The three renderings a consumer actually reads must describe the same
/// finding. Human text, JSON and SARIF are produced from one report document,
/// so this pins that each projection keeps the finding's identity, its severity
/// and every one of its locations rather than dropping the related ones.
#[test]
fn human_json_and_sarif_agree_about_the_overload_finding() {
    let workspace_dir = tempfile::tempdir().expect("temporary workspace");
    fs::create_dir_all(workspace_dir.path().join("app")).expect("source directory");
    fs::create_dir_all(workspace_dir.path().join("policies")).expect("policy directory");
    fs::write(
        workspace_dir.path().join("app/Widget.java"),
        SEEDED_AMBIGUOUS_OVERLOAD,
    )
    .expect("source fixture");
    fs::write(
        workspace_dir.path().join("policies/overload.rqlp"),
        UNIQUE_APPLICABLE_OVERLOAD,
    )
    .expect("policy fixture");

    let outcome = evaluate_policy_files(
        workspace_dir.path(),
        &[PathBuf::from("policies").join("overload.rqlp")],
        &PolicyEvaluationOptions::new(
            PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed test date"),
        ),
    )
    .expect("coordinated policy evaluation");
    let report = outcome.report();
    assert_eq!(report.runs().len(), 1);
    assert_eq!(
        report.runs()[0].findings().len(),
        1,
        "{:?}",
        report.runs()[0].findings()
    );
    let finding = &report.runs()[0].findings()[0];
    let finding_id = finding.id().to_string();
    let related_count = finding.related().len();

    let mut human = Vec::new();
    write_policy_human(
        report,
        &HumanRenderOptions::new(HumanRenderDetail::Verbose, HumanRenderColor::Plain),
        &mut human,
        usize::MAX,
    )
    .expect("human rendering");
    let human = String::from_utf8(human).expect("human output is utf-8");
    assert!(
        human.contains(
            "a call site must select exactly one callable its own applicability check accepted"
        ),
        "{human}"
    );
    assert!(human.contains("app/Widget.java"), "{human}");

    let mut json_bytes = Vec::new();
    write_policy_json(report, &mut json_bytes, usize::MAX).expect("json rendering");
    let json: Value = serde_json::from_slice(&json_bytes).expect("json output parses");
    let json_finding = &json["runs"][0]["findings"][0];
    assert_eq!(json_finding["id"], finding_id, "{json_finding}");
    assert_eq!(json_finding["severity"], "error", "{json_finding}");
    assert_eq!(
        json_finding["related"]
            .as_array()
            .expect("related locations")
            .len(),
        related_count,
        "{json_finding}"
    );

    let mut sarif_bytes = Vec::new();
    write_policy_sarif(
        report,
        &SarifToolIdentity::new("bifrost"),
        &mut sarif_bytes,
        usize::MAX,
    )
    .expect("sarif rendering");
    let sarif: Value = serde_json::from_slice(&sarif_bytes).expect("sarif output parses");
    let sarif_result = &sarif["runs"][0]["results"][0];
    assert_eq!(
        sarif_result["ruleId"], "test.overload.unique-applicable",
        "{sarif_result}"
    );
    assert_eq!(sarif_result["level"], "error", "{sarif_result}");
    assert_eq!(
        sarif_result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "app/Widget.java",
        "{sarif_result}"
    );
    assert_eq!(
        sarif_result["relatedLocations"]
            .as_array()
            .expect("related locations")
            .len(),
        related_count,
        "every location the finding carries survives the SARIF projection: {sarif_result}"
    );
}
