//! Known-answer regression net for the require-model taint compile.
//!
//! Stage A of the demand-driven-taint ExecPlan. This module makes no runtime
//! change. It only locks in current, confirmed-correct behavior of the eager
//! (all-procedures-rooted) `TaintPolicyCompiler` so a later demand-driven
//! rewrite of discovery cannot silently regress completeness.
//!
//! Each fixture below was run against current `master` and its assertion
//! confirmed to pass before this file was committed.
//!
//! Every fixture here selects native (bodiless) `attacker()`/`sensitive`
//! calls as its source and sink, with no registered summary for either: under
//! `call-modeling :unmodeled require-model`, a native call with no covering
//! summary always leaves the enclosing procedure's value-flow snapshot
//! `Unknown`, so every run below is typed `Inconclusive { PartialDiscovery }`
//! -- this is confirmed current behavior, not a defect this module works
//! around. `taint_findings()` (the diagnostic-neutral projection the solve
//! itself retains) is the reliable "was the flow found" signal used
//! throughout `taint_policy_adapter.rs`
//! (`production_taint_discovers_an_unselected_common_caller_for_sibling_callees`
//! is the precedent for exactly this shape), so each assertion below anchors
//! on it and treats the public `run.findings()` count as a second, separately
//! confirmed data point.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::semantic_model::{
    CatalogCoordinate, CatalogOptions, CompiledSemanticModelPack, CompilerOptions,
    SemanticModelActivationEvidence, SemanticModelActivationRequest, SemanticModelRuntimeLimits,
    SemanticPackCatalog, SessionPackSource, SessionPackSourceKind, SourceFormat, compile_source,
};
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyIncompleteReason, PolicyRunCompletion, PolicySemanticModelContext, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer, evaluate_policy_inputs_with_analyzer_and_semantic_models,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

const MODEL_ARTIFACT_SHA256: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A require-model taint policy over a single named source and a single named
/// sink, both selected by callee name. Mirrors `java_summary_policy` and
/// `policy_source` in `taint_policy_adapter.rs` / `issue_1917_bodied_sink.rs`,
/// reduced to the two names each fixture below needs.
fn require_model_policy(id: &str, source_callee: &str, sink_callee: &str) -> String {
    format!(
        r#"(policy
  :schema-version 1
  :id "{id}"
  :name "Require-model taint regression fixture"
  :message "taint regression fixture"
  :severity warning
  :analysis (analysis
    :type taint
    :mode may
    :call-modeling (call-modeling :unmodeled require-model)
    :sources (endpoint-set :entries [
      (source :id attacker :display-name "attacker" :categories [input.user]
        :selector (rql :schema-version 1
          (language java (call :callee (name "{source_callee}"))))
        :bind return-value :labels [untrusted])])
    :sinks (endpoint-set :entries [
      (sink :id sensitive :display-name "sensitive" :categories [data.sensitive]
        :selector (rql :schema-version 1
          (language java (call :callee (name "{sink_callee}"))))
        :dangerous-operand (argument :index 0) :accepts [untrusted])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Test" :id "TAINT-REGRESSION")))"#
    )
}

/// Evaluates one Java fixture against one embedded require-model policy
/// source, with no semantic-model catalog. Mirrors `direct_finding_count` in
/// `issue_1917_bodied_sink.rs`.
fn evaluate_java(fixture: &str, policy_source: &str) -> PolicyBatchOutcome {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", fixture)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:taint-regression.rqlp"),
        policy_source,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 12).expect("fixed evaluation date"),
    );
    evaluate_policy_inputs_with_analyzer(project.root(), &inputs, &workspace, &options, None)
        .expect("require-model taint regression evaluation")
}

/// The public policy report's one run. Every fixture in this module produces
/// exactly one policy run.
fn one_run(outcome: &PolicyBatchOutcome) -> &brokk_bifrost::policy::PolicyRun {
    let [run] = outcome.report().runs() else {
        panic!(
            "expected exactly one policy run, got {:?}",
            outcome.report().runs()
        );
    };
    run
}

