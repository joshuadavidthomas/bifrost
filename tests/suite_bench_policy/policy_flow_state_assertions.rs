//! End-to-end coverage for the RQLP temporal and termination asserts (issue
//! #1480, Milestone 5).
//!
//! `assert-flow-establishment` states that control flow has established a
//! binding or property before the captured token reads it. Its evidence is the
//! production CFG's own reaching-definition, dominance and same-evaluation
//! relations; there is no lexical or source-order fallback anywhere under it,
//! which is what makes the read-before-write fixture fail even though the write
//! is one line below the read.
//!
//! `assert-rewrite-termination` states that every bounded chase of one declared
//! rewrite domain converges within the bound the domain declares for itself. A
//! repeated semantic state is a counterexample and becomes a finding with its
//! ordered witness; an exhausted budget is absence of evidence and makes the
//! run inconclusive instead.
//!
//! Every test asserts the run's completion before reading its findings. The
//! soundness rule returns zero findings whenever an input is incomplete, so a
//! test that only counted findings would pass just as happily on a language
//! whose lowering states nothing as on a satisfied invariant.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use brokk_bifrost::policy::{
    CatalogRegistryLimits, DefaultPolicyEvaluator, HumanRenderColor, HumanRenderDetail,
    HumanRenderOptions, PolicyAnalysis, PolicyAnalysisType, PolicyBudget, PolicyEvaluationContext,
    PolicyEvaluationDate, PolicyEvaluationOptions, PolicyEvaluator, PolicyFindingEvidence,
    PolicyRegistry, PolicyRegistryLimits, PolicyRun, PolicyRunCompletion, PolicySourceIdentity,
    RqlpDocument, SarifToolIdentity, TaintCatalogRegistry, evaluate_policy_files,
    parse_rqlp_source, write_policy_human, write_policy_json, write_policy_sarif,
};
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The satisfied shape: the only establishment of `total` dominates its read.
const JS_AFTER_ESTABLISHMENT: &str = r#"
function afterEstablishment(flag) {
  let total = 0;
  const observed = total;
  return observed;
}
"#;

/// The mined shape: the write is one line *below* the read, so source
/// co-presence would serve it and control flow must not.
const JS_BEFORE_ESTABLISHMENT: &str = r#"
function beforeEstablishment() {
  let early;
  const captured = early;
  early = 1;
  return captured;
}
"#;

/// One-armed conditional establishment: some path to the read carries the
/// write and some path does not, so the read is reached but not dominated.
const JS_CONDITIONAL_ESTABLISHMENT: &str = r#"
function conditionalEstablishment(flag) {
  let value;
  if (flag) {
    value = 1;
  }
  const observed = value;
  return observed;
}
"#;

/// `x = wrap(x)`: the read feeds the very value the write assigns, so the
/// write cannot serve it (mined commit `9e60fddcb`'s ordering rule).
const JS_SAME_ASSIGNMENT: &str = r#"
function wrap(value) {
  return value;
}

function sameAssignment(x) {
  x = wrap(x);
  return x;
}
"#;

/// A file whose property axis is genuinely partial -- the JavaScript lowering
/// publishes a field-memory capability gap for `ns.value` -- while the asserted
/// read is a binding read the derivation states completely. The assert must
/// still conclude: it consults the axes it names and no others.
const JS_PARTIAL_BUT_IRRELEVANT: &str = r#"
function partialButIrrelevant() {
  const ns = {};
  ns.value = 1;
  let total = 0;
  const observed = total;
  return observed;
}
"#;

/// Mined commit `edb00e017`: `for x := range x`. Milestone 2 found that the Go
/// lowering publishes no assignment instruction for the `:=` range binder, so
/// the axes this assert reads are uncovered and the honest verdict is
/// inconclusive -- never a pass and never a finding.
const GO_RANGE_SELF_BINDER: &str = r#"package app

func rangeSelfBinder(x []int) int {
	total := 0
	for x := range x {
		total += x
	}
	return total
}
"#;

const GO_MOD: &str = "module example.com/app\n\ngo 1.22\n";

