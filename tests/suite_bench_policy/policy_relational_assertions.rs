//! End-to-end coverage for RQLP relational assertion plans (issues #1477 and
//! #1478).
//!
//! These tests execute named `bind` queries and typed row expansions through
//! the production `run_policy` evaluation path: every binding is a real
//! CodeQuery against a real analyzer snapshot, joins are typed row-field
//! equality, and each violated group becomes one finding anchored at the exact
//! source range of its contributing rows.
//!
//! The last two families are the #1478 Milestone 4 additions: the
//! `assert-selected-in-winning-tier` sugar over overload-selection and
//! callable-applicability rows, and the `ordered-equal` aggregate over two
//! ordered row sequences.

use std::sync::Arc;

use brokk_bifrost::analyzer::structural::CodeQueryExecutionLimits;

use crate::common::InlineTestProject;
use brokk_bifrost::policy::{
    CatalogRegistryLimits, DefaultPolicyEvaluator, PolicyAnalysisType, PolicyBudget,
    PolicyEvaluationContext, PolicyEvaluator, PolicyFindingEvidence, PolicyLocationRelationship,
    PolicyRegistry, PolicyRegistryLimits, PolicyRun, PolicyRunCompletion, PolicySourceIdentity,
    TaintCatalogRegistry,
};
use brokk_bifrost::{IAnalyzer, JavaAnalyzer, Language, TypescriptAnalyzer};

/// `render` is declared once and never read.
const CORRECT_SOURCE: &str = "export function render(): number {\n  return 1;\n}\n";

/// A second `render` identifier exists that is a plain value read.
const BUGGY_SOURCE: &str =
    "export function render(): number {\n  return 1;\n}\n\nexport const alias = render;\n";

/// Forbid value reads through the relational plan: bind every value-reference
/// occurrence, group by its AST identity, and require an exact zero count. A
/// group only exists where a read exists, so each read violates on its own
/// exact source range.
const FORBID_READS_RELATIONAL: &str = r#"(policy
  :id "test.relational.forbid-reads"
  :name "No value reads"
  :message "value reads are forbidden in this fixture"
  :severity warning
  :analysis (analysis
    :type assertion
    (bind :name read :query (rql (occurrences :role [value_reference])))
    (group :name by-read :by (read.ast_id)
      (aggregate :name reads :op count))
    (assert :group by-read :value reads :cardinality (exactly 0))))"#;

/// Every member-position occurrence must join to at least one mandatory
/// receiver outcome row. The anti-join keeps exactly the sites that have no
/// outcome row, and any surviving group is a violation.
const REQUIRE_RECEIVER_OUTCOME: &str = r#"(policy
  :id "test.relational.receiver-outcome"
  :name "Member sites have receiver outcomes"
  :message "every member occurrence must produce a receiver outcome row"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query (rql (occurrences :role [member_position])))
    (bind :name receiver :from site :step receiver-outcome)
    (join :left site :right receiver :kind anti :on ((ast_id site_ast_id)))
    (group :name orphaned :by (site.ast_id)
      (aggregate :name sites :op count))
    (assert :group orphaned :value sites :cardinality (exactly 0))))"#;

fn evaluate(source: &str, analyzer: &dyn IAnalyzer, budget: &mut PolicyBudget) -> PolicyRun {
    let catalogs = Arc::new(TaintCatalogRegistry::new_without_workspace(
        CatalogRegistryLimits::default(),
    ));
    let mut registry =
        PolicyRegistry::new_without_workspace(catalogs, PolicyRegistryLimits::default());
    registry
        .register_policy_bytes(
            PolicySourceIdentity::new("test:relational"),
            source.as_bytes(),
        )
        .expect("valid relational assertion policy");
    let policy = registry.policies().next().expect("one policy");
    DefaultPolicyEvaluator::new()
        .evaluate(
            policy,
            &PolicyEvaluationContext {
                analyzer,
                workspace: None,
                cancellation: None,
                cvss_overlays: &[],
                organizational_risk: &[],
            },
            budget,
        )
        .expect("relational assertion evaluation")
}

