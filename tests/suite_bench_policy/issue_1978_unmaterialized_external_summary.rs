//! Acceptance for issue #1978: a fully-qualified external callee that never
//! materializes to a workspace or classpath artifact is modeled only by an
//! activated authored procedure summary.
//!
//! The fixture calls `com.example.Ext.wrap(...)`. No `com.example.Ext` type
//! exists in the workspace and there is no classpath, so the call resolves to an
//! external boundary with no materialized target. Under `require-model`:
//!
//!   * With the summary active, the summary's parameter-0 -> normal-return
//!     transfer carries taint from `source()` through `wrap` to `sink()`, and the
//!     run concludes with one finding.
//!   * With no summary active, the same call is unmodeled, the flow abstains at
//!     the external boundary, and the run is inconclusive with no finding.
//!
//! Scope boundary (stated in the issue): only fully-qualified callees, whose
//! owner FQN is present verbatim in the callee text, are in scope. Instance
//! transforms whose owner needs type resolution (`s.trim()`) and import-qualified
//! calls (`Ext.wrap`) are out of scope and are not exercised here.

use brokk_bifrost::analyzer::semantic_model::{
    CatalogCoordinate, CatalogOptions, CompiledSemanticModelPack, CompilerOptions,
    SemanticModelActivationEvidence, SemanticModelActivationRequest, SemanticModelRuntimeLimits,
    SemanticPackCatalog, SessionPackSource, SessionPackSourceKind, SourceFormat, compile_source,
};
use brokk_bifrost::policy::{
    PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions, PolicyIncompleteReason,
    PolicyRunCompletion, PolicySemanticModelContext, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer_and_semantic_models,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

use crate::common::InlineTestProject;

const MODEL_ARTIFACT_SHA256: &str =
    "1978197819781978197819781978197819781978197819781978197819781978";

const FIXTURE_SOURCE: &str = r#"class App {
  static native String source();
  static native void sink(String value);
  void run() {
    sink(com.example.Ext.wrap(source()));
  }
}
"#;

const POLICY_SOURCE: &str = r#"(policy
  :schema-version 1
  :id "test.issue-1978-unmaterialized-external-summary"
  :name "Unmaterialized external summary carries taint"
  :message "issue 1978 unmaterialized external summary taint"
  :severity warning
  :analysis (analysis
    :type taint
    :mode may
    :call-modeling (call-modeling :unmodeled require-model)
    :sources (endpoint-set :entries [
      (source :id user-source :display-name "user source" :categories [input.user]
        :selector (rql :schema-version 1
          (language java (call :callee (name "source"))))
        :bind return-value :labels [untrusted])])
    :sinks (endpoint-set :entries [
      (sink :id data-sink :display-name "data sink" :categories [data.sensitive]
        :selector (rql :schema-version 1
          (language java (call :callee (name "sink"))))
        :dangerous-operand (argument :index 0) :accepts [untrusted])]))
  :classification (classification
    :fallback (classification-id :taxonomy "Test" :id "ISSUE-1978")))"#;

#[test]
fn an_unmaterialized_external_summary_carries_taint_and_abstains_without_it() {
    // With the summary active: one finding whose reached label set is `untrusted`.
    let active = run_case(true);
    assert_eq!(
        active,
        vec![vec!["untrusted".to_owned()]],
        "the activated summary must carry taint from the unmaterialized external callee to the sink"
    );

    // Without the summary: require-model abstains at the unmodeled external call.
    let inactive = run_case(false);
    assert!(
        inactive.is_empty(),
        "without the summary the unmaterialized external call is unmodeled and must not produce a finding"
    );
}

