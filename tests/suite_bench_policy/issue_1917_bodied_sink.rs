//! Regression tests for issue #1917: a taint sink declared with a body must
//! report the same public policy findings as a native (bodyless) sink
//! declaration.
//!
//! The defect was not in binding or propagation: the solver met the sink and
//! retained the finding row either way. A native callee has no call edge, so
//! its sink-meeting fact materialized on the caller's continuation edge and
//! kept the caller's concrete path. A bodied callee materialized the meeting
//! on the call edge, where the summary solver seeded it as a fresh callee
//! entry context: the seed witness severed the caller prefix holding the
//! source injection, origin reconstruction found no origin evidence, and the
//! public report projection dropped the finding. The solver now publishes
//! observation facts crossing a call edge in the calling context at the call
//! point, and the witness lookup accepts a retained quality that dominates a
//! prefix-conjoined request.
//!
//! These tests therefore assert on the public policy report, not on the
//! diagnostic-neutral `taint_findings()` rows, which retained the flow in
//! both cases.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::semantic_model::{
    CatalogCoordinate, CatalogOptions, CompiledSemanticModelPack, CompilerOptions,
    SemanticModelActivationEvidence, SemanticModelActivationRequest, SemanticModelRuntimeLimits,
    SemanticPackCatalog, SessionPackSource, SessionPackSourceKind, SourceFormat, compile_source,
};
use brokk_bifrost::policy::{
    PolicyBatchOutcome, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicySemanticModelContext, PolicySourceIdentity, evaluate_policy_inputs_with_analyzer,
    evaluate_policy_inputs_with_analyzer_and_semantic_models,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

const MODEL_ARTIFACT_SHA256: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const NATIVE_SINK_SOURCE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);
  void run() {
    sensitive(attacker());
  }
}
"#;

const BODIED_SINK_SOURCE: &str = r#"class App {
  static native String attacker();
  static void sensitive(String value) { }
  void run() {
    sensitive(attacker());
  }
}
"#;

const NEGATIVE_BODIED_SINK_SOURCE: &str = r#"class App {
  static native String attacker();
  static void sensitive(String value) { }
  void run() {
    attacker();
    sensitive("constant");
  }
}
"#;

const SUMMARY_NATIVE_SINK_SOURCE: &str = r#"class App {
  static native String attacker();
  static native void sensitive(String value);
  native String relay(String value);
  void run() {
    sensitive(this.relay(attacker()));
  }
}
"#;

const SUMMARY_BODIED_SINK_SOURCE: &str = r#"class App {
  static native String attacker();
  static void sensitive(String value) { }
  native String relay(String value);
  void run() {
    sensitive(this.relay(attacker()));
  }
}
"#;

fn policy_source(unmodeled: &str) -> String {
    format!(
        r#"(policy
  :schema-version 1
  :id "test.issue-1917"
  :name "Direct source to sink"
  :message "direct taint"
  :severity warning
  :analysis (analysis
    :type taint
    :mode may
    :call-modeling (call-modeling :unmodeled {unmodeled})
    :sources (endpoint-set :entries [
      (source :id attacker :display-name "attacker" :categories [input.user]
        :selector (rql :schema-version 1
          (language java (call :callee (name "attacker"))))
        :bind return-value :labels [user-input])])
    :sinks (endpoint-set :entries [
      (sink :id sensitive :display-name "sensitive" :categories [data.sensitive]
        :selector (rql :schema-version 1
          (language java (call :callee (name "sensitive"))))
        :dangerous-operand (argument :index 0) :accepts [user-input])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Test" :id "ISSUE-1917")))"#
    )
}

/// The public policy report's finding count: the count a host consumes. The
/// diagnostic-neutral `taint_findings()` rows retained the flow even while the
/// public report dropped it, so the report is the surface under test.
fn report_finding_count(outcome: &PolicyBatchOutcome) -> usize {
    let [run] = outcome.report().runs() else {
        panic!(
            "expected exactly one policy run, got {:?}",
            outcome.report().runs()
        );
    };
    run.findings().len()
}

fn direct_finding_count(fixture: &str) -> usize {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", fixture)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let policy = policy_source("optimistic");
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:issue-1917.rqlp"),
        &policy,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed evaluation date"),
    );
    let outcome =
        evaluate_policy_inputs_with_analyzer(project.root(), &inputs, &workspace, &options, None)
            .expect("production taint evaluation");
    report_finding_count(&outcome)
}

/// Build a session pack whose one summary is a complete parameter-to-return
/// transfer over the native `relay` helper, mirroring the #1871 foundry M3
/// fixture shape that recorded this defect.
fn relay_pack() -> CompiledSemanticModelPack {
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.issue-1917-relay",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "issue-1917" },
        "license": "Apache-2.0",
        "completeness": "complete",
        "safety": { "generated_code_only": false, "review_required": false },
        "shards": [{
            "id": "summaries.relay",
            "activation": [{
                "package": { "name": "com.acme:external", "version": ">=1.0.0, <2.0.0" },
                "targets": ["jvm"],
                "configurations": ["release"],
                "artifact_sha256": MODEL_ARTIFACT_SHA256
            }],
            "payload": { "kind": "procedure_summaries", "summaries": [{
                "id": "summary.relay",
                "target": {
                    "path": "app.java",
                    "symbol": "relay(String)",
                    "has_receiver": true,
                    "parameter_count": 1
                },
                "completeness": "complete",
                "transfers": [{
                    "input": { "kind": "parameter", "ordinal": 0 },
                    "exit_kind": "normal",
                    "output": { "kind": "normal_return" }
                }],
                "effects": []
            }] }
        }]
    }))
    .unwrap();
    compile_source(SourceFormat::Json, &source, &CompilerOptions::default())
        .unwrap_or_else(|diagnostics| panic!("relay pack failed: {diagnostics:#?}"))
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

fn summary_finding_count(fixture: &str) -> usize {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", fixture)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    catalog
        .register_session_pack(
            &relay_pack(),
            &SessionPackSource {
                kind: SessionPackSourceKind::Embedded,
                source_id: "test:issue-1917-relay".to_owned(),
            },
        )
        .expect("register the relay summary pack");
    let request = semantic_model_request();

    let policy = policy_source("require-model");
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:issue-1917.rqlp"),
        &policy,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 11).expect("fixed evaluation date"),
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
    .expect("production summary-bound taint evaluation");
    report_finding_count(&outcome)
}

#[test]
fn a_native_sink_reports_the_direct_flow() {
    assert_eq!(direct_finding_count(NATIVE_SINK_SOURCE), 1);
}

#[test]
fn a_bodied_sink_reports_the_same_direct_flow() {
    assert_eq!(direct_finding_count(BODIED_SINK_SOURCE), 1);
}

#[test]
fn a_bodied_sink_does_not_report_an_untainted_argument() {
    assert_eq!(direct_finding_count(NEGATIVE_BODIED_SINK_SOURCE), 0);
}

#[test]
fn a_native_sink_reports_the_summary_relayed_flow() {
    assert_eq!(summary_finding_count(SUMMARY_NATIVE_SINK_SOURCE), 1);
}

#[test]
fn a_bodied_sink_reports_the_same_summary_relayed_flow() {
    assert_eq!(summary_finding_count(SUMMARY_BODIED_SINK_SOURCE), 1);
}
