//! Acceptance for issue #1923: a context-scoped sanitizer ships in a
//! `procedure_summaries` pack and neutralizes taint end to end.
//!
//! The workspace routes two tainted flows through one bodiless external method,
//! `escape`, whose only model is a shipped procedure summary. That summary
//! carries a `Sanitize` effect that removes the label `sql` on its
//! parameter-to-return transfer. A `require-model` taint policy proves the
//! mechanism through the production route
//! (`evaluate_policy_inputs_with_analyzer_and_semantic_models`), the same route
//! the M3/M4 foundry acceptance tests use:
//!
//! - The `sql`-labeled flow through `escape` yields no finding, because the
//!   modeled transfer removes `sql`.
//! - The `safe`-labeled flow, whose label the summary does not remove, still
//!   reaches its sink.
//!
//! A control pack with the same transfer but no `Sanitize` effect lets both
//! flows through, so the difference isolates the sanitizer.

use brokk_bifrost::analyzer::semantic_model::{
    CatalogCoordinate, CatalogOptions, CompiledSemanticModelPack, CompilerOptions,
    SemanticModelActivationEvidence, SemanticModelActivationRequest, SemanticModelRuntimeLimits,
    SemanticPackCatalog, SessionPackSource, SessionPackSourceKind, SourceFormat, compile_source,
};
use brokk_bifrost::policy::{
    PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicySemanticModelContext, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer_and_semantic_models,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

use crate::common::InlineTestProject;

const MODEL_ARTIFACT_SHA256: &str =
    "1923192319231923192319231923192319231923192319231923192319231923";

const FIXTURE_SOURCE: &str = r#"class App {
  static native String sqlSource();
  static native String safeSource();
  static native void sqlSink(String value);
  static native void safeSink(String value);
  native String escape(String value);
  void run() {
    sqlSink(this.escape(sqlSource()));
    safeSink(this.escape(safeSource()));
  }
}
"#;

/// One retained taint finding, reduced to the sink id and the labels that
/// reached it. This is enough to prove which flow survived.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FindingSummary {
    reached_labels: Vec<String>,
}

#[test]
fn a_shipped_sanitize_effect_neutralizes_its_label_and_leaves_others_flowing() {
    // With the sanitizer: only the label the summary does not remove reaches a
    // sink. The `sql` flow is neutralized on the modeled transfer.
    let sanitized = run_case(true, "sql", "safe");
    assert_eq!(
        sanitized,
        vec![FindingSummary {
            reached_labels: vec!["safe".to_owned()],
        }],
        "the sql label must be removed on the modeled transfer while safe still flows"
    );

    // Without the sanitizer, the same transfer carries both labels through, so
    // both sinks fire. The difference is exactly the sanitized `sql` flow.
    let unsanitized = run_case(false, "sql", "safe");
    assert_eq!(
        unsanitized,
        vec![
            FindingSummary {
                reached_labels: vec!["safe".to_owned()],
            },
            FindingSummary {
                reached_labels: vec!["sql".to_owned()],
            },
        ],
        "without the sanitize effect both flows must reach their sinks"
    );
}

/// The M4 sanitizer fixture (#1871) gates a held candidate into adversarial
/// probes over each context's breakout metacharacter. This proves that a k3
/// entry's `neutralizes: [context]` maps directly onto a shipped `Sanitize`
/// effect's `removes: [label]`: the gate decides which label the pack removes
/// and which one must still flow, and the taint run confirms both. The
/// mechanism proof is label-based on a synthetic bodyless target; proving a
/// real escaper body encodes the metacharacter stays M4's separate real-body
/// seam (a tracked follow-up), not this test.
#[cfg(feature = "release-tooling")]
#[test]
fn a_gated_k3_entry_maps_neutralizes_onto_removes_and_neutralizes_end_to_end() {
    use brokk_bifrost::semantic_packs::summary_foundry::sanitizer::{
        InjectionContext, SanitizerCompleteness, SanitizerEntry, SanitizerExpectation,
        SanitizerPort, SanitizerTarget, gate_sanitizer,
    };

    // A held-shape entry: the escaper neutralizes `sql` and explicitly does not
    // neutralize `html`.
    let entry = SanitizerEntry {
        target: SanitizerTarget {
            path: "app.java".to_owned(),
            symbol: "escape(String)".to_owned(),
            has_receiver: true,
            parameter_count: 1,
        },
        artifact: None,
        sanitized_input: SanitizerPort::Parameter { ordinal: 0 },
        output: serde_json::json!({ "kind": "normal_return" }),
        neutralizes: vec![InjectionContext::Sql],
        does_not_neutralize: vec![InjectionContext::Html],
        completeness: SanitizerCompleteness::Complete,
        rationale: None,
        confidence: None,
        citations: None,
    };

    // The gate turns the claim into adversarial probes over real breakout
    // metacharacters: the neutralized context must encode `'`, the surviving one
    // leaves `<` live. This is the adversarial-by-construction check.
    let suite = gate_sanitizer(&entry).expect("a sql-neutralizing entry gates");
    let neutralized = suite
        .probes
        .iter()
        .find(|probe| probe.expectation == SanitizerExpectation::Neutralized)
        .expect("one neutralized probe");
    let survives = suite
        .probes
        .iter()
        .find(|probe| probe.expectation == SanitizerExpectation::Survives)
        .expect("one survives probe");
    assert_eq!(neutralized.context, InjectionContext::Sql);
    assert_eq!(neutralized.metacharacter, "'");
    assert_eq!(survives.context, InjectionContext::Html);
    assert_eq!(survives.metacharacter, "<");

    // Map `neutralizes -> removes` and `does_not_neutralize -> survives`, then
    // prove the shipped pack removes exactly the neutralized label.
    let removed_label = neutralized.context.as_str();
    let surviving_label = survives.context.as_str();
    let findings = run_case(true, removed_label, surviving_label);
    assert_eq!(
        findings,
        vec![FindingSummary {
            reached_labels: vec![surviving_label.to_owned()],
        }],
        "the neutralized context's label is removed while the does-not-neutralize label still flows"
    );
}