/// Runs the fixture and policy once, optionally activating the summary, and
/// returns the sorted reached-label set of each retained public finding. It also
/// asserts the run's completion contract for the active and control cases.
fn run_case(activate: bool) -> Vec<Vec<String>> {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", FIXTURE_SOURCE)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default())
        .expect("open an ephemeral semantic-pack catalog");
    if activate {
        catalog
            .register_session_pack(
                &wrap_pack(),
                &SessionPackSource {
                    kind: SessionPackSourceKind::Embedded,
                    source_id: "test:issue-1978-external-summary".to_owned(),
                },
            )
            .expect("register the issue-1978 procedure-summary pack");
    }
    let request = semantic_model_request();

    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:issue-1978-external-summary.rqlp"),
        POLICY_SOURCE,
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
    .expect("production issue-1978 taint evaluation");

    let run = &outcome.report().runs()[0];
    if activate {
        assert_eq!(
            run.findings().len(),
            1,
            "expected exactly one finding with the summary active: {:?}",
            run.diagnostics()
        );
    } else {
        assert!(
            matches!(
                run.completion(),
                PolicyRunCompletion::Inconclusive { reasons }
                    if reasons.contains(&PolicyIncompleteReason::PartialDiscovery)
            ),
            "expected an inconclusive require-model abstain without the summary: {:?}",
            run.completion()
        );
        assert!(run.findings().is_empty());
    }

    outcome
        .taint_findings()
        .iter()
        .map(|finding| {
            let mut labels = finding.reached_labels.clone();
            labels.sort_unstable();
            labels
        })
        .collect()
}

/// A pack with one procedure summary for the fully-qualified external method
/// `com.example.Ext.wrap`. The summary transfers parameter 0 to the normal
/// return. The target `symbol` is fully qualified with a parameter-typed
/// signature, exactly as a shipped library summary would spell it; the canonical
/// identity (owner `com.example.Ext`, member `wrap`, arity 1, with receiver) is
/// what binds it to the unmaterialized boundary. The Java IR models the
/// qualified type in `com.example.Ext.wrap(...)` as the call receiver, so the
/// summary declares `has_receiver: true` to match that boundary shape.
fn wrap_pack() -> CompiledSemanticModelPack {
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.issue-1978-unmaterialized-external-summary",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "issue-1978" },
        "license": "Apache-2.0",
        "completeness": "complete",
        "safety": { "generated_code_only": false, "review_required": false },
        "shards": [{
            "id": "summaries.wrap",
            "activation": [{
                "package": { "name": "com.acme:external", "version": ">=1.0.0, <2.0.0" },
                "targets": ["jvm"],
                "configurations": ["release"],
                "artifact_sha256": MODEL_ARTIFACT_SHA256
            }],
            "payload": { "kind": "procedure_summaries", "summaries": [{
                "id": "summary.wrap",
                "target": {
                    "path": "com/example/Ext.java",
                    "symbol": "com.example.Ext.wrap(java.lang.String)",
                    "has_receiver": true,
                    "parameter_count": 1
                },
                "completeness": "complete",
                "transfers": [{
                    "input":  { "kind": "parameter", "ordinal": 0 },
                    "exit_kind": "normal",
                    "output": { "kind": "normal_return" }
                }],
                "effects": []
            }] }
        }]
    }))
    .expect("serialize the issue-1978 pack source");
    compile_source(SourceFormat::Json, &source, &CompilerOptions::default())
        .unwrap_or_else(|diagnostics| panic!("issue-1978 pack failed to compile: {diagnostics:#?}"))
}

fn semantic_model_request() -> SemanticModelActivationRequest {
    SemanticModelActivationRequest {
        bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version parses"),
        evidence: vec![SemanticModelActivationEvidence {
            language: "java".to_owned(),
            ecosystem: "maven".to_owned(),
            package: Some(CatalogCoordinate {
                name: "com.acme:external".to_owned(),
                version: Some(Version::parse("1.5.0").expect("evidence version parses")),
            }),
            module: None,
            toolchain: Some(CatalogCoordinate {
                name: "jdk".to_owned(),
                version: Some(Version::parse("17.0.1").expect("toolchain version parses")),
            }),
            target: Some("jvm".to_owned()),
            configuration: Some("release".to_owned()),
            artifact_sha256: Some(MODEL_ARTIFACT_SHA256.to_owned()),
        }],
        controls: Vec::new(),
        limits: SemanticModelRuntimeLimits::default(),
    }
}