/// Every fixture's native `attacker()`/`sensitive` calls leave the enclosing
/// procedure's value-flow snapshot `Unknown` under require-model with no
/// covering summary, so every run here is typed `Inconclusive {
/// PartialDiscovery }`. This asserts exactly that, so a future fixture that
/// starts reaching `Complete` (for example after a later stage adds summary
/// coverage) is a visible, deliberate change to this file rather than a
/// silent drift.
fn assert_partial_discovery(run: &brokk_bifrost::policy::PolicyRun) {
    assert!(
        matches!(
            run.completion(),
            PolicyRunCompletion::Inconclusive { reasons }
                if reasons.contains(&PolicyIncompleteReason::PartialDiscovery)
        ),
        "{:?}: {:?}",
        run.completion(),
        run.diagnostics()
    );
}

/// A source call in callee `A` (`produce`), a sink call in callee `B`
/// (`consume`), and a common caller `C` (`run`) that takes `A`'s tainted
/// return and passes it to `B`'s argument. Neither `A` nor `B` contains the
/// other endpoint, so only the common caller's own root discovers a region
/// holding both. This is the completeness case that all-procedures rooting
/// exists for in `TaintPolicyCompiler::compile_inner`: the root set is every
/// procedure of the materialized artifact, not just the endpoints'
/// procedures, so `run`'s forward closure over its call graph reaches both
/// `produce` and `consume` even though `run` selects neither source nor sink
/// itself.
///
/// `run.findings()` is empty here (not 1): this shape is the same one
/// `production_taint_discovers_an_unselected_common_caller_for_sibling_callees`
/// pins for the "may"/optimistic policy family -- the sibling-callee finding
/// retains no source origin evidence through the summary join at a
/// non-endpoint root (#1951's minimization), so the public projection drops
/// it while the diagnostic-neutral `taint_findings()` retains it. This
/// fixture confirms the same origin-retention shape holds for the
/// require-model family.
#[test]
fn cross_procedure_flow_through_a_common_caller_is_found() {
    const FIXTURE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);

  static String produce() {
    return attacker();
  }

  static void consume(String value) {
    sensitive(value);
  }

  void run() {
    String produced = produce();
    consume(produced);
  }
}
"#;
    let policy = require_model_policy("test.cross-procedure", "attacker", "sensitive");
    let outcome = evaluate_java(FIXTURE, &policy);
    assert_eq!(
        outcome.taint_findings().len(),
        1,
        "diagnostics={:?}",
        outcome.report().diagnostics()
    );
    let run = one_run(&outcome);
    assert_partial_discovery(run);
    assert!(
        run.findings().is_empty(),
        "{:?}: {:?}",
        run.findings(),
        run.diagnostics()
    );
}

/// The source and sink calls sit in one procedure. This is the baseline case:
/// the endpoints' own procedure is trivially a root, and no cross-procedure
/// discovery is needed.
#[test]
fn direct_source_to_sink_in_one_procedure_is_found() {
    const FIXTURE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);

  void run() {
    sensitive(attacker());
  }
}
"#;
    let policy = require_model_policy("test.direct", "attacker", "sensitive");
    let outcome = evaluate_java(FIXTURE, &policy);
    assert_eq!(
        outcome.taint_findings().len(),
        1,
        "diagnostics={:?}",
        outcome.report().diagnostics()
    );
    let run = one_run(&outcome);
    assert_partial_discovery(run);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
}

/// A recursive helper sits on the taint path: `attacker()` feeds a recursive,
/// in-repo passthrough (`recurse`, which calls itself before returning its
/// argument unchanged), and the passthrough's result reaches `sensitive`.
/// Discovery's `seen` guard in `discover_value_flow` must visit `recurse`
/// once, not loop, and the flow must still be found.
#[test]
fn recursive_passthrough_on_the_taint_path_is_found() {
    const FIXTURE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);

  static String recurse(String value, int depth) {
    if (depth <= 0) {
      return value;
    }
    return recurse(value, depth - 1);
  }

  void run() {
    sensitive(recurse(attacker(), 2));
  }
}
"#;
    let policy = require_model_policy("test.recursion", "attacker", "sensitive");
    let outcome = evaluate_java(FIXTURE, &policy);
    assert_eq!(
        outcome.taint_findings().len(),
        1,
        "diagnostics={:?}",
        outcome.report().diagnostics()
    );
    let run = one_run(&outcome);
    assert_partial_discovery(run);
    assert_eq!(run.findings().len(), 1, "{:?}", run.findings());
}