fn run_case(
    include_sanitize: bool,
    removed_label: &str,
    surviving_label: &str,
) -> Vec<FindingSummary> {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", FIXTURE_SOURCE)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    let pack = escape_pack(include_sanitize, removed_label);
    catalog
        .register_session_pack(
            &pack,
            &SessionPackSource {
                kind: SessionPackSourceKind::Embedded,
                source_id: "test:sanitizer-summary-pack".to_owned(),
            },
        )
        .expect("register the sanitizer procedure-summary pack");
    let request = semantic_model_request();

    let policy_source = policy_source(removed_label, surviving_label);
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:sanitizer-summary-pack.rqlp"),
        &policy_source,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 8, 10).expect("fixed evaluation date"),
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
    .expect("production sanitizer-summary taint evaluation");

    let mut findings = outcome
        .taint_findings()
        .iter()
        .map(|finding| {
            let mut reached_labels = finding.reached_labels.clone();
            reached_labels.sort_unstable();
            FindingSummary { reached_labels }
        })
        .collect::<Vec<_>>();
    findings.sort_unstable();
    findings
}

fn policy_source(removed_label: &str, surviving_label: &str) -> String {
    // `sqlSource`/`sqlSink` carry the label the summary removes; the source and
    // sink names are fixed by the workspace, the labels are the variable.
    format!(
        r#"(policy
      :schema-version 1
      :id "test.sanitizer-summary-pack"
      :name "Shipped sanitizer neutralizes its label"
      :message "shipped sanitizer taint"
      :severity warning
      :analysis (analysis
        :type taint
        :mode may
        :call-modeling (call-modeling :unmodeled require-model)
        :sources (endpoint-set :entries [
          (source :id removed-source :display-name "removed source" :categories [input.user]
            :selector (rql :schema-version 1
              (language java (call :callee (name "sqlSource"))))
            :bind return-value :labels [{removed_label}])
          (source :id surviving-source :display-name "surviving source" :categories [input.user]
            :selector (rql :schema-version 1
              (language java (call :callee (name "safeSource"))))
            :bind return-value :labels [{surviving_label}])])
        :sinks (endpoint-set :entries [
          (sink :id removed-sink :display-name "removed sink" :categories [data.sensitive]
            :selector (rql :schema-version 1
              (language java (call :callee (name "sqlSink"))))
            :dangerous-operand (argument :index 0) :accepts [{removed_label}])
          (sink :id surviving-sink :display-name "surviving sink" :categories [data.sensitive]
            :selector (rql :schema-version 1
              (language java (call :callee (name "safeSink"))))
            :dangerous-operand (argument :index 0) :accepts [{surviving_label}])]))
      :classification (classification
        :fallback (classification-id :taxonomy "Test" :id "SANITIZER-SUMMARY")))"#
    )
}

fn escape_pack(include_sanitize: bool, removed_label: &str) -> CompiledSemanticModelPack {
    let mut effects = Vec::new();
    if include_sanitize {
        effects.push(serde_json::json!({
            "kind": "sanitize",
            "input": { "kind": "parameter", "ordinal": 0 },
            "output": { "kind": "normal_return" },
            "removes": [removed_label]
        }));
    }
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.sanitizer-summary-pack",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "issue-1923" },
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
                "effects": effects
            }] }
        }]
    }))
    .unwrap();
    compile_source(SourceFormat::Json, &source, &CompilerOptions::default())
        .unwrap_or_else(|diagnostics| panic!("sanitizer summary pack failed: {diagnostics:#?}"))
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