const RUST_RENAME_MODULES: &str = "pub mod deep { pub struct Leaf; }\npub mod a { pub struct Foo; }\npub mod c { pub struct Bar; }\nmod consumer;\n";

/// One rename hop that converges on a real module root.
const RUST_CONVERGENT_RENAME: &str =
    "use c as a;\n\npub fn use_rename() {\n    let _z = a::Bar;\n}\n";

/// The `9deded6f5` shape in the form the production rule engages: two renames
/// whose roots form a cycle, so the chase rewrites A -> B -> A forever.
const RUST_RENAME_CYCLE_ROOTS: &str = "use a as c;\nuse c as a;\n\npub fn use_root_cycle() {\n    let _x = a::Bar;\n    let _y = c::Foo;\n}\n";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn policy(id: &str, subject: &str, asserts: &str) -> String {
    format!(
        r#"(policy
  :id "{id}"
  :name "Flow assertion"
  :message "the flow invariant does not hold"
  :severity warning
  :analysis (analysis
    :type assertion
    :subject (rql {subject})
    :asserts [{asserts}]))"#
    )
}

fn registry_with_policy(source: &str) -> PolicyRegistry {
    let catalogs = Arc::new(TaintCatalogRegistry::new_without_workspace(
        CatalogRegistryLimits::default(),
    ));
    let mut registry =
        PolicyRegistry::new_without_workspace(catalogs, PolicyRegistryLimits::default());
    registry
        .register_policy_bytes(
            PolicySourceIdentity::new("test:flow-assertion"),
            source.as_bytes(),
        )
        .expect("valid flow assertion policy");
    registry
}

/// Evaluate against a full workspace snapshot. The flow-state family reads the
/// production CFG, which is a workspace artifact and has no substitute.
fn evaluate(source: &str, workspace: &WorkspaceAnalyzer) -> PolicyRun {
    let registry = registry_with_policy(source);
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
        .expect("flow assertion evaluation")
}

fn javascript(source: &str) -> (BuiltInlineTestProject, WorkspaceAnalyzer) {
    let project = InlineTestProject::with_language(Language::JavaScript)
        .file("src/main.js", source)
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    (project, workspace)
}

fn go(source: &str) -> (BuiltInlineTestProject, WorkspaceAnalyzer) {
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", GO_MOD)
        .file("app.go", source)
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    (project, workspace)
}

