//! Acceptance for the shipped golden-core JDK flow-through summaries (#1935
//! blocker 4).
//!
//! The converter reads the hand-authored golden candidates, drops any
//! duplicate-target candidate, and assembles one `procedure_summaries` pack
//! pinned on the `jdk` toolchain. Each summary is a pure propagation: it carries
//! `transfers` (input -> output) and no sanitize effect. This suite proves:
//!
//! 1. the conversion is deterministic (two runs are byte-identical);
//! 2. the checked-in pack under `semantic-packs/golden-core/` is exactly the
//!    generator's output, so a drift is caught here rather than at release;
//! 3. the audit report is the real conversion outcome (54 candidates, 0
//!    rejected, one pinned pack);
//! 4. a real shipped transform propagates taint end to end through the
//!    production route: with the summary active a `require-model` flow across
//!    `StringBuilder.append` reaches its sink and concludes, and without it the
//!    same flow abstains at the unmodeled call and reports none.
//!
//! Property 4 carries a real shipped entry's transfer set onto a synthetic
//! bodyless target, exactly as the sanitizer ship does: a summary binds only
//! against a declaration without a body, so the target is a synthetic in-file
//! declaration and the proof is that the shipped transfer set, applied to a
//! target of the shape the entry declares, binds through the production binder
//! and carries taint where it claims. Whether the transfer matches the real JDK
//! body is the golden authoring stage's question, not this suite's.

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
use brokk_bifrost::semantic_packs::summary_foundry::golden_pack::{
    GOLDEN_AUDIT_FILE_NAME, convert_golden_candidates, serialize_audit,
};
use brokk_bifrost::{AnalyzerConfig, Language};
use semver::Version;

use crate::common::InlineTestProject;

const MODEL_ARTIFACT_SHA256: &str =
    "1935193519351935193519351935193519351935193519351935193519351935";

/// The transform whose shipped transfer set property 4 drives end to end.
const APPEND_SYMBOL: &str = "java.lang.StringBuilder.append(java.lang.String)";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn candidates_dir() -> PathBuf {
    repo_root().join(".agents/foundry/candidates/golden")
}

fn shipped_dir() -> PathBuf {
    repo_root().join("semantic-packs/golden-core")
}

#[test]
fn the_conversion_is_deterministic() {
    let first = convert_golden_candidates(&candidates_dir()).expect("first conversion");
    let second = convert_golden_candidates(&candidates_dir()).expect("second conversion");
    assert_eq!(
        first, second,
        "two conversions over the same candidates must be identical"
    );
}

#[test]
fn the_audit_report_records_the_real_outcome() {
    let conversion = convert_golden_candidates(&candidates_dir()).expect("conversion");
    let audit = &conversion.audit;
    assert_eq!(audit.candidates_total, 54);
    assert_eq!(audit.shipped_summaries, 54);
    assert_eq!(audit.rejected, 0);
    assert!(audit.rejects.is_empty());
    assert_eq!(audit.packs.len(), 1);
    let pack = &audit.packs[0];
    assert_eq!(pack.pack_id, "bifrost.jdk-golden-summaries");
    assert!(
        pack.pinned,
        "the golden pack is pinned by the jdk toolchain"
    );
    assert_eq!(pack.shipped_summaries, 54);
}

#[test]
fn every_generated_pack_compiles_through_the_production_compiler() {
    let conversion = convert_golden_candidates(&candidates_dir()).expect("conversion");
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
fn the_checked_in_pack_matches_the_generator() {
    let conversion = convert_golden_candidates(&candidates_dir()).expect("conversion");
    let root = shipped_dir();
    for pack in &conversion.packs {
        let path = root.join(&pack.relative_path);
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read checked-in pack {}: {error}", path.display()));
        assert_eq!(
            on_disk,
            pack.source_json,
            "checked-in {} drifted from the generator; regenerate with golden-summary-pack",
            path.display()
        );
    }
    let audit_on_disk = std::fs::read_to_string(root.join(GOLDEN_AUDIT_FILE_NAME))
        .expect("read checked-in rejects.json");
    assert_eq!(
        audit_on_disk,
        serialize_audit(&conversion.audit),
        "checked-in rejects.json drifted from the generator"
    );
}

/// A shipped golden transform propagates taint end to end, and the same flow
/// abstains without it, proven through the production route with the real
/// shipped transfer set carried onto a synthetic target.
#[test]
fn a_shipped_transform_propagates_taint_end_to_end() {
    let conversion = convert_golden_candidates(&candidates_dir()).expect("conversion");
    let pack = &conversion.packs[0];
    let transfers = shipped_transfers(pack, APPEND_SYMBOL);
    // The audited StringBuilder.append(String) claim propagates the appended
    // argument (parameter 0) to the returned value, among its four transfers.
    assert!(
        transfers.iter().any(|transfer| {
            transfer["input"]["kind"] == "parameter"
                && transfer["input"]["ordinal"] == 0
                && transfer["output"]["kind"] == "normal_return"
        }),
        "append must carry parameter 0 to the return: {transfers:#?}"
    );

    // With the shipped transfer set active, the tainted argument reaches the sink
    // through the modeled transform and the run concludes with one finding.
    let findings = run_case(Some(&transfers));
    assert_eq!(
        findings,
        vec![FindingSummary {
            reached_labels: vec!["untrusted".to_owned()],
        }],
        "the modeled transform must carry taint to the sink"
    );

    // The fail-closed control: with no summary, require-model abstains at the
    // unmodeled `append` call, so nothing reaches the sink.
    let control = run_case(None);
    assert!(
        control.is_empty(),
        "without the summary the unmodeled transform must not carry taint: {control:#?}"
    );
}

/// Read the `transfers` of one shipped summary out of a generated pack, by
/// target symbol.
fn shipped_transfers(
    pack: &brokk_bifrost::semantic_packs::summary_foundry::golden_pack::GeneratedGoldenPack,
    symbol: &str,
) -> Vec<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(&pack.source_json).expect("a generated pack is valid JSON");
    let summaries = value["shards"][0]["payload"]["summaries"]
        .as_array()
        .expect("the shard carries summaries");
    let summary = summaries
        .iter()
        .find(|summary| summary["target"]["symbol"] == symbol)
        .unwrap_or_else(|| panic!("summary for {symbol} is shipped"));
    summary["transfers"]
        .as_array()
        .expect("transfers array")
        .clone()
}

