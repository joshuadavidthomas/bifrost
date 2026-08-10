//! Acceptance for the shipped k3 sanitizer packs (#1871 Stage 4, #1923).
//!
//! The converter reads the audited candidate files, runs every entry through the
//! M4 adversarial mechanism gate, maps `neutralizes -> removes` onto a shipped
//! `Sanitize` effect, and assembles one `procedure_summaries` pack per artifact.
//! This suite proves four properties of that output:
//!
//! 1. the conversion is deterministic (two runs are byte-identical);
//! 2. the checked-in packs under `semantic-packs/sanitizers/` are exactly the
//!    generator's output, so a drift is caught here rather than at release;
//! 3. the audit report is the real audit outcome (64 candidates, the gate's
//!    reject count, the folded overloads, the shipped totals); and
//! 4. a shipped sanitizer neutralizes end to end through the production route:
//!    a label the entry removes yields no finding, and a label it lists under
//!    `does_not_neutralize` still reaches its sink.
//!
//! Property 4 proves the mechanism on a synthetic bodiless target, exactly as
//! #1923 does. It carries a real shipped entry's `removes` set. It does not
//! prove that the real escaper body encodes the breakout metacharacter; that is
//! M4's separate real-body seam and a tracked follow-up.

use std::path::PathBuf;

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
use brokk_bifrost::semantic_packs::summary_foundry::sanitizer_pack::{
    GeneratedSanitizerPack, SANITIZER_AUDIT_FILE_NAME, convert_sanitizer_candidates,
    serialize_audit,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

use crate::common::InlineTestProject;

const MODEL_ARTIFACT_SHA256: &str =
    "1923192319231923192319231923192319231923192319231923192319231923";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn candidates_dir() -> PathBuf {
    repo_root().join(".agents/foundry/candidates/sanitizers")
}

fn shipped_dir() -> PathBuf {
    repo_root().join("semantic-packs/sanitizers")
}

#[test]
fn the_conversion_is_deterministic() {
    let first = convert_sanitizer_candidates(&candidates_dir()).expect("first conversion");
    let second = convert_sanitizer_candidates(&candidates_dir()).expect("second conversion");
    assert_eq!(
        first, second,
        "two conversions over the same candidates must be identical"
    );
}

#[test]
fn the_audit_report_records_the_real_outcome() {
    let conversion = convert_sanitizer_candidates(&candidates_dir()).expect("conversion");
    let audit = &conversion.audit;

    // The corrected k3 batch: every candidate expresses as an adversarial probe
    // suite and none is rejected on a structural contradiction. If a future
    // batch reintroduces the URL-encoder-as-sql error, its does_not_neutralize
    // overlap makes it a contradiction and the count moves here.
    assert_eq!(audit.candidates_total, 64);
    assert_eq!(audit.gate_rejected, 0);
    assert!(audit.gate_rejects.is_empty());
    // 8 overloads collapse into identical-claim twins because a signature-less
    // symbol cannot carry two summaries on one target.
    assert_eq!(audit.folded_overloads, 8);
    assert_eq!(audit.shipped_summaries, 56);
    // One probe per claimed or disclaimed context over every shipped candidate.
    assert_eq!(audit.adversarial_probes, 768);

    // One pack per artifact: the JDK pack is byte-pinned by its toolchain, the
    // five library packs are staged unpinned.
    assert_eq!(audit.packs.len(), 6);
    let jdk = audit
        .packs
        .iter()
        .find(|pack| pack.pack_id == "bifrost.java-sanitizers")
        .expect("the JDK pack ships");
    assert!(jdk.pinned, "the JDK pack is pinned by the jdk toolchain");
    assert_eq!(jdk.shipped_summaries, 11);
    assert!(
        audit
            .packs
            .iter()
            .filter(|pack| pack.pack_id != "bifrost.java-sanitizers")
            .all(|pack| !pack.pinned),
        "every library pack is staged unpinned"
    );
}

#[test]
fn every_generated_pack_compiles_through_the_production_compiler() {
    let conversion = convert_sanitizer_candidates(&candidates_dir()).expect("conversion");
    for pack in &conversion.packs {
        compile_source(
            SourceFormat::Json,
            pack.source_json.as_bytes(),
            &CompilerOptions::default(),
        )
        .unwrap_or_else(|diagnostics| {
            panic!(
                "shipped pack {} failed to compile: {diagnostics:#?}",
                pack.pack_id
            )
        });
    }
}

#[test]
fn the_checked_in_packs_match_the_generator() {
    let conversion = convert_sanitizer_candidates(&candidates_dir()).expect("conversion");
    let root = shipped_dir();
    for pack in &conversion.packs {
        let path = root.join(&pack.relative_path);
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read checked-in pack {}: {error}", path.display()));
        assert_eq!(
            on_disk,
            pack.source_json,
            "checked-in {} drifted from the generator; regenerate with the sanitizer-pack command",
            path.display()
        );
    }
    let audit_on_disk = std::fs::read_to_string(root.join(SANITIZER_AUDIT_FILE_NAME))
        .expect("read checked-in rejects.json");
    assert_eq!(
        audit_on_disk,
        serialize_audit(&conversion.audit),
        "checked-in rejects.json drifted from the generator"
    );
}

/// A shipped sanitizer removes the label it neutralizes and leaves a
/// `does_not_neutralize` label flowing, proven through the production route with
/// the real shipped `removes` set carried onto a synthetic target.
#[test]
fn a_shipped_sanitizer_neutralizes_end_to_end() {
    let conversion = convert_sanitizer_candidates(&candidates_dir()).expect("conversion");
    let jdk = conversion
        .packs
        .iter()
        .find(|pack| pack.pack_id == "bifrost.java-sanitizers")
        .expect("the JDK pack ships");
    let removes = shipped_removes(jdk, "java.util.base64.encoder.encodetostring");

    // The audited Base64.encodeToString claim: it neutralizes `sql` and lists
    // `shell` under does_not_neutralize.
    assert!(removes.contains(&"sql".to_owned()), "sql is removed");
    assert!(
        !removes.contains(&"shell".to_owned()),
        "shell is a does-not-neutralize label"
    );

    // With the shipped removes set active, the sql flow is neutralized on the
    // modeled transfer while the shell flow still reaches its sink.
    let findings = run_case(&removes, "sql", "shell");
    assert_eq!(
        findings,
        vec![FindingSummary {
            reached_labels: vec!["shell".to_owned()],
        }],
        "the removed label must not reach its sink while the surviving label does"
    );

    // The fail-before control: with no removes, both flows reach their sinks.
    let control = run_case(&[], "sql", "shell");
    assert_eq!(
        control,
        vec![
            FindingSummary {
                reached_labels: vec!["shell".to_owned()],
            },
            FindingSummary {
                reached_labels: vec!["sql".to_owned()],
            },
        ],
        "without the sanitize effect both flows reach their sinks"
    );
}

/// Read the `removes` labels of one shipped summary out of a generated pack.
fn shipped_removes(pack: &GeneratedSanitizerPack, summary_id: &str) -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(&pack.source_json).expect("a generated pack is valid JSON");
    let summaries = value["shards"][0]["payload"]["summaries"]
        .as_array()
        .expect("the shard carries summaries");
    let summary = summaries
        .iter()
        .find(|summary| summary["id"] == summary_id)
        .unwrap_or_else(|| panic!("summary {summary_id} is shipped"));
    let effect = summary["effects"]
        .as_array()
        .expect("effects array")
        .iter()
        .find(|effect| effect["kind"] == "sanitize")
        .expect("a sanitize effect");
    effect["removes"]
        .as_array()
        .expect("removes array")
        .iter()
        .map(|label| label.as_str().expect("a string label").to_owned())
        .collect()
}