/// Evaluate against a full workspace snapshot, which the semantic row
/// families (the #1477 M4 dispatch expansions) require.
fn evaluate_with_workspace(
    source: &str,
    workspace: &brokk_bifrost::analyzer::WorkspaceAnalyzer,
    budget: &mut PolicyBudget,
) -> PolicyRun {
    let catalogs = Arc::new(TaintCatalogRegistry::new_without_workspace(
        CatalogRegistryLimits::default(),
    ));
    let mut registry =
        PolicyRegistry::new_without_workspace(catalogs, PolicyRegistryLimits::default());
    registry
        .register_policy_bytes(
            PolicySourceIdentity::new("test:relational"),
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
            budget,
        )
        .expect("relational assertion evaluation")
}

/// A Java workspace snapshot for the semantic row families.
fn java_workspace(
    source: &str,
) -> (
    crate::common::BuiltInlineTestProject,
    brokk_bifrost::analyzer::WorkspaceAnalyzer,
) {
    let project = InlineTestProject::with_language(Language::Java)
        .file("App.java", source)
        .build();
    let workspace = brokk_bifrost::analyzer::WorkspaceAnalyzer::build(
        project.project_dyn(),
        brokk_bifrost::AnalyzerConfig::default(),
    );
    (project, workspace)
}

fn typescript(source: &str) -> (crate::common::BuiltInlineTestProject, TypescriptAnalyzer) {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("widget.ts", source)
        .build();
    let analyzer = TypescriptAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

fn java(source: &str) -> (crate::common::BuiltInlineTestProject, JavaAnalyzer) {
    let project = InlineTestProject::with_language(Language::Java)
        .file("App.java", source)
        .build();
    let analyzer = JavaAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

#[test]
fn a_violated_relational_group_is_one_finding_with_exact_source_ranges() {
    let (_project, analyzer) = typescript(BUGGY_SOURCE);
    let run = evaluate(
        FORBID_READS_RELATIONAL,
        &analyzer,
        &mut PolicyBudget::default(),
    );

    assert_eq!(run.analysis_type(), PolicyAnalysisType::Assertion);
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(
        run.findings().len(),
        1,
        "only the single read violates; findings: {:?}",
        run.findings()
    );
    let finding = &run.findings()[0];
    assert_eq!(finding.primary().path(), "widget.ts");
    let region = finding
        .primary()
        .region()
        .expect("a relational violation anchors at the row's exact display range");
    assert_eq!(
        region.start_line(),
        5,
        "the finding points at the read of `render`, not its declaration"
    );
    assert!(
        finding.primary().byte_span().is_some(),
        "the violation retains the exact byte span of the offending row"
    );

    let PolicyFindingEvidence::Assertion { evidence } = finding.evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.assert_kind(), "relational");
    assert_eq!(evidence.expectation(), "(exactly 0)");
    assert_eq!(evidence.actual_count(), 1);
    assert_eq!(evidence.anchor().assert_id(), "by-read-reads");
    assert!(
        !evidence.anchor().subject_ast_id().is_empty(),
        "the anchor is keyed on the violated group key"
    );

    let relationships = finding
        .related()
        .iter()
        .map(|related| related.relationship())
        .collect::<Vec<_>>();
    assert!(
        relationships.contains(&PolicyLocationRelationship::Subject),
        "{relationships:?}"
    );
    assert!(!finding.related_truncated());
}

#[test]
fn the_corrected_fixture_is_clean_under_the_relational_plan() {
    let (_project, analyzer) = typescript(CORRECT_SOURCE);
    let run = evaluate(
        FORBID_READS_RELATIONAL,
        &analyzer,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(
        run.findings().is_empty(),
        "corrected fixture must be clean: {:?}",
        run.findings()
    );
}

#[test]
fn a_receiver_outcome_expansion_executes_and_covers_every_member_site() {
    let (_project, analyzer) = typescript(
        "class Widget {\n  render(): number {\n    return 1;\n  }\n}\n\nconst w = new Widget();\nexport const n = w.render();\n",
    );
    let run = evaluate(
        REQUIRE_RECEIVER_OUTCOME,
        &analyzer,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(
        run.findings().is_empty(),
        "every member occurrence has one mandatory receiver outcome row: {:?}",
        run.findings()
    );
}

/// The #1477 member-selection invariant: every member occurrence selects
/// exactly one member. The plan binds sites, expands them into the mandatory
/// selection summary via the production resolver trace, and asserts on the
/// summary's selected cardinality.
const EXACTLY_ONE_SELECTED_MEMBER: &str = r#"(policy
  :id "test.relational.one-selected-member"
  :name "Every member access selects exactly one member"
  :message "a member access must resolve to exactly one selected member"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query (rql (occurrences :role [member_position])))
    (bind :name selection :from site :step member-selection)
    (join :left site :right selection :on ((ast_id site_ast_id)))
    (group :name by-site :by (site.ast_id)
      (aggregate :name winners :op min :value selection.selected_count))
    (assert :group by-site :value winners :cardinality (exactly 1))))"#;

