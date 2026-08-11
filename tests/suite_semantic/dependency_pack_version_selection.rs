//! Version selection at pack activation is exact-match only (#1884).
//!
//! A pack activates only when its declared version requirement accepts the
//! exact version discovery found. A near miss never activates silently: it is
//! a typed diagnostic naming the installed and required versions, so a
//! workspace on JDK 17 with only a JDK 21 pack installed hears the rejection
//! instead of a bare "no pack found". A compatible range is honored only when
//! the pack spec declares one; nothing here infers or widens a requirement.

use std::fs;

use brokk_bifrost::analyzer::packs_document::{
    WORKSPACE_PACKS_DOCUMENT_PATH, activate_workspace_packs, load_workspace_packs_config_at,
};
use brokk_bifrost::analyzer::semantic_model::*;
use brokk_bifrost::{AnalyzerConfig, CancellationToken, Language};
use semver::Version;

use crate::common::InlineTestProject;

/// One complete authored JDK stdlib pack pinned to exactly 21.0.2, carrying a
/// `java.util.ArrayList` declaration so activation is observable through the
/// published overlay.
const JDK_21_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": false },
  "shards": [{
    "id": "jdk.core",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "declaration_facts",
      "types": [{
        "id": "jdk.java-util-arraylist",
        "name": "java.util.ArrayList",
        "type_kind": "class",
        "visibility": "public",
        "type_parameters": [],
        "hierarchy": [],
        "aliases": [],
        "extension_surfaces": [],
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/ArrayList.java",
          "symbol": "java.util.ArrayList"
        }
      }],
      "members": [],
      "relations": []
    }
  }]
}"#;

fn compiled_jdk_pack() -> CompiledSemanticModelPack {
    compile_source(
        SourceFormat::Json,
        JDK_21_PACK.as_bytes(),
        &CompilerOptions::default(),
    )
    .unwrap_or_else(|diagnostics| panic!("fixture compilation failed: {diagnostics:#?}"))
}

/// The same pack as `JDK_21_PACK`, except it sets `safety.review_required =
/// true`, the value every shipped pack declares. This fixture proves the
/// document's `enable` list (#1937) is what lets such a pack reach `Active`
/// through the shared document flow.
const JDK_21_REVIEW_REQUIRED_PACK: &str = r#"{
  "schema_version": 1,
  "pack_id": "fixture.jdk-gated",
  "version": "21.0.2",
  "producer": { "name": "bifrost-fixture", "version": "1.0.0" },
  "language": "java",
  "ecosystem": "jdk",
  "compatibility": {
    "bifrost": ">=0.8.0, <1.0.0",
    "toolchains": [{ "name": "jdk", "requirement": "=21.0.2" }]
  },
  "provenance": { "source": "checked-in test source", "revision": "fixture-v1" },
  "license": "GPL-2.0-only WITH Classpath-exception-2.0",
  "completeness": "complete",
  "safety": { "generated_code_only": false, "review_required": true },
  "shards": [{
    "id": "jdk.core",
    "activation": [{
      "toolchain": { "name": "jdk", "version": "=21.0.2" },
      "targets": ["jvm"]
    }],
    "payload": {
      "kind": "declaration_facts",
      "types": [{
        "id": "jdk.java-util-arraylist",
        "name": "java.util.ArrayList",
        "type_kind": "class",
        "visibility": "public",
        "type_parameters": [],
        "hierarchy": [],
        "aliases": [],
        "extension_surfaces": [],
        "locator": {
          "kind": "artifact",
          "path": "java.base/java/util/ArrayList.java",
          "symbol": "java.util.ArrayList"
        }
      }],
      "members": [],
      "relations": []
    }
  }]
}"#;

fn compiled_jdk_gated_pack() -> CompiledSemanticModelPack {
    compile_source(
        SourceFormat::Json,
        JDK_21_REVIEW_REQUIRED_PACK.as_bytes(),
        &CompilerOptions::default(),
    )
    .unwrap_or_else(|diagnostics| panic!("fixture compilation failed: {diagnostics:#?}"))
}

fn catalog_with_jdk_pack() -> SemanticPackCatalog {
    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    catalog
        .install(
            &compiled_jdk_pack(),
            &DurablePackSource {
                kind: DurablePackSourceKind::PreShipped,
                source_id: "test:fixture.jdk@21.0.2".to_owned(),
            },
        )
        .unwrap();
    catalog
}