/// One retained taint finding, reduced to the labels that reached a sink.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FindingSummary {
    reached_labels: Vec<String>,
}

const FIXTURE_SOURCE: &str = r#"class App {
  static native String source();
  static native void sink(String value);
  native String append(String value);
  void run() {
    sink(this.append(source()));
  }
}
"#;

/// Run the fixture with the modeled transform present (`Some(transfers)`) or
/// absent (`None`, the fail-closed control), and reduce the findings.
fn run_case(transfers: Option<&[serde_json::Value]>) -> Vec<FindingSummary> {
    let project = InlineTestProject::with_language(Language::Java)
        .file("app.java", FIXTURE_SOURCE)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());

    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    // With transfers present, register the summary that models `append`. The
    // fail-closed control registers nothing, so `append` stays unmodeled and
    // require-model must abstain at it.
    if let Some(transfers) = transfers {
        catalog
            .register_session_pack(
                &append_pack(transfers),
                &SessionPackSource {
                    kind: SessionPackSourceKind::Embedded,
                    source_id: "test:golden-transform-pack".to_owned(),
                },
            )
            .expect("register the golden-shape summary pack");
    }
    let request = semantic_model_request();

    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("test:golden-transform-pack.rqlp"),
        POLICY_SOURCE,
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
    .expect("production golden-transform taint evaluation");

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

const POLICY_SOURCE: &str = r#"(policy
  :schema-version 1
  :id "test.golden-transform-pack"
  :name "Golden transform carries taint"
  :message "golden transform taint"
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
    :fallback (classification-id :taxonomy "Test" :id "GOLDEN-TRANSFORM")))"#;

/// Build a session pack carrying the given transfers on the synthetic `append`
/// target.
fn append_pack(transfers: &[serde_json::Value]) -> CompiledSemanticModelPack {
    let summaries = serde_json::json!([{
        "id": "summary.append",
        "target": {
            "path": "app.java",
            "symbol": "append(String)",
            "has_receiver": true,
            "parameter_count": 1
        },
        "completeness": "complete",
        "transfers": transfers,
        "effects": []
    }]);
    let source = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "pack_id": "test.golden-transform-pack",
        "version": "1.0.0",
        "producer": { "name": "bifrost-test", "version": "1.0.0" },
        "language": "java",
        "ecosystem": "maven",
        "compatibility": {
            "bifrost": ">=0.8.0, <1.0.0",
            "toolchains": [{ "name": "jdk", "requirement": ">=17.0.0" }]
        },
        "provenance": { "source": "test:inline", "revision": "golden-transform" },
        "license": "Apache-2.0",
        "completeness": "complete",
        "safety": { "generated_code_only": false, "review_required": false },
        "shards": [{
            "id": "summaries.append",
            "activation": [{
                "package": { "name": "com.acme:external", "version": ">=1.0.0, <2.0.0" },
                "targets": ["jvm"],
                "configurations": ["release"],
                "artifact_sha256": MODEL_ARTIFACT_SHA256
            }],
            "payload": { "kind": "procedure_summaries", "summaries": summaries }
        }]
    }))
    .unwrap();
    compile_source(SourceFormat::Json, &source, &CompilerOptions::default())
        .unwrap_or_else(|diagnostics| panic!("golden-shape pack failed: {diagnostics:#?}"))
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