/// Clean: a typed receiver selects its owner's member and nothing else.
#[test]
fn member_selection_invariant_is_clean_on_the_resolving_fixture() {
    let (_project, analyzer) = typescript(
        "class Service {\n  run(): number {\n    return 1;\n  }\n}\n\nexport function caller(service: Service) {\n  return service.run();\n}\n",
    );
    let run = evaluate(
        EXACTLY_ONE_SELECTED_MEMBER,
        &analyzer,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Finding: an unresolvable member access has a mandatory summary row with a
/// zero selected count, and the invariant reports it at the exact site.
#[test]
fn member_selection_invariant_reports_the_unresolved_site() {
    let (_project, analyzer) = typescript(
        "class Service {\n  run(): number {\n    return 1;\n  }\n}\n\nexport function caller(service) {\n  return service.run();\n}\n",
    );
    let run = evaluate(
        EXACTLY_ONE_SELECTED_MEMBER,
        &analyzer,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let finding = &run.findings()[0];
    assert_eq!(finding.primary().path(), "widget.ts");
    let PolicyFindingEvidence::Assertion { evidence } = finding.evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.expectation(), "(exactly 1)");
    assert_eq!(evidence.actual_count(), 0);
}

/// Unreliable: a truncated site binding makes the invariant inconclusive,
/// never clean.
#[test]
fn member_selection_invariant_is_unreliable_on_a_truncated_binding() {
    let (_project, analyzer) = typescript(
        "class Service {\n  run(): number {\n    return 1;\n  }\n  stop(): number {\n    return 2;\n  }\n}\n\nexport function caller(service: Service) {\n  return service.run() + service.stop();\n}\n",
    );
    let mut budget = PolicyBudget::builder()
        .with_query_limits(CodeQueryExecutionLimits {
            max_pipeline_rows: 1,
            ..CodeQueryExecutionLimits::default()
        })
        .expect("query limits")
        .build()
        .expect("budget");
    let run = evaluate(EXACTLY_ONE_SELECTED_MEMBER, &analyzer, &mut budget);
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Inconclusive { .. }),
        "{:?}",
        run.completion()
    );
    assert!(run.findings().is_empty());
}

/// The #1477 hierarchy-route invariant: every traced member candidate's route
/// is exactly as long as the hierarchy distance the resolver walked. The plan
/// binds sites, expands them into hop rows through the production trace, and
/// asserts the per-candidate hop count. This is also the executable proof that
/// `candidate-hierarchy` is a usable relational expansion.
const TWO_HOP_ROUTE: &str = r#"(policy
  :id "test.relational.candidate-route-length"
  :name "The inherited member is reached in exactly two hops"
  :message "the resolver's hierarchy route must be exactly two hops long"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query (rql (occurrences :role [member_position])))
    (bind :name hop :from site :step candidate-hierarchy)
    (join :left site :right hop :on ((ast_id ast_id)))
    (group :name by-candidate :by (hop.candidate_id)
      (aggregate :name hops :op count))
    (assert :group by-candidate :value hops :cardinality (exactly 2))))"#;