/// A single-crate Rust workspace whose consumer files hold the alias chains.
fn rust_workspace(consumers: &[(&str, &str)]) -> (BuiltInlineTestProject, WorkspaceAnalyzer) {
    let mut lib = String::from(
        "pub mod deep { pub struct Leaf; }\npub mod a { pub struct Foo; }\npub mod c { pub struct Bar; }\n",
    );
    for (path, _) in consumers {
        let module = path
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".rs"))
            .expect("a consumer path names a module file");
        lib.push_str(&format!("mod {module};\n"));
    }
    let mut project = InlineTestProject::with_language(Language::Rust)
        .file("Cargo.toml", "[workspace]\nmembers = [\"alias\"]\n")
        .file(
            "alias/Cargo.toml",
            "[package]\nname = \"alias\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("alias/src/lib.rs", lib);
    for (path, source) in consumers {
        project = project.file(format!("alias/src/{path}"), *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    (project, workspace)
}

fn assertion_observed(run: &PolicyRun, index: usize) -> String {
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[index].evidence() else {
        panic!("assertion policies produce assertion evidence");
    };
    evidence
        .observed()
        .expect("a temporal or termination finding always names what it observed")
        .to_string()
}

fn assert_kind(run: &PolicyRun, index: usize) -> String {
    let PolicyFindingEvidence::Assertion { evidence } = run.findings()[index].evidence() else {
        panic!("assertion policies produce assertion evidence");
    };
    evidence.assert_kind().to_string()
}

// ---------------------------------------------------------------------------
// assert-flow-establishment
// ---------------------------------------------------------------------------

/// The satisfied case: the establishment dominates the read, so both the
/// default `reached` requirement and the stronger `dominated` one hold.
#[test]
fn a_read_after_its_establishment_satisfies_both_requirements() {
    let (_project, workspace) = javascript(JS_AFTER_ESTABLISHMENT);
    for require in ["", ":require reached", ":require dominated"] {
        let run = evaluate(
            &policy(
                "test.flow.after",
                r#"(identifier :text/regex "^total$" :capture "site")"#,
                &format!(
                    r#"(assert-flow-establishment :id established :at "site"
                          :role value_reference {require})"#
                ),
            ),
            &workspace,
        );
        assert_eq!(run.analysis_type(), PolicyAnalysisType::Assertion);
        assert_eq!(
            run.completion(),
            &PolicyRunCompletion::Complete,
            "{require}: the verdict must be read only from a complete run: {:?}",
            run.diagnostics()
        );
        assert!(
            run.findings().is_empty(),
            "{require}: the establishment dominates the read: {:?}",
            run.findings()
        );
    }
}

/// The mined shape: the only establishment is one line *below* the read, so no
/// CFG path carries it there. The finding names the read and the establishment
/// that was considered, with the reason it does not serve it.
#[test]
fn a_read_before_its_establishment_is_a_finding_with_the_considered_witness() {
    let (_project, workspace) = javascript(JS_BEFORE_ESTABLISHMENT);
    let run = evaluate(
        &policy(
            "test.flow.before",
            r#"(identifier :text/regex "^early$" :capture "site")"#,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference)"#,
        ),
        &workspace,
    );
    assert_eq!(
        run.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        run.diagnostics()
    );
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
    assert_eq!(assert_kind(&run, 0), "flow_establishment");
    let observed = assertion_observed(&run, 0);
    assert!(
        observed.contains("is not reached by any establishment"),
        "the finding must state the unmet requirement: {observed}"
    );
    assert!(
        observed.contains("considered: binding at src/main.js:5 (does not reach the read)"),
        "the finding must list the establishment it considered and why it fails: {observed}"
    );
}

/// A one-armed conditional establishment reaches the read on some path and
/// dominates it on none. The two requirements must therefore disagree, which is
/// exactly why dominance is its own relation rather than "exact reaching".
#[test]
fn a_one_armed_conditional_is_reached_but_not_dominated() {
    let (_project, workspace) = javascript(JS_CONDITIONAL_ESTABLISHMENT);
    let subject = r#"(identifier :text/regex "^value$" :capture "site")"#;

    let reached = evaluate(
        &policy(
            "test.flow.conditional.reached",
            subject,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference
                  :require reached)"#,
        ),
        &workspace,
    );
    assert_eq!(
        reached.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        reached.diagnostics()
    );
    assert!(
        reached.findings().is_empty(),
        "the conditional write reaches the read on one path: {:?}",
        reached.findings()
    );

    let dominated = evaluate(
        &policy(
            "test.flow.conditional.dominated",
            subject,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference
                  :require dominated)"#,
        ),
        &workspace,
    );
    assert_eq!(
        dominated.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        dominated.diagnostics()
    );
    assert_eq!(dominated.findings().len(), 1, "{:?}", dominated.findings());
    let observed = assertion_observed(&dominated, 0);
    assert!(
        observed.contains("is not dominated by any establishment")
            && observed.contains("reaches the read but does not dominate it"),
        "the finding must distinguish reaching from dominance: {observed}"
    );
}

/// `x = wrap(x)`: the binder must not serve its own evaluation's read. The
/// finding names both ends.
#[test]
fn a_same_evaluation_binder_is_forbidden_and_names_both_ends() {
    let (_project, workspace) = javascript(JS_SAME_ASSIGNMENT);
    let run = evaluate(
        &policy(
            "test.flow.same-evaluation",
            r#"(identifier :text/regex "^x$" :capture "site")"#,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference
                  :forbid-same-evaluation true)"#,
        ),
        &workspace,
    );
    assert_eq!(
        run.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        run.diagnostics()
    );
    assert!(!run.findings().is_empty(), "{:?}", run.findings());
    let observed = (0..run.findings().len())
        .map(|index| assertion_observed(&run, index))
        .find(|observed| observed.contains("in the same evaluation"))
        .expect("one finding must report the same-evaluation violation");
    assert!(
        observed.contains("establishment of binding at src/main.js:7")
            && observed.contains("the read of binding at src/main.js:7"),
        "the finding must name both ends of the relation: {observed}"
    );
}