/// The zero-artifact dependency a JDK home without `src.zip` resolves to:
/// exact toolchain version, nothing locally producible, so selection must go
/// through installed packs.
fn jdk_dependency(version: &str) -> ResolvedDependency {
    ResolvedDependency {
        id: format!("jdk:{version}"),
        evidence: SemanticModelActivationEvidence {
            language: "java".to_owned(),
            ecosystem: "jdk".to_owned(),
            package: None,
            module: None,
            toolchain: Some(CatalogCoordinate {
                name: "jdk".to_owned(),
                version: Some(Version::parse(version).unwrap()),
            }),
            target: Some("jvm".to_owned()),
            configuration: None,
            artifact_sha256: None,
        },
        provenance: vec![DependencyProvenance {
            key: "version".to_owned(),
            value: version.to_owned(),
        }],
        artifacts: Vec::new(),
    }
}

fn prepare(catalog: &SemanticPackCatalog, version: &str) -> DependencyPackPreparationOutcome {
    prepare_dependency_semantic_packs(
        catalog,
        &brokk_bifrost::analyzer::JvmDependencyPackAdapter,
        &[jdk_dependency(version)],
        &DependencyPackLimits::default(),
        None,
    )
}

#[test]
fn an_exact_jdk_toolchain_match_selects_the_installed_pack() {
    let catalog = catalog_with_jdk_pack();
    let outcome = prepare(&catalog, "21.0.2");
    assert!(outcome.complete, "{outcome:#?}");
    assert_eq!(outcome.installed_packs.len(), 1);
    assert_eq!(outcome.installed_packs[0].dependency_id, "jdk:21.0.2");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
}

#[test]
fn a_jdk_version_near_miss_never_activates_and_names_both_versions() {
    let catalog = catalog_with_jdk_pack();
    let outcome = prepare(&catalog, "17.0.10");
    assert!(!outcome.complete);
    assert!(outcome.installed_packs.is_empty(), "{outcome:#?}");
    assert!(outcome.evidence.is_empty());
    let diagnostics = &outcome.diagnostics;
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].code, "dependency.pack_version_mismatch");
    assert_eq!(diagnostics[0].dependency_id.as_deref(), Some("jdk:17.0.10"));
    assert!(
        diagnostics[0].message.contains("17.0.10") && diagnostics[0].message.contains("=21.0.2"),
        "the near-miss diagnostic must name the installed and required versions: {}",
        diagnostics[0].message
    );
    assert!(
        diagnostics[0].message.contains("fixture.jdk@21.0.2"),
        "the near-miss diagnostic must name the rejecting pack: {}",
        diagnostics[0].message
    );
}

#[test]
fn an_absent_jdk_pack_stays_a_pack_unavailable_diagnostic() {
    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    let outcome = prepare(&catalog, "17.0.10");
    assert!(!outcome.complete);
    assert!(outcome.installed_packs.is_empty());
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    assert_eq!(outcome.diagnostics[0].code, "dependency.pack_unavailable");
}

#[test]
fn runtime_activation_names_the_version_near_miss_in_its_explanation() {
    let catalog = catalog_with_jdk_pack();
    let request = SemanticModelActivationRequest {
        bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
        evidence: vec![jdk_dependency("17.0.10").evidence],
        controls: Vec::new(),
        limits: SemanticModelRuntimeLimits::default(),
    };
    let SemanticModelResolutionOutcome::Ready(active) =
        resolve_active_semantic_models(&catalog, &request, &CancellationToken::default())
    else {
        panic!("a complete mismatch is still a ready empty model set");
    };
    assert!(active.shards().is_empty());
    let explanation = active
        .activation_report()
        .explanations
        .iter()
        .find(|entry| entry.status == SemanticModelActivationStatus::Incompatible)
        .expect("the rejected pack must be explained");
    assert!(
        explanation.reason.contains("does not satisfy"),
        "{explanation:#?}"
    );
    assert!(
        explanation.reason.contains("17.0.10") && explanation.reason.contains("=21.0.2"),
        "the explanation must name the installed and required versions: {}",
        explanation.reason
    );
}