/// Clean: the candidate is found two supertypes up, so its route has exactly
/// two hop rows.
#[test]
fn candidate_hierarchy_expansion_is_clean_on_the_two_hop_route() {
    let (_project, analyzer) = java(
        "class Root { void run() {} }\nclass Base extends Root { }\nclass Service extends Base { }\nclass Caller {\n    void call(Service service) { service.run(); }\n}\n",
    );
    let run = evaluate(TWO_HOP_ROUTE, &analyzer, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Finding: a direct member is reached in zero hops, so the only group that
/// exists is the one-hop route of the *other* site, and the zero-hop candidate
/// contributes no group at all. A one-hop fixture therefore violates.
#[test]
fn candidate_hierarchy_expansion_reports_a_route_of_the_wrong_length() {
    let (_project, analyzer) = java(
        "class Root { void run() {} }\nclass Service extends Root { }\nclass Caller {\n    void call(Service service) { service.run(); }\n}\n",
    );
    let run = evaluate(TWO_HOP_ROUTE, &analyzer, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[0].evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.expectation(), "(exactly 2)");
    assert_eq!(evidence.actual_count(), 1);
}

/// A truncated binding row set is never a verdict: the relational plan reports
/// the run inconclusive instead of concluding over a proper subset.
#[test]
fn a_truncated_relational_binding_is_inconclusive() {
    // Two reads exist, so a one-row pipeline cap makes the binding a proper
    // subset of the true row set.
    let (_project, analyzer) = typescript(
        "export function render(): number {\n  return 1;\n}\n\nexport const alias = render;\nexport const alias2 = render;\n",
    );
    let mut budget = PolicyBudget::builder()
        .with_query_limits(CodeQueryExecutionLimits {
            max_pipeline_rows: 1,
            ..CodeQueryExecutionLimits::default()
        })
        .expect("query limits")
        .build()
        .expect("budget");
    let run = evaluate(FORBID_READS_RELATIONAL, &analyzer, &mut budget);
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Inconclusive { .. }),
        "{:?}",
        run.completion()
    );
    assert!(
        run.findings().is_empty(),
        "an incomplete row set never yields a verdict"
    );
}

/// The #1477 M4 open-world honesty rule as an executable policy: every call
/// site must dispatch to exactly one proven target. A closed monomorphic call
/// satisfies it; an open interface receiver must not, because its arms stay
/// `may_dispatch` inside a non-exhaustive set. This is also the executable
/// proof that `dispatch-outcome` and `dispatch-targets` are usable relational
/// expansions.
const EXACTLY_ONE_PROVEN_DISPATCH: &str = r#"(policy
  :id "test.relational.proven-dispatch"
  :name "Every call has exactly one proven dispatch target"
  :message "a call site must dispatch to exactly one proven target"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query (rql (occurrences :role [member_position])))
    (bind :name outcome :from site :step dispatch-outcome)
    (bind :name target :from site :step dispatch-targets)
    (join :left site :right outcome :on ((ast_id site_ast_id)))
    (join :left site :right target :on ((ast_id site_ast_id)))
    (group :name by-site :by (outcome.site_id)
      (aggregate :name proven :op count
                 :where ((target.dispatch eq proven_dispatch))))
    (assert :group by-site :value proven :cardinality (exactly 1))))"#;