/// A modeled sanitizer sits on the path and removes the taint label. This is
/// the true-negative fixture: it proves the regression net also pins the
/// absence of a finding, not only its presence.
///
/// The sanitizer is a shipped `Sanitize` summary effect on an external
/// (bodiless) boundary method, `escape`, mirroring
/// `sanitizer_summary_pack.rs` (issue #1923). A policy-local `(sanitizer
/// ...)` selector is not available for this: `TaintPolicyCompiler::compile_inner`
/// rejects any policy whose resolved spec carries a non-empty `sanitizers`
/// list with `UnsupportedAuxiliarySemantics("sanitizer")`, so only the
/// summary-effect route is exercised.
#[test]
fn a_modeled_sanitizer_on_the_path_clears_the_finding() {
    const FIXTURE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);
  native String escape(String value);

  void run() {
    sensitive(this.escape(attacker()));
  }
}
"#;
    let policy = require_model_policy("test.sanitizer-cleared", "attacker", "sensitive");

    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", FIXTURE)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    catalog
        .register_session_pack(
            &escape_sanitizer_pack(),
            &SessionPackSource {
                kind: SessionPackSourceKind::Embedded,
                source_id: "test:taint-regression-sanitizer".to_owned(),
            },
        )
        .expect("register the sanitizer procedure-summary pack");
    let request = semantic_model_request();

    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:taint-regression-sanitizer.rqlp"),
        &policy,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 12).expect("fixed evaluation date"),
    );
    let outcome = evaluate_policy_inputs_with_analyzer_and_semantic_models(
        project.root(),
        &inputs,
        &workspace,
        &options,
        PolicySemanticModelContext {
            catalog: &catalog,
            request: &request,
            persistence: None,
        },
        None,
    )
    .expect("require-model sanitizer regression evaluation");

    assert!(
        outcome.taint_findings().is_empty(),
        "{:?}",
        outcome.taint_findings()
    );
    let run = one_run(&outcome);
    assert_partial_discovery(run);
    assert!(run.findings().is_empty(), "{:?}", run.findings());
}

fn escape_sanitizer_pack() -> CompiledSemanticModelPack {
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.taint-regression-sanitizer",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "taint-regression-fixtures" },
        "license": "Apache-2.0",
        "completeness": "complete",
        "safety": { "generated_code_only": false, "review_required": false },
        "shards": [{
            "id": "summaries.escape",
            "activation": [{
                "package": { "name": "com.acme:external", "version": ">=1.0.0, <2.0.0" },
                "targets": ["jvm"],
                "configurations": ["release"],
                "artifact_sha256": MODEL_ARTIFACT_SHA256
            }],
            "payload": { "kind": "procedure_summaries", "summaries": [{
                "id": "summary.escape",
                "target": {
                    "path": "app.java",
                    "symbol": "escape(String)",
                    "has_receiver": true,
                    "parameter_count": 1
                },
                "completeness": "complete",
                "transfers": [{
                    "input": { "kind": "parameter", "ordinal": 0 },
                    "exit_kind": "normal",
                    "output": { "kind": "normal_return" }
                }],
                "effects": [{
                    "kind": "sanitize",
                    "input": { "kind": "parameter", "ordinal": 0 },
                    "output": { "kind": "normal_return" },
                    "removes": ["untrusted"]
                }]
            }] }
        }]
    }))
    .unwrap();
    compile_source(SourceFormat::Json, &source, &CompilerOptions::default())
        .unwrap_or_else(|diagnostics| panic!("sanitizer pack failed: {diagnostics:#?}"))
}

fn semantic_model_request() -> SemanticModelActivationRequest {
    SemanticModelActivationRequest {
        bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
        evidence: vec![SemanticModelActivationEvidence {
            language: "java".to_owned(),
            ecosystem: "maven".to_owned(),
            package: Some(CatalogCoordinate {
                name: "com.acme:external".to_owned(),
                version: Some(Version::parse("1.5.0").unwrap()),
            }),
            module: None,
            toolchain: Some(CatalogCoordinate {
                name: "jdk".to_owned(),
                version: Some(Version::parse("17.0.1").unwrap()),
            }),
            target: Some("jvm".to_owned()),
            configuration: Some("release".to_owned()),
            artifact_sha256: Some(MODEL_ARTIFACT_SHA256.to_owned()),
        }],
        controls: Vec::new(),
        limits: SemanticModelRuntimeLimits::default(),
    }
}