/// One fake JDK home: a `release` file declaring the exact version, no
/// `src.zip`, so discovery yields a zero-artifact dependency that can only be
/// served by an installed pack.
fn write_jdk_home(root: &std::path::Path, version: &str) -> std::path::PathBuf {
    let home = root.join(format!("jdk-{version}"));
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("release"),
        format!("JAVA_VERSION=\"{version}\"\n"),
    )
    .unwrap();
    home
}

fn hermetic_jvm_config(jdk_home: std::path::PathBuf) -> AnalyzerConfig {
    let mut config = AnalyzerConfig::default();
    config.jvm.dependency_discovery.mode = brokk_bifrost::JvmDependencyDiscoveryMode::Disabled;
    config.jvm.standard_library_discovery.discover_java_home = false;
    config.jvm.standard_library_discovery.jdk_homes = vec![jdk_home];
    config
}

/// End to end through the shared packs document (#1868 + #1884): the same
/// workspace, the same document, the same installed JDK 21 pack. On a JDK
/// 21.0.2 workspace the pack activates and the overlay indexes the external
/// surface; on a JDK 17 workspace nothing activates, the boundary stays
/// honest, and the outcome names the near miss.
#[test]
fn document_driven_jvm_activation_is_version_exact() {
    let project = InlineTestProject::with_language(Language::Java)
        .file("src/Main.java", "final class Main {}")
        .build();
    fs::create_dir_all(project.root().join(".bifrost")).unwrap();
    fs::write(
        project.root().join(WORKSPACE_PACKS_DOCUMENT_PATH),
        r#"{ "schema_version": 1, "catalog": ".bifrost/packs-catalog", "ecosystems": ["jvm"] }"#,
    )
    .unwrap();
    {
        let catalog = SemanticPackCatalog::open(
            &project.root().join(".bifrost/packs-catalog"),
            CatalogOpenMode::ReadWrite,
            CatalogOptions::default(),
        )
        .unwrap();
        catalog
            .install(
                &compiled_jdk_pack(),
                &DurablePackSource {
                    kind: DurablePackSourceKind::PreShipped,
                    source_id: "test:fixture.jdk@21.0.2".to_owned(),
                },
            )
            .unwrap();
    }
    let config = load_workspace_packs_config_at(project.root())
        .unwrap()
        .expect("packs document present");
    let homes = tempfile::tempdir().unwrap();

    // Near miss: JDK 17 workspace against the JDK 21 pack.
    let analyzer_config = hermetic_jvm_config(write_jdk_home(homes.path(), "17.0.10"));
    let workspace = project.workspace_analyzer(analyzer_config.clone());
    let activation = activate_workspace_packs(
        &workspace,
        &analyzer_config,
        project.root(),
        &config,
        &CancellationToken::default(),
    )
    .unwrap()
    .expect("the jvm ecosystem serves this workspace");
    assert!(!activation.outcome.complete(), "{:#?}", activation.outcome);
    let preparation = activation.outcome.ecosystems[0]
        .preparation
        .as_ref()
        .expect("discovery completed");
    let near_miss = preparation
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "dependency.pack_version_mismatch")
        .unwrap_or_else(|| panic!("{:#?}", preparation.diagnostics));
    assert!(
        near_miss.message.contains("17.0.10") && near_miss.message.contains("=21.0.2"),
        "{}",
        near_miss.message
    );
    assert!(
        workspace.analyzer().semantic_model_overlay().is_none(),
        "a near miss must not publish an overlay"
    );

    // Exact match: JDK 21.0.2 workspace activates the installed pack and the
    // overlay indexes the pack's external declaration.
    let analyzer_config = hermetic_jvm_config(write_jdk_home(homes.path(), "21.0.2"));
    let workspace = project.workspace_analyzer(analyzer_config.clone());
    let activation = activate_workspace_packs(
        &workspace,
        &analyzer_config,
        project.root(),
        &config,
        &CancellationToken::default(),
    )
    .unwrap()
    .expect("the jvm ecosystem serves this workspace");
    assert!(activation.outcome.complete(), "{:#?}", activation.outcome);
    let overlay = workspace
        .analyzer()
        .semantic_model_overlay()
        .expect("an exact match publishes the overlay");
    assert!(
        !overlay
            .symbols_named("java.util.ArrayList")
            .records
            .is_empty(),
        "the activated pack must index its external declaration"
    );
}