/// One retained taint finding, reduced to the labels that reached a sink.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FindingSummary {
    reached_labels: Vec<String>,
}

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

fn run_case(removes: &[String], removed_label: &str, surviving_label: &str) -> Vec<FindingSummary> {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", FIXTURE_SOURCE)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    let pack = escape_pack(removes);
    catalog
        .register_session_pack(
            &pack,
            &SessionPackSource {
                kind: SessionPackSourceKind::Embedded,
                source_id: "test:shipped-sanitizer-pack".to_owned(),
            },
        )
        .expect("register the shipped-shape sanitizer pack");
    let request = semantic_model_request();

    let policy_source = policy_source(removed_label, surviving_label);
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:shipped-sanitizer-pack.rqlp"),
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
    .expect("production shipped-sanitizer taint evaluation");

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
    format!(
        r#"(policy
      :schema-version 1
      :id "test.shipped-sanitizer-pack"
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
        :fallback (classification-id :taxonomy "Test" :id "SHIPPED-SANITIZER")))"#
    )
}

/// Build a session pack whose one summary carries the given `removes` set on a
/// parameter-to-return transfer, targeting the synthetic `escape` method. An
/// empty `removes` produces the fail-before control (transfer, no sanitize).
fn escape_pack(removes: &[String]) -> CompiledSemanticModelPack {
    let mut effects = Vec::new();
    if !removes.is_empty() {
        effects.push(serde_json::json!({
            "kind": "sanitize",
            "input": { "kind": "parameter", "ordinal": 0 },
            "output": { "kind": "normal_return" },
            "removes": removes
        }));
    }
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.shipped-sanitizer-pack",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "shipped-sanitizer" },
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
        .unwrap_or_else(|diagnostics| panic!("shipped-shape pack failed: {diagnostics:#?}"))
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