/// The completeness contract, tested on the exact shape Milestone 2 found in
/// the wild: the JavaScript lowering publishes a field-memory capability gap,
/// so the property axis of this file is partial -- and the assert, which reads
/// binding events and the reaching relation, must still conclude.
#[test]
fn a_gap_on_an_unconsulted_axis_still_concludes() {
    let (_project, workspace) = javascript(JS_PARTIAL_BUT_IRRELEVANT);
    let run = evaluate(
        &policy(
            "test.flow.partial",
            r#"(identifier :text/regex "^total$" :capture "site")"#,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference)"#,
        ),
        &workspace,
    );
    assert_eq!(
        run.completion(),
        &PolicyRunCompletion::Complete,
        "a partial property axis must not make a binding assert unreliable: {:?}",
        run.diagnostics()
    );
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Mined commit `edb00e017`, as an honest negative. The Go `:=` range binder
/// has no establishment in the production IR, which uncovers the very axes this
/// assert reads, so the run is inconclusive -- not a pass, not a finding.
#[test]
fn the_go_range_binder_gap_is_inconclusive_rather_than_a_verdict() {
    let (_project, workspace) = go(GO_RANGE_SELF_BINDER);
    let run = evaluate(
        &policy(
            "test.flow.go-range",
            r#"(identifier :text/regex "^x$" :capture "site")"#,
            r#"(assert-flow-establishment :id established :at "site" :role value_reference)"#,
        ),
        &workspace,
    );
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Inconclusive { .. }),
        "the Go adapter gap must be reported, not resolved: {:?}",
        run.completion()
    );
    assert!(run.findings().is_empty(), "{:?}", run.findings());
    let reported = format!("{:?}", run.completion());
    assert!(
        reported.contains("CapabilityIncomplete"),
        "the typed reason must survive to the run: {reported}"
    );
}

// ---------------------------------------------------------------------------
// assert-rewrite-termination
// ---------------------------------------------------------------------------

/// A chase that converges satisfies the assert.
#[test]
fn a_convergent_rewrite_chain_satisfies_the_termination_assert() {
    let (_project, workspace) = rust_workspace(&[("consumer.rs", RUST_CONVERGENT_RENAME)]);
    let run = evaluate(
        &policy(
            "test.rewrite.converged",
            r#"(identifier :text/regex "^use_rename$" :capture "site")"#,
            r#"(assert-rewrite-termination :id terminates :domain rust-import-alias)"#,
        ),
        &workspace,
    );
    assert_eq!(
        run.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        run.diagnostics()
    );
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

/// Mined commit `9deded6f5`: a rename cycle is a concrete counterexample, and
/// the finding carries the ordered repeated-state witness.
#[test]
fn a_rename_cycle_is_a_finding_with_its_ordered_witness() {
    let (_project, workspace) = rust_workspace(&[("consumer.rs", RUST_RENAME_CYCLE_ROOTS)]);
    let run = evaluate(
        &policy(
            "test.rewrite.cycle",
            r#"(identifier :text/regex "^use_root_cycle$" :capture "site")"#,
            r#"(assert-rewrite-termination :id terminates :domain rust-import-alias)"#,
        ),
        &workspace,
    );
    assert_eq!(
        run.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        run.diagnostics()
    );
    assert!(!run.findings().is_empty(), "{:?}", run.findings());
    assert_eq!(assert_kind(&run, 0), "rewrite_termination");
    let observed = (0..run.findings().len())
        .map(|index| assertion_observed(&run, index))
        .find(|observed| observed.contains("witness: c -> a -> c"))
        .expect("one finding must carry the ordered repeated-root witness");
    assert!(
        observed.contains("repeats a semantic state") && observed.contains("declared bound"),
        "the finding must state the outcome and the bound it was measured against: {observed}"
    );
}

/// `:scope` narrows the file set to the files whose subject rows bind the
/// capture: the same workspace is a finding without it and clean with it.
#[test]
fn a_scope_capture_restricts_the_termination_file_set() {
    let (_project, workspace) = rust_workspace(&[
        ("cycle_consumer.rs", RUST_RENAME_CYCLE_ROOTS),
        ("clean_consumer.rs", RUST_CONVERGENT_RENAME),
    ]);
    let subject = r#"(identifier :text/regex "^use_rename$" :capture "site")"#;

    let unscoped = evaluate(
        &policy(
            "test.rewrite.unscoped",
            subject,
            r#"(assert-rewrite-termination :id terminates :domain rust-import-alias)"#,
        ),
        &workspace,
    );
    assert_eq!(
        unscoped.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        unscoped.diagnostics()
    );
    assert!(
        !unscoped.findings().is_empty(),
        "without a scope the assert is about every analyzed file: {:?}",
        unscoped.findings()
    );

    let scoped = evaluate(
        &policy(
            "test.rewrite.scoped",
            subject,
            r#"(assert-rewrite-termination :id terminates :domain rust-import-alias
                  :scope "site")"#,
        ),
        &workspace,
    );
    assert_eq!(
        scoped.completion(),
        &PolicyRunCompletion::Complete,
        "{:?}",
        scoped.diagnostics()
    );
    assert!(
        scoped.findings().is_empty(),
        "the scoped file set holds only the convergent chase: {:?}",
        scoped.findings()
    );
}