/// #1937: `.bifrost/packs.json` must be able to activate a
/// `review_required` pack, not only leave it selected. The document's
/// `enable` list is the control that does this, mirroring the in-process
/// `Enable` control in `owasp_benchmark.rs`. Without an `enable` entry, an
/// exact toolchain match still leaves the pack `ReviewRequired`, not
/// `Active`.
#[test]
fn document_driven_activation_needs_an_enable_entry_for_review_required_packs() {
    let project = InlineTestProject::with_language(Language::Java)
        .file("src/Main.java", "final class Main {}")
        .build();
    fs::create_dir_all(project.root().join(".bifrost")).unwrap();
    let catalog_relative = ".bifrost/packs-catalog";
    {
        let catalog = SemanticPackCatalog::open(
            &project.root().join(catalog_relative),
            CatalogOpenMode::ReadWrite,
            CatalogOptions::default(),
        )
        .unwrap();
        catalog
            .install(
                &compiled_jdk_gated_pack(),
                &DurablePackSource {
                    kind: DurablePackSourceKind::PreShipped,
                    source_id: "test:fixture.jdk-gated@21.0.2".to_owned(),
                },
            )
            .unwrap();
    }
    let homes = tempfile::tempdir().unwrap();
    let analyzer_config = hermetic_jvm_config(write_jdk_home(homes.path(), "21.0.2"));

    // Negative control: no `enable` entry. The toolchain evidence matches
    // exactly, but `review_required` still gates the pack out of the active
    // set.
    fs::write(
        project.root().join(WORKSPACE_PACKS_DOCUMENT_PATH),
        format!(
            r#"{{ "schema_version": 1, "catalog": "{catalog_relative}", "ecosystems": ["jvm"] }}"#
        ),
    )
    .unwrap();
    let config_without_enable = load_workspace_packs_config_at(project.root())
        .unwrap()
        .expect("packs document present");
    assert!(config_without_enable.enable().is_empty());
    let workspace = project.workspace_analyzer(analyzer_config.clone());
    let activation = activate_workspace_packs(
        &workspace,
        &analyzer_config,
        project.root(),
        &config_without_enable,
        &CancellationToken::default(),
    )
    .unwrap()
    .expect("the jvm ecosystem serves this workspace");
    let Some(SemanticModelRuntimeOutcome::Ready { active, .. }) = &activation.outcome.runtime
    else {
        panic!(
            "expected a ready runtime outcome: {:#?}",
            activation.outcome
        );
    };
    let explanation = active
        .activation_report()
        .explanations
        .iter()
        .find(|entry| entry.pack_id.as_deref() == Some("fixture.jdk-gated"))
        .expect("the gated pack must be explained");
    assert_eq!(
        explanation.status,
        SemanticModelActivationStatus::ReviewRequired,
        "{explanation:#?}"
    );

    // Positive: the document names the pack in `enable`, so the pack
    // reaches `Active` and the overlay indexes its declaration.
    fs::write(
        project.root().join(WORKSPACE_PACKS_DOCUMENT_PATH),
        format!(
            r#"{{ "schema_version": 1, "catalog": "{catalog_relative}", "ecosystems": ["jvm"], "enable": ["fixture.jdk-gated"] }}"#
        ),
    )
    .unwrap();
    let config_with_enable = load_workspace_packs_config_at(project.root())
        .unwrap()
        .expect("packs document present");
    assert_eq!(config_with_enable.enable(), ["fixture.jdk-gated"]);
    let workspace = project.workspace_analyzer(analyzer_config.clone());
    let activation = activate_workspace_packs(
        &workspace,
        &analyzer_config,
        project.root(),
        &config_with_enable,
        &CancellationToken::default(),
    )
    .unwrap()
    .expect("the jvm ecosystem serves this workspace");
    assert!(activation.outcome.complete(), "{:#?}", activation.outcome);
    let overlay = workspace
        .analyzer()
        .semantic_model_overlay()
        .expect("an enabled review-required pack publishes the overlay");
    assert!(
        !overlay
            .symbols_named("java.util.ArrayList")
            .records
            .is_empty(),
        "the enabled pack must index its external declaration"
    );
}
