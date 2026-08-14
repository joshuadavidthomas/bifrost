use brokk_bifrost_runtime::extension::*;
use std::fs;

fn digest(c: char) -> StableDigest {
    StableDigest::parse(c.to_string().repeat(64)).unwrap()
}
fn generation() -> WorkspaceGeneration {
    serde_json::from_str(&format!("\"{}\"", "a".repeat(64))).unwrap()
}
fn description() -> ExtensionWorkspaceDescription {
    let generation = generation();
    ExtensionWorkspaceDescription {
        api: EXTENSION_API_VERSION,
        generation: generation.clone(),
        capabilities: ExtensionCapabilityReport {
            generation,
            languages: Box::new([]),
            operations: Box::new([]),
        },
    }
}
fn builder(purpose: RunPurpose) -> RunManifestBuilder {
    RunManifestBuilder::from_workspace(
        &description(),
        EngineRunIdentity {
            package_version: "0.9.4".into(),
            commit: "1".repeat(40).into(),
            dirty_tree: None,
            profile: "release".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            features: vec!["python".into()].into_boxed_slice(),
            extension_api: EXTENSION_API_VERSION,
            semantic_ir_versions: vec!["1.0".into()].into_boxed_slice(),
            adapter_identities: vec!["java:1".into()].into_boxed_slice(),
            capability_report_digest: digest('b'),
        },
        WorkspaceRunIdentity {
            repository: "https://example.invalid/repo".into(),
            commit: "2".repeat(40).into(),
            dirty_tree: None,
            generation: generation(),
            source_inventory_digest: digest('c'),
            roots: vec![".".into()].into_boxed_slice(),
            exclusions: Box::new([]),
            dependency_fingerprints: Box::new([]),
        },
        ExtensionRunIdentity {
            name: "example".into(),
            version: "1.2.3".into(),
            commit: Some("3".repeat(40).into()),
            package_digest: None,
            configuration_digest: digest('d'),
        },
        purpose,
        CacheStateDeclaration {
            kind: CacheStateKind::FullyCold,
            same_process: false,
            persisted_source: false,
            semantic_artifact_reused: false,
            warmup_count: 0,
        },
    )
    .unwrap()
}
fn complete_manifest() -> (ExtensionRunManifest, Vec<u8>) {
    let bytes = b"expectation\n".to_vec();
    let manifest = builder(RunPurpose::Conformance {
        expectation_role: "expectations".into(),
        comparison_role: "comparison".into(),
    })
    .add_component(component(
        "comparison",
        "results/comparison.json",
        b"comparison\n",
        RunStatus::Complete,
    ))
    .unwrap()
    .add_component(component(
        "expectations",
        "protocols/expectations.json",
        &bytes,
        RunStatus::Complete,
    ))
    .unwrap()
    .build(RunStatus::Complete)
    .unwrap();
    (manifest, bytes)
}

fn component<'a>(
    role: &str,
    path: &str,
    bytes: &'a [u8],
    status: RunStatus,
) -> RunComponentInput<'a> {
    RunComponentInput {
        role: role.into(),
        path: NormalizedRelativePath::new(path).unwrap(),
        media_type: "application/json".into(),
        schema: None,
        bytes,
        canonical_digest: None,
        status,
        dependencies: vec![],
    }
}

#[test]
fn canonical_manifest_separates_volatile_measurements() {
    let (first, _) = complete_manifest();
    let second = builder(first.purpose.clone())
        .add_component(component(
            "comparison",
            "results/comparison.json",
            b"comparison\n",
            RunStatus::Complete,
        ))
        .unwrap()
        .add_component(component(
            "expectations",
            "protocols/expectations.json",
            b"expectation\n",
            RunStatus::Complete,
        ))
        .unwrap()
        .volatile(VolatileRunMeasurements {
            elapsed_millis: Some(42),
            ..Default::default()
        })
        .build(RunStatus::Complete)
        .unwrap();
    assert_eq!(first.manifest_digest, second.manifest_digest);
    assert_ne!(
        encode_run_manifest_json(&first).unwrap(),
        encode_run_manifest_json(&second).unwrap()
    );
    let encoded = encode_run_manifest_json(&second).unwrap();
    assert_eq!(
        decode_canonical_run_manifest_json(&encoded).unwrap(),
        second
    );
}

#[test]
fn complete_rejects_incomplete_components_and_deviations() {
    let error = builder(RunPurpose::DevelopmentExperiment {
        objective: "probe".into(),
    })
    .add_component(component(
        "result",
        "results/result.json",
        b"{}\n",
        RunStatus::Incomplete,
    ))
    .unwrap()
    .build(RunStatus::Complete)
    .unwrap_err();
    assert_eq!(error.path.as_ref(), "status");
}

#[test]
fn bundle_verification_rejects_tampering() {
    let root = tempfile::tempdir().unwrap();
    let (manifest, expectation) = complete_manifest();
    fs::create_dir_all(root.path().join("protocols")).unwrap();
    fs::create_dir_all(root.path().join("results")).unwrap();
    fs::write(root.path().join("protocols/expectations.json"), expectation).unwrap();
    fs::write(root.path().join("results/comparison.json"), b"comparison\n").unwrap();
    fs::write(
        root.path().join("manifest.json"),
        encode_run_manifest_json(&manifest).unwrap(),
    )
    .unwrap();
    assert!(verify_extension_bundle(root.path(), BundleVerificationLimits::default()).is_ok());
    fs::write(root.path().join("results/comparison.json"), b"changed\n").unwrap();
    let errors =
        verify_extension_bundle(root.path(), BundleVerificationLimits::default()).unwrap_err();
    assert_eq!(errors[0].path.as_ref(), "results/comparison.json");
}

struct Resolver;
impl ReproductionResolver for Resolver {
    fn compare(&self, _: &ExtensionRunManifest) -> Vec<ReproductionMismatch> {
        vec![
            ReproductionMismatch {
                kind: ReproductionMismatchKind::Workspace,
                path: "workspace.commit".into(),
                expected: "a".into(),
                observed: Some("b".into()),
                remediation: "provide exact source".into(),
            },
            ReproductionMismatch {
                kind: ReproductionMismatchKind::Engine,
                path: "engine.features".into(),
                expected: "python".into(),
                observed: Some("none".into()),
                remediation: "provide exact engine".into(),
            },
        ]
    }
}

#[test]
fn reproduction_preflight_returns_all_mismatches_in_canonical_order() {
    let root = tempfile::tempdir().unwrap();
    let (manifest, expectation) = complete_manifest();
    fs::create_dir_all(root.path().join("protocols")).unwrap();
    fs::create_dir_all(root.path().join("results")).unwrap();
    fs::write(root.path().join("protocols/expectations.json"), expectation).unwrap();
    fs::write(root.path().join("results/comparison.json"), b"comparison\n").unwrap();
    fs::write(
        root.path().join("manifest.json"),
        encode_run_manifest_json(&manifest).unwrap(),
    )
    .unwrap();
    let verified =
        verify_extension_bundle(root.path(), BundleVerificationLimits::default()).unwrap();
    let report = match plan_reproduction(&verified, &Resolver) {
        Ok(_) => panic!("expected mismatches"),
        Err(report) => report,
    };
    assert_eq!(report.mismatches.len(), 2);
    assert_eq!(report.mismatches[0].path.as_ref(), "engine.features");
}