/// A scope capture the subject selector never binds is an authoring error, not
/// a vacuous pass over an empty file set.
#[test]
fn an_unbound_scope_capture_fails_the_run() {
    let (_project, workspace) = rust_workspace(&[("consumer.rs", RUST_RENAME_CYCLE_ROOTS)]);
    let run = evaluate(
        &policy(
            "test.rewrite.unbound-scope",
            r#"(identifier :text/regex "^use_root_cycle$" :capture "site")"#,
            r#"(assert-rewrite-termination :id terminates :domain rust-import-alias
                  :scope "missing")"#,
        ),
        &workspace,
    );
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Failed { .. }),
        "an unbound scope capture must fail the run: {:?}",
        run.completion()
    );
}

// ---------------------------------------------------------------------------
// Authoring surface: source form, canonical projection, renderings
// ---------------------------------------------------------------------------

/// Both families decode from `.rqlp` source form, defaults included, and both
/// project into canonical semantic JSON under their own `kind` tag. The
/// canonical projection is what the semantic hash is taken over, so a family
/// that projected an untagged or lossy object would collapse two different
/// policies into one identity.
#[test]
fn both_families_parse_from_source_and_project_canonically() {
    let source = policy(
        "test.flow.canonical",
        r#"(identifier :text/regex "^value$" :capture "site")"#,
        r#"(assert-flow-establishment :id established :at "site" :role value_reference
              :require dominated :forbid-same-evaluation true)
           (assert-flow-establishment :id defaulted :at "site" :role value_reference)
           (assert-rewrite-termination :id terminates :domain rust-import-alias :scope "site")
           (assert-rewrite-termination :id everywhere :domain rust_import_alias)"#,
    );
    let parsed = parse_rqlp_source(&source, PolicySourceIdentity::new("test:canonical"))
        .expect("both new assert families parse from source form");

    let RqlpDocument::Policy { definition } = parsed.document().clone() else {
        panic!("the fixture is a policy");
    };
    let PolicyAnalysis::Assertion { spec } = definition.analysis else {
        panic!("expected assertion analysis");
    };
    assert_eq!(spec.asserts.len(), 4);
    assert_eq!(spec.asserts[0].at(), Some("site"));
    assert_eq!(spec.asserts[0].kind_label(), "flow_establishment");
    assert_eq!(
        spec.asserts[3].at(),
        None,
        "the termination family binds no subject capture"
    );
    assert_eq!(spec.asserts[3].kind_label(), "rewrite_termination");

    let canonical = parsed
        .to_inline_local_canonical_semantic_json()
        .expect("an inline policy projects canonically");
    let asserts = canonical
        .pointer("/analysis/asserts")
        .and_then(Value::as_array)
        .expect("the canonical projection lists the asserts");
    assert_eq!(
        asserts[0],
        json!({
            "kind": "flow_establishment",
            "id": "established",
            "at": "site",
            "role": "value_reference",
            "require": "dominated",
            "forbid_same_evaluation": true,
        })
    );
    assert_eq!(
        asserts[1],
        json!({
            "kind": "flow_establishment",
            "id": "defaulted",
            "at": "site",
            "role": "value_reference",
            "require": "reached",
            "forbid_same_evaluation": false,
        }),
        "the omitted requirement projects its default explicitly"
    );
    assert_eq!(
        asserts[2],
        json!({
            "kind": "rewrite_termination",
            "id": "terminates",
            "domain": "rust_import_alias",
            "scope": "site",
        })
    );
    assert_eq!(
        asserts[3],
        json!({
            "kind": "rewrite_termination",
            "id": "everywhere",
            "domain": "rust_import_alias",
            "scope": null,
        }),
        "the two accepted domain spellings canonicalize to one value"
    );
}

