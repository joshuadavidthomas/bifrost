use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use brokk_bifrost_analysis::analyzer::semantic_model::{
    ActivationSelector, ArtifactProducerLimits, CatalogOpenMode, CatalogOptions, Compatibility,
    CompilerOptions, DurablePackSource, DurablePackSourceKind, NameSelector, Provenance, Safety,
    SemanticPackCatalog, SourceFormat, VersionConstraint, compile_source, read_exact_source_set,
};
use brokk_bifrost_semantic_packs::release_bundle::{
    PACK_SPEC_SCHEMA_VERSION, PinnedArtifact, PinnedLookupQuery, PinnedPackKind, PinnedPackSpec,
};
use serde_json::{Value, json};
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/semantic-model-packs")
        .join(name)
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bifrost-semantic-pack"))
        .args(arguments)
        .output()
        .unwrap()
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error_value| {
        panic!(
            "invalid JSON output: {error_value}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn validate_lint_compile_and_workspace_check_have_stable_json_and_status() {
    let source = fixture("generator-rules-v1.yaml");
    let source_text = source.to_str().unwrap();
    let validate = run(&["validate", source_text, "--format", "json"]);
    assert!(validate.status.success(), "{validate:#?}");
    assert_eq!(
        json_output(&validate)["format"],
        "bifrost_semantic_model_validate/v1"
    );

    let lint = run(&["lint", source_text, "--format", "json"]);
    assert!(lint.status.success(), "{lint:#?}");
    assert_eq!(
        json_output(&lint)["format"],
        "bifrost_semantic_model_lint/v1"
    );

    let temporary = TempDir::new().unwrap();
    let output = temporary.path().join("compiled");
    let compile = run(&[
        "compile",
        source_text,
        output.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(compile.status.success(), "{compile:#?}");
    assert_eq!(
        json_output(&compile)["format"],
        "bifrost_semantic_model_write/v1"
    );
    let repeated = run(&[
        "compile",
        source_text,
        output.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(repeated.status.success(), "{repeated:#?}");

    let workspace_rules = temporary.path().join(".bifrost/semantic-models");
    fs::create_dir_all(&workspace_rules).unwrap();
    fs::copy(&source, workspace_rules.join("builders.yaml")).unwrap();
    let workspace = run(&[
        "workspace-check",
        temporary.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(workspace.status.success(), "{workspace:#?}");
    let workspace = json_output(&workspace);
    assert_eq!(workspace["format"], "bifrost_semantic_model_workspace/v1");
    assert_eq!(workspace["enabled"], true);
    assert_eq!(
        workspace["files"][0]["path"],
        ".bifrost/semantic-models/builders.yaml"
    );
}

#[test]
fn invalid_source_and_arguments_return_versioned_nonzero_results() {
    let temporary = TempDir::new().unwrap();
    let invalid = temporary.path().join("invalid.json");
    fs::write(&invalid, b"{}").unwrap();
    let validate = run(&["validate", invalid.to_str().unwrap(), "--format", "json"]);
    assert_eq!(validate.status.code(), Some(1));
    assert_eq!(
        json_output(&validate)["format"],
        "bifrost_semantic_model_validate/v1"
    );

    let arguments = run(&["validate", "--format", "json"]);
    assert_eq!(arguments.status.code(), Some(2));
    assert_eq!(
        json_output(&arguments)["format"],
        "bifrost_semantic_model_cli_error/v1"
    );
}

#[test]
fn generate_and_verify_python_stub_bundle_and_list_extraction_rejects() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path().join("stubs");
    fs::create_dir_all(root.join("pkg")).unwrap();
    fs::write(
        root.join("pkg/__init__.pyi"),
        "class Widget: ...\ndef make_widget() -> Widget: ...\n",
    )
    .unwrap();
    // One deliberately rejected stub: not UTF-8, so extraction must record a
    // structured reject instead of silently dropping the entry.
    fs::write(root.join("pkg/bad.pyi"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    fs::write(temporary.path().join("NOTICE.txt"), "fixture notice\n").unwrap();
    let stubs = vec!["pkg/__init__.pyi".to_owned(), "pkg/bad.pyi".to_owned()];
    let artifact_sha256 = read_exact_source_set(
        &root,
        &stubs.iter().map(PathBuf::from).collect::<Vec<_>>(),
        16,
        8,
        &ArtifactProducerLimits::default(),
    )
    .unwrap()
    .sha256()
    .to_owned();
    let selector = ActivationSelector {
        package: Some(NameSelector {
            name: "widget-stubs".to_owned(),
            version: Some("=1.0.0".to_owned()),
        }),
        module: None,
        toolchain: Some(NameSelector {
            name: "python".to_owned(),
            version: Some("=1.0.0".to_owned()),
        }),
        targets: Vec::new(),
        configurations: Vec::new(),
        artifact_sha256: None,
    };
    let spec = PinnedPackSpec {
        schema_version: PACK_SPEC_SCHEMA_VERSION,
        pack_id: "python-stubs-cli".to_owned(),
        pack_version: "1.0.0".to_owned(),
        ecosystem: "pypi".to_owned(),
        kind: PinnedPackKind::PythonStub { stubs },
        artifact: PinnedArtifact {
            file_name: "stubs".to_owned(),
            sha256: artifact_sha256,
            url: Some("https://example.invalid/widget-stubs".to_owned()),
            container: None,
        },
        compatibility: Compatibility {
            bifrost: format!("={}", env!("CARGO_PKG_VERSION")),
            toolchains: vec![VersionConstraint {
                name: "python".to_owned(),
                requirement: "=1.0.0".to_owned(),
            }],
        },
        activation: vec![selector.clone()],
        provenance: Provenance {
            source: "fixture".to_owned(),
            revision: Some("fixture-v1".to_owned()),
        },
        license: "Apache-2.0".to_owned(),
        safety: Safety {
            generated_code_only: false,
            review_required: false,
        },
        notices: vec!["NOTICE.txt".to_owned()],
        measurement_activation: selector,
        measurement_queries: vec![PinnedLookupQuery::Type {
            name: "pkg.Widget".to_owned(),
        }],
    };
    let spec_path = temporary.path().join("python-stubs.json");
    fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let output_root = temporary.path().join("bundle");

    let generate = run(&[
        "generate",
        output_root.to_str().unwrap(),
        spec_path.to_str().unwrap(),
        root.to_str().unwrap(),
    ]);
    assert!(generate.status.success(), "{generate:#?}");
    let stdout = String::from_utf8(generate.stdout).unwrap();
    assert!(
        stdout.contains("generated 1 pinned semantic packs"),
        "{stdout}"
    );
    assert!(
        stdout.contains("rejects python-stubs-cli@1.0.0: 1 rejected entries, 0 suppressed"),
        "{stdout}"
    );
    assert!(
        stdout.contains("warning python.source.encoding pkg/bad.pyi:"),
        "{stdout}"
    );

    let verify = run(&["verify", output_root.to_str().unwrap()]);
    assert!(verify.status.success(), "{verify:#?}");
    let stdout = String::from_utf8(verify.stdout).unwrap();
    assert!(
        stdout.contains("verified 1 pinned semantic packs"),
        "{stdout}"
    );
    assert!(
        stdout.contains("warning python.source.encoding pkg/bad.pyi:"),
        "{stdout}"
    );
}

#[test]
fn list_reports_installed_and_evidence_backed_active_packs() {
    let temporary = TempDir::new().unwrap();
    let catalog_root = temporary.path().join("catalog");
    let catalog = SemanticPackCatalog::open(
        &catalog_root,
        CatalogOpenMode::ReadWrite,
        CatalogOptions::default(),
    )
    .unwrap();
    let source = fs::read(fixture("declarations-v1.json")).unwrap();
    let compiled =
        compile_source(SourceFormat::Json, &source, &CompilerOptions::default()).unwrap();
    catalog
        .install(
            &compiled,
            &DurablePackSource {
                kind: DurablePackSourceKind::Installed,
                source_id: "cli-test".to_owned(),
            },
        )
        .unwrap();
    drop(catalog);

    let activation = temporary.path().join("activation.json");
    fs::write(
        &activation,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "bifrost_version": "0.8.21",
            "evidence": [{
                "language": "java",
                "ecosystem": "maven",
                "package": {"name": "com.acme:widget", "version": "1.5.0"},
                "toolchain": {"name": "jdk", "version": "17.0.1"},
                "target": "jvm",
                "configuration": "release"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let output = run(&[
        "list",
        catalog_root.to_str().unwrap(),
        activation.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(output.status.success(), "{output:#?}");
    let report = json_output(&output);
    assert_eq!(report["format"], "bifrost_semantic_model_inventory/v1");
    assert_eq!(report["installed"][0]["pack_id"], "acme.widget");
    assert_eq!(report["active"][0]["pack_id"], "acme.widget");
    assert_eq!(
        report["active"][0]["matched_evidence"]["package"]["name"],
        "com.acme:widget"
    );
}