/// Clean: a closed monomorphic call has one proven target in an exhaustive
/// set, so the exact-set assertion holds.
#[test]
fn dispatch_expansions_are_clean_on_a_closed_monomorphic_call() {
    let (_project, workspace) = java_workspace(
        "public class App {\n  static int helper() { return 1; }\n  static int caller() { return helper(); }\n}\n",
    );
    let run = evaluate_with_workspace(
        EXACTLY_ONE_PROVEN_DISPATCH,
        &workspace,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Open-world dispatch never satisfies an exact-set assertion. The interface
/// receiver's arm stays `may_dispatch`, so the proven count is zero and the
/// site is reported instead of passing.
#[test]
fn open_world_dispatch_never_satisfies_the_exact_set_assertion() {
    let (_project, workspace) = java_workspace(
        "interface Shape { int area(); }\nclass Square implements Shape { public int area() { return 1; } }\nclass Circle implements Shape { public int area() { return 2; } }\npublic class App {\n  static int caller(Shape shape) { return shape.area(); }\n}\n",
    );
    let run = evaluate_with_workspace(
        EXACTLY_ONE_PROVEN_DISPATCH,
        &workspace,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[0].evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.expectation(), "(exactly 1)");
    assert_eq!(
        evidence.actual_count(),
        0,
        "an open arm must never be counted as a proven dispatch"
    );
}

/// A workspace snapshot over one inline file, for the row families that need
/// the production resolver rather than a single-file analyzer.
fn workspace(
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

/// The #1478 Milestone 4 sugar, executed end to end: every call site must have
/// exactly one candidate that is both selected and applicable.
const SELECTED_IN_WINNING_TIER: &str = r#"(policy
  :id "test.relational.winning-tier"
  :name "The selected callable is applicable"
  :message "the resolver must select a callable its own applicability check accepted"
  :severity error
  :analysis (analysis
    :type assertion
    (bind :name site :query
      (rql (overload-selection (occurrences :role [member_position]))))
    (bind :name cand :query
      (rql (callable-applicability (occurrences :role [member_position]))))
    (assert-selected-in-winning-tier :id one-winner :site site :candidates cand)))"#;

const JAVA_OVERLOADS: &str = r#"package app;

public class Widget {
    int render() { return 0; }
    int render(int width) { return width; }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#;

const JAVA_NO_OVERLOAD_ACCEPTS: &str = r#"package app;

public class Widget {
    int render() { return 0; }
    int render(int width) { return width; }

    int caller(Widget widget) {
        return widget.render(1, 2, 3);
    }
}
"#;

const JAVA_TWO_EQUAL_WINNERS: &str = r#"package app;

public class Widget {
    int render(int width) { return width; }
    int render(String label) { return label.length(); }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#;

/// The aggregate value of the sole violated group, which for these policies is
/// the exact number the assertion rejected.
fn violated_count(run: &PolicyRun) -> u64 {
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[0].evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    evidence.actual_count()
}

/// Clean: the resolver selects the one-argument overload, and its own
/// applicability check accepted that overload.
#[test]
fn the_selected_overload_sits_in_the_winning_tier() {
    let (_project, workspace) = workspace("app/Widget.java", JAVA_OVERLOADS);
    let run = evaluate_with_workspace(
        SELECTED_IN_WINNING_TIER,
        &workspace,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Finding: no overload accepts three arguments, so the resolver binds no
/// callable and the site has zero candidates that are both `selected` and
/// `applicable`. The sugar's default cardinality states that every call site
/// resolves to exactly one accepted callable, so the unresolved site is one
/// finding of `actual 0` at the call.
#[test]
fn a_selection_outside_the_winning_tier_is_one_finding_at_the_call_site() {
    let (_project, workspace) = workspace("app/Widget.java", JAVA_NO_OVERLOAD_ACCEPTS);
    let run = evaluate_with_workspace(
        SELECTED_IN_WINNING_TIER,
        &workspace,
        &mut PolicyBudget::default(),
    );
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(violated_count(&run), 0);
    let finding = &run.findings()[0];
    assert_eq!(finding.primary().path(), "app/Widget.java");
    assert_eq!(
        finding
            .primary()
            .region()
            .expect("the violation anchors at the row's exact display range")
            .start_line(),
        8,
        "the finding points at the call, not at any declaration"
    );
    let PolicyFindingEvidence::Assertion { evidence } = finding.evidence() else {
        panic!("relational assertion policies produce assertion evidence");
    };
    assert_eq!(evidence.expectation(), "(exactly 1)");
    assert_eq!(evidence.anchor().assert_id(), "one-winner");
}

/// Zero stays assertable rather than being an error state: a site where the
/// resolver accepted nothing is exactly what `(exactly 0)` states, and the
/// same fixture is clean under it.
#[test]
fn an_unresolved_site_is_assertable_as_zero_winners() {
    let source = SELECTED_IN_WINNING_TIER.replace(
        ":site site :candidates cand)",
        ":site site :candidates cand :cardinality (exactly 0))",
    );
    let (_project, workspace) = workspace("app/Widget.java", JAVA_NO_OVERLOAD_ACCEPTS);
    let run = evaluate_with_workspace(&source, &workspace, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Many stays assertable too. Two overloads of one arity are both accepted by
/// Java's arity check, so the site keeps two winners rather than having one
/// picked by candidate order, and `(at-least 2)` states that outcome.
#[test]
fn an_ambiguous_site_is_assertable_as_several_winners() {
    let ambiguous = SELECTED_IN_WINNING_TIER.replace(
        ":site site :candidates cand)",
        ":site site :candidates cand :cardinality (at-least 2))",
    );
    let (_project, workspace) = workspace("app/Widget.java", JAVA_TWO_EQUAL_WINNERS);
    let run = evaluate_with_workspace(&ambiguous, &workspace, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());

    // The same site is a finding under the uniqueness assertion, which is what
    // makes the ambiguity observable rather than silently tolerated.
    let unique = evaluate_with_workspace(
        SELECTED_IN_WINNING_TIER,
        &workspace,
        &mut PolicyBudget::default(),
    );
    assert_eq!(violated_count(&unique), 2);
}

const ARGUMENT_ORDER: &str = r#"(policy
  :id "test.relational.argument-order"
  :name "Named arguments follow declaration order"
  :message "a named argument must sit at the position its parameter was declared at"
  :severity warning
  :analysis (analysis
    :type assertion
    (bind :name arg :query
      (rql (call-arguments (call-argument-groups
        (call-shape (language python (call :callee "greet")))))))
    (bind :name param :query
      (rql (signature-parameters (callable-signature
        (enclosing-decl (language python (function :name "greet")))))))
    (join :left arg :right param :on ((name label)))
    (group :name shape :by (arg.site_id)
      (aggregate :name parity :op ordered-equal
        :left (arg.argument_index arg.name)
        :right (param.parameter_index param.label)))
    (assert :group shape :value parity :cardinality (exactly 1))))"#;

const PYTHON_IN_ORDER: &str = r#"def greet(name, greeting):
    return greeting + name


def caller():
    return greet(name="ada", greeting="hi")
"#;

/// The same two named arguments, written in the other order. A set-equality
/// check cannot tell this fixture from the one above; the ordered predicate
/// exists because that difference is the whole invariant.
const PYTHON_OUT_OF_ORDER: &str = r#"def greet(name, greeting):
    return greeting + name


def caller():
    return greet(greeting="hi", name="ada")
"#;

#[test]
fn ordered_equal_is_clean_when_named_arguments_follow_declaration_order() {
    let (_project, workspace) = workspace("app.py", PYTHON_IN_ORDER);
    let run = evaluate_with_workspace(ARGUMENT_ORDER, &workspace, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert!(run.findings().is_empty(), "{:?}", run.findings());

    // The clean result is a satisfied assertion, not an absent group: the same
    // fixture violates the inverted assertion.
    let inverted = ARGUMENT_ORDER.replace(
        ":value parity :cardinality (exactly 1)",
        ":value parity :cardinality (exactly 0)",
    );
    let run = evaluate_with_workspace(&inverted, &workspace, &mut PolicyBudget::default());
    assert_eq!(violated_count(&run), 1, "parity was computed and held");
}

#[test]
fn ordered_equal_reports_the_same_set_written_in_a_different_order() {
    let (_project, workspace) = workspace("app.py", PYTHON_OUT_OF_ORDER);
    let run = evaluate_with_workspace(ARGUMENT_ORDER, &workspace, &mut PolicyBudget::default());
    assert_eq!(run.completion(), &PolicyRunCompletion::Complete);
    assert_eq!(violated_count(&run), 0, "the lists are equal as sets only");
    assert_eq!(run.findings()[0].primary().path(), "app.py");
}