const TERMINATION_POLICY: &str = r#"(policy
  :id "test.rewrite.rendered"
  :name "Rewrite termination"
  :message "a bounded rewrite chase must converge"
  :severity error
  :analysis (analysis
    :type assertion
    :subject (rql (identifier :text/regex "^use_root_cycle$" :capture "site"))
    :asserts [(assert-rewrite-termination :id terminates :domain rust-import-alias)]))"#;

/// The three renderings a consumer actually reads must all carry the ordered
/// witness. A cycle finding whose witness survived only into one projection
/// would leave the other two readers with an unreplayable claim.
#[test]
fn human_json_and_sarif_all_carry_the_ordered_cycle_witness() {
    let workspace_dir = tempfile::tempdir().expect("temporary workspace");
    for directory in ["alias/src", "policies"] {
        fs::create_dir_all(workspace_dir.path().join(directory)).expect("fixture directory");
    }
    fs::write(
        workspace_dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"alias\"]\n",
    )
    .expect("workspace manifest");
    fs::write(
        workspace_dir.path().join("alias/Cargo.toml"),
        "[package]\nname = \"alias\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("crate manifest");
    fs::write(
        workspace_dir.path().join("alias/src/lib.rs"),
        RUST_RENAME_MODULES,
    )
    .expect("crate root");
    fs::write(
        workspace_dir.path().join("alias/src/consumer.rs"),
        RUST_RENAME_CYCLE_ROOTS,
    )
    .expect("consumer fixture");
    fs::write(
        workspace_dir.path().join("policies/termination.rqlp"),
        TERMINATION_POLICY,
    )
    .expect("policy fixture");

    let outcome = evaluate_policy_files(
        workspace_dir.path(),
        &[PathBuf::from("policies").join("termination.rqlp")],
        &PolicyEvaluationOptions::new(
            PolicyEvaluationDate::from_ymd(2026, 8, 12).expect("fixed test date"),
        ),
    )
    .expect("coordinated policy evaluation");
    let report = outcome.report();
    assert_eq!(report.runs().len(), 1);
    assert_eq!(
        report.runs()[0].findings().len(),
        1,
        "one assert states one claim, so one violation is one finding: {:?}",
        report.runs()[0].findings()
    );

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
        human.contains("rewrite_termination") && human.contains("witness: c -> a -> c"),
        "{human}"
    );

    let mut json_bytes = Vec::new();
    write_policy_json(report, &mut json_bytes, usize::MAX).expect("json rendering");
    let json: Value = serde_json::from_slice(&json_bytes).expect("json output parses");
    let evidence = &json["runs"][0]["findings"][0]["evidence"]["evidence"];
    assert_eq!(evidence["assert_kind"], "rewrite_termination", "{evidence}");
    assert!(
        evidence["observed"]
            .as_str()
            .expect("the finding names what it observed")
            .contains("witness: c -> a -> c"),
        "{evidence}"
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
    let result = &sarif["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "test.rewrite.rendered", "{result}");
    assert!(
        serde_json::to_string(&result["properties"])
            .expect("sarif properties serialize")
            .contains("witness: c -&gt; a -&gt; c")
            || serde_json::to_string(&result["properties"])
                .expect("sarif properties serialize")
                .contains("witness: c -> a -> c"),
        "{result}"
    );
}
