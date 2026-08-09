use brokk_bifrost::analyzer::semantic_model::{
    ActivationSelector, ArtifactProducerLimits, ArtifactProductionRequest, AuthoredPayload,
    CatalogOptions, Compatibility, DeclarationGuard, DependencyPackLimits, ExternalArtifactKind,
    ExternalArtifactPackProducer, GuardVersion, Provenance, ResolvedActiveSemanticModels, Safety,
    SemanticModelActivationRequest, SemanticModelResolutionOutcome, SemanticModelRuntimeLimits,
    SemanticPackCatalog, prepare_discovered_dependency_semantic_packs,
    resolve_active_semantic_models,
};
use brokk_bifrost::analyzer::{
    AnalyzerConfig, PythonAnalyzerConfig, PythonArtifactPackProducer, PythonDependencyPackAdapter,
    PythonEnvironmentConfig, PythonEnvironmentLimits, PythonSemanticModelWorkspaceContext,
    resolve_python_semantic_pack_dependencies,
};
use brokk_bifrost::{CancellationToken, Language, Project};
use semver::Version;

use crate::common::InlineTestProject;

fn environment(
    standard_library_root: std::path::PathBuf,
    distribution_root: std::path::PathBuf,
) -> PythonAnalyzerConfig {
    PythonAnalyzerConfig {
        environment: Some(PythonEnvironmentConfig {
            implementation: "cpython".to_owned(),
            version: "3.12.3".to_owned(),
            platform: "darwin".to_owned(),
            standard_library_root,
            bundled_stub_roots: Vec::new(),
            distribution_roots: vec![distribution_root],
            limits: PythonEnvironmentLimits::default(),
        }),
    }
}

fn write_distribution(
    root: &std::path::Path,
    metadata_name: &str,
    version: &str,
    top_level: &str,
    source: &str,
    typed: bool,
) {
    let package = root.join(top_level);
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("__init__.py"), source).unwrap();
    if typed {
        std::fs::write(package.join("py.typed"), "").unwrap();
    }
    let metadata = root.join(format!("{metadata_name}-{version}.dist-info"));
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::write(
        metadata.join("METADATA"),
        format!("Name: {metadata_name}\nVersion: {version}\n"),
    )
    .unwrap();
    std::fs::write(metadata.join("top_level.txt"), format!("{top_level}\n")).unwrap();
}

fn artifact_request(
    path: std::path::PathBuf,
    kind: ExternalArtifactKind,
) -> ArtifactProductionRequest {
    ArtifactProductionRequest {
        path,
        artifact_kind: kind,
        pack_id: "test.python".to_owned(),
        pack_version: "1.0.0".to_owned(),
        ecosystem: "python".to_owned(),
        compatibility: Compatibility {
            bifrost: "*".to_owned(),
            toolchains: Vec::new(),
        },
        activation: vec![ActivationSelector {
            package: None,
            module: None,
            toolchain: None,
            targets: Vec::new(),
            configurations: Vec::new(),
            artifact_sha256: None,
        }],
        provenance: Provenance {
            source: "test".to_owned(),
            revision: None,
        },
        license: "NOASSERTION".to_owned(),
        safety: Safety {
            generated_code_only: false,
            review_required: false,
        },
    }
}

#[test]
fn explicitly_configured_environment_discovers_static_artifacts_without_expanding_workspace() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import alpha\n")
        .build();
    let files_before = workspace.project().all_files().unwrap();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(&standard_library).unwrap();
    std::fs::create_dir_all(&distributions).unwrap();
    std::fs::write(
        standard_library.join("re.pyi"),
        "def compile(pattern: str) -> Pattern: ...\n",
    )
    .unwrap();
    write_distribution(
        &distributions,
        "alpha",
        "1.2.3",
        "alpha",
        "def connect(host: str) -> None: ...\n",
        true,
    );
    write_distribution(
        &distributions,
        "beta-stubs",
        "2.0.0",
        "beta",
        "def parse(value: str) -> int: ...\n",
        false,
    );

    let outcome = resolve_python_semantic_pack_dependencies(
        &environment(standard_library, distributions),
        workspace.project(),
        &DependencyPackLimits::default(),
        None,
    );

    assert!(outcome.complete, "{:#?}", outcome.diagnostics);
    assert_eq!(outcome.dependencies.len(), 3);
    assert_eq!(
        outcome
            .dependencies
            .iter()
            .map(|dependency| dependency.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "python:distribution:alpha:1.2.3",
            "python:distribution:beta-stubs:2.0.0",
            "python:stdlib:cpython:3.12.3",
        ]
    );
    assert!(outcome.dependencies.iter().all(|dependency| {
        dependency.evidence.language == "python"
            && dependency.evidence.ecosystem == "python"
            && dependency.evidence.artifact_sha256.is_none()
            && !dependency.artifacts.is_empty()
    }));
    assert_eq!(workspace.project().all_files().unwrap(), files_before);
}

#[test]
fn disabled_environment_has_no_implicit_interpreter_or_cache_discovery() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import re\n")
        .build();

    let outcome = resolve_python_semantic_pack_dependencies(
        &PythonAnalyzerConfig::default(),
        workspace.project(),
        &DependencyPackLimits::default(),
        None,
    );

    assert!(outcome.complete);
    assert!(outcome.dependencies.is_empty());
    assert!(outcome.diagnostics.is_empty());
}

#[test]
fn discovery_preserves_qualified_module_names_and_applies_global_stub_precedence() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import nested.widget\n")
        .build();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let bundled_stubs = environment_root.path().join("bundled-stubs");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(standard_library.join("nested")).unwrap();
    std::fs::create_dir_all(bundled_stubs.join("nested")).unwrap();
    std::fs::create_dir_all(&distributions).unwrap();
    std::fs::write(
        standard_library.join("nested").join("widget.py"),
        "def standard() -> None: ...\n",
    )
    .unwrap();
    std::fs::write(
        bundled_stubs.join("nested").join("widget.pyi"),
        "def stub() -> None: ...\n",
    )
    .unwrap();
    write_distribution(
        &distributions,
        "nested-widget",
        "1.0.0",
        "nested",
        "def distribution() -> None: ...\n",
        true,
    );

    let mut config = environment(standard_library, distributions);
    config
        .environment
        .as_mut()
        .unwrap()
        .bundled_stub_roots
        .push(bundled_stubs.clone());
    let outcome = resolve_python_semantic_pack_dependencies(
        &config,
        workspace.project(),
        &DependencyPackLimits::default(),
        None,
    );

    assert!(outcome.complete, "{:#?}", outcome.diagnostics);
    let artifacts = outcome
        .dependencies
        .iter()
        .flat_map(|dependency| dependency.artifacts.iter())
        .filter(|artifact| artifact.module.as_deref() == Some("nested.widget"))
        .collect::<Vec<_>>();
    assert_eq!(artifacts.len(), 1, "{outcome:#?}");
    assert_eq!(artifacts[0].kind, ExternalArtifactKind::PythonStub);
    assert_eq!(
        artifacts[0].path(),
        bundled_stubs
            .join("nested")
            .join("widget.pyi")
            .canonicalize()
            .unwrap()
    );
}

#[test]
fn discovery_uses_record_for_distribution_import_names_without_top_level_metadata() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import yaml\n")
        .build();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(&standard_library).unwrap();
    std::fs::create_dir_all(distributions.join("yaml")).unwrap();
    std::fs::write(
        distributions.join("yaml").join("__init__.py"),
        "def load(value: str) -> object: ...\n",
    )
    .unwrap();
    std::fs::write(distributions.join("yaml").join("py.typed"), "").unwrap();
    let metadata = distributions.join("PyYAML-6.0.0.dist-info");
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::write(metadata.join("METADATA"), "Name: PyYAML\nVersion: 6.0.0\n").unwrap();
    std::fs::write(
        metadata.join("RECORD"),
        "yaml/__init__.py,,\nyaml/py.typed,,\n",
    )
    .unwrap();

    let outcome = resolve_python_semantic_pack_dependencies(
        &environment(standard_library, distributions),
        workspace.project(),
        &DependencyPackLimits::default(),
        None,
    );

    assert!(outcome.complete, "{:#?}", outcome.diagnostics);
    let dependency = outcome
        .dependencies
        .iter()
        .find(|dependency| dependency.id == "python:distribution:pyyaml:6.0.0")
        .unwrap();
    assert_eq!(dependency.artifacts.len(), 1);
    assert_eq!(dependency.artifacts[0].module.as_deref(), Some("yaml"));
}

#[test]
fn cancelled_discovery_returns_no_dependencies() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import re\n")
        .build();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(&standard_library).unwrap();
    std::fs::create_dir_all(&distributions).unwrap();
    let cancellation = CancellationToken::default();
    cancellation.cancel();

    let outcome = resolve_python_semantic_pack_dependencies(
        &environment(standard_library, distributions),
        workspace.project(),
        &DependencyPackLimits::default(),
        Some(&cancellation),
    );

    assert!(outcome.cancelled);
    assert!(!outcome.complete);
    assert!(outcome.dependencies.is_empty());
    assert_eq!(outcome.diagnostics[0].code, "discovery.cancelled");
}

#[test]
fn static_stub_producer_preserves_classes_signatures_properties_and_aliases() {
    let environment = tempfile::tempdir().unwrap();
    let artifact = environment.path().join("widgets.pyi");
    std::fs::write(
        &artifact,
        r#"
from typing import Protocol, TypeAlias, overload

Alias: TypeAlias = str

class Reader(Protocol):
    value: int

    @property
    def name(self) -> str: ...

    @overload
    def read(self, count: int) -> bytes: ...
    @overload
    def read(self, count: None = ...) -> str: ...
"#,
    )
    .unwrap();

    let production = PythonArtifactPackProducer.produce_exact_artifact(
        &artifact_request(artifact, ExternalArtifactKind::PythonStub),
        &ArtifactProducerLimits::default(),
    );

    assert_eq!(
        production.completeness,
        brokk_bifrost::analyzer::semantic_model::Completeness::Complete
    );
    let pack = production.pack.unwrap();
    let AuthoredPayload::DeclarationFacts { types, members, .. } = &pack.shards[0].payload else {
        panic!("expected declaration facts");
    };
    assert!(types.iter().any(|fact| fact.name == "widgets.Reader"));
    assert!(types.iter().any(|fact| fact.name == "widgets.Alias"));
    assert_eq!(members.iter().filter(|fact| fact.name == "read").count(), 2);
    assert!(members.iter().any(|fact| fact.name == "name"
        && fact.member_kind == brokk_bifrost::analyzer::semantic_model::MemberKind::Property));
    assert!(
        members
            .iter()
            .filter(|fact| fact.name == "read")
            .all(|fact| fact
                .signature
                .as_ref()
                .is_some_and(|signature| signature.returns.is_some()))
    );
}

/// Two overloads of one name that share an arity are one declaration unless
/// the producer reads their annotations, and a stub tree deduplicates by
/// declaration identity. Typeshed spells fifteen overloads of `builtins.pow`
/// this way, so losing annotations publishes one of them and drops fourteen
/// while still reporting the pack complete (#1869).
#[test]
fn stub_producer_reads_annotations_names_and_pep695_aliases() {
    let environment = tempfile::tempdir().unwrap();
    let artifact = environment.path().join("shapes.pyi");
    std::fs::write(
        &artifact,
        r#"
import os
from typing import overload

type Pair[T] = tuple[T, T]

@overload
def widen(value: int) -> float: ...
@overload
def widen(value: str) -> bytes: ...
def locate(path: os.PathLike[str], *parts: str) -> list[str]: ...
"#,
    )
    .unwrap();

    let production = PythonArtifactPackProducer.produce_exact_artifact(
        &artifact_request(artifact, ExternalArtifactKind::PythonStub),
        &ArtifactProducerLimits::default(),
    );

    let pack = production.pack.unwrap();
    let AuthoredPayload::DeclarationFacts { types, members, .. } = &pack.shards[0].payload else {
        panic!("expected declaration facts");
    };
    let widen = members
        .iter()
        .filter(|fact| fact.name == "widen")
        .collect::<Vec<_>>();
    assert_eq!(widen.len(), 2, "{members:#?}");
    assert_ne!(widen[0].id, widen[1].id, "{widen:#?}");
    let locate = members
        .iter()
        .find(|fact| fact.name == "locate")
        .and_then(|fact| fact.signature.as_ref())
        .expect("locate publishes a signature");
    assert_eq!(
        locate
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("path"), Some("parts")],
        "{locate:#?}"
    );
    assert!(locate.parameters[1].variadic, "{locate:#?}");
    assert!(
        types
            .iter()
            .any(|fact| fact.name == "shapes.Pair" && fact.type_parameters == vec!["T".to_owned()]),
        "{types:#?}"
    );
}

/// Typeshed spells version- and platform-specific declarations inside
/// `sys.version_info` and `sys.platform` blocks. Publishing them with no
/// recorded condition makes a published name mean only "some supported
/// interpreter declares this", which is a false positive for a presence claim
/// (#1899).
#[test]
fn stub_producer_records_the_condition_of_each_guarded_declaration() {
    let environment = tempfile::tempdir().unwrap();
    let artifact = environment.path().join("guarded.pyi");
    std::fs::write(
        &artifact,
        r#"
import sys

def getcwd() -> str: ...

if sys.version_info >= (3, 14):
    def from_number(value: object) -> float: ...

if sys.platform == "win32":
    def startfile(path: str) -> None: ...
else:
    def fork() -> int: ...

if sys.version_info >= (3, 12) and sys.platform in ("linux", "darwin"):
    def sendfile(target: int) -> int: ...

if sys.version_info >= (3, 12):
    def swap(value: int) -> int: ...
else:
    def swap(value: int) -> int: ...

if _HAS_EXTENSION:
    def mystery() -> None: ...
"#,
    )
    .unwrap();

    let production = PythonArtifactPackProducer.produce_exact_artifact(
        &artifact_request(artifact, ExternalArtifactKind::PythonStub),
        &ArtifactProducerLimits::default(),
    );

    let pack = production.pack.unwrap();
    let AuthoredPayload::DeclarationFacts { members, .. } = &pack.shards[0].payload else {
        panic!("expected declaration facts");
    };
    let guard_of = |name: &str| {
        members
            .iter()
            .find(|fact| fact.name == name)
            .unwrap_or_else(|| panic!("`{name}` must stay in the pack: {members:#?}"))
            .guard
            .clone()
    };
    assert_eq!(guard_of("getcwd"), None);
    assert_eq!(
        guard_of("from_number"),
        Some(DeclarationGuard {
            min_toolchain_version: Some(GuardVersion::new(3, 14, 0)),
            ..DeclarationGuard::default()
        })
    );
    assert_eq!(
        guard_of("startfile"),
        Some(DeclarationGuard {
            required_targets: vec!["win32".to_owned()],
            ..DeclarationGuard::default()
        })
    );
    // The `else` branch is the negation of the branch above it.
    assert_eq!(
        guard_of("fork"),
        Some(DeclarationGuard {
            excluded_targets: vec!["win32".to_owned()],
            ..DeclarationGuard::default()
        })
    );
    // Both sides of an `and` stay necessary conditions.
    assert_eq!(
        guard_of("sendfile"),
        Some(DeclarationGuard {
            min_toolchain_version: Some(GuardVersion::new(3, 12, 0)),
            required_targets: vec!["linux".to_owned(), "darwin".to_owned()],
            ..DeclarationGuard::default()
        })
    );
    // Two branches declare one identity, so no branch's condition survives as
    // a necessary one and the record must not be droppable.
    assert_eq!(guard_of("swap"), Some(DeclarationGuard::uninterpreted()));
    // A condition this producer cannot read keeps the declaration and says so.
    assert_eq!(guard_of("mystery"), Some(DeclarationGuard::uninterpreted()));
}

/// The probes from #1869's typeshed slice: an activation that pins one
/// interpreter must stop resolving a declaration whose recorded condition that
/// interpreter cannot satisfy, and must keep everything else.
#[test]
fn activation_drops_only_declarations_the_pinned_interpreter_cannot_declare() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import os\n")
        .build();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(&standard_library).unwrap();
    std::fs::create_dir_all(&distributions).unwrap();
    std::fs::write(
        standard_library.join("builtins.pyi"),
        r#"
import sys

class float:
    def hex(self) -> str: ...
    if sys.version_info >= (3, 14):
        def from_number(self, value: object) -> float: ...
"#,
    )
    .unwrap();
    std::fs::write(
        standard_library.join("os.pyi"),
        r#"
import sys

def getcwd() -> str: ...

if sys.platform == "win32":
    def startfile(path: str) -> None: ...

if _HAS_EXTENSION:
    def mystery() -> None: ...
"#,
    )
    .unwrap();

    let activate = |version: &str, platform: &str| {
        let mut config = environment(standard_library.clone(), distributions.clone());
        let environment = config.environment.as_mut().unwrap();
        environment.version = version.to_owned();
        environment.platform = platform.to_owned();
        let limits = DependencyPackLimits::default();
        let discovery =
            resolve_python_semantic_pack_dependencies(&config, workspace.project(), &limits, None);
        assert!(discovery.complete, "{:#?}", discovery.diagnostics);
        let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
        let prepared = prepare_discovered_dependency_semantic_packs(
            &catalog,
            &PythonDependencyPackAdapter,
            discovery,
            &limits,
            None,
        );
        assert!(prepared.complete, "{:#?}", prepared.diagnostics);
        let request = prepared
            .compose_activation_request(SemanticModelActivationRequest {
                bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
                evidence: Vec::new(),
                controls: Vec::new(),
                limits: SemanticModelRuntimeLimits::default(),
            })
            .unwrap();
        let SemanticModelResolutionOutcome::Ready(active) =
            resolve_active_semantic_models(&catalog, &request, &CancellationToken::default())
        else {
            panic!("the exact Python stub pack must activate");
        };
        active
    };
    let resolves = |active: &ResolvedActiveSemanticModels, owner: &str, name: &str| {
        let matched = active.types_named(owner);
        assert_eq!(matched.records.len(), 1, "{matched:#?}");
        !active
            .members_named(&matched.records[0].record.id, name)
            .records
            .is_empty()
    };

    let pinned = activate("3.13.5", "linux");
    assert!(!resolves(&pinned, "builtins.float", "from_number"));
    assert!(resolves(&pinned, "builtins.float", "hex"));
    assert!(!resolves(&pinned, "os", "startfile"));
    assert!(resolves(&pinned, "os", "getcwd"));
    // A condition the producer could not read is not proof of anything.
    assert!(resolves(&pinned, "os", "mystery"));
    // Two dropped members, one per probe. Nothing else leaves the pack.
    assert_eq!(pinned.activation_report().guard_excluded_records, 2);

    let matching = activate("3.14.1", "win32");
    assert!(resolves(&matching, "builtins.float", "from_number"));
    assert!(resolves(&matching, "builtins.float", "hex"));
    assert!(resolves(&matching, "os", "startfile"));
    assert!(resolves(&matching, "os", "getcwd"));
    assert!(resolves(&matching, "os", "mystery"));
    assert_eq!(matching.activation_report().guard_excluded_records, 0);
}

#[test]
fn explicit_activation_publishes_python_facts_without_expanding_workspace_files() {
    let workspace = InlineTestProject::with_language(Language::Python)
        .file("app.py", "import re\nre.compile('a')\n")
        .build();
    let environment_root = tempfile::tempdir().unwrap();
    let standard_library = environment_root.path().join("stdlib");
    let distributions = environment_root.path().join("site-packages");
    std::fs::create_dir_all(&standard_library).unwrap();
    std::fs::create_dir_all(&distributions).unwrap();
    std::fs::write(
        standard_library.join("re.pyi"),
        "def compile(pattern: str) -> Pattern: ...\n",
    )
    .unwrap();
    std::fs::write(standard_library.join("pathlib.pyi"), "class Path: ...\n").unwrap();
    let config = AnalyzerConfig {
        python: environment(standard_library, distributions),
        ..Default::default()
    };
    let analyzer = workspace.workspace_analyzer(config.clone());
    let files_before = analyzer.analyzer().project().all_files().unwrap();
    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    let cancellation = CancellationToken::default();
    let activation = SemanticModelActivationRequest {
        bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
        evidence: Vec::new(),
        controls: Vec::new(),
        limits: SemanticModelRuntimeLimits::default(),
    };

    let outcome = analyzer.activate_python_environment_packs(
        &config,
        PythonSemanticModelWorkspaceContext {
            catalog: &catalog,
            persistence: None,
            activation: &activation,
            limits: DependencyPackLimits::default(),
            cancellation: &cancellation,
        },
    );

    let preparation = outcome.preparation.as_ref().unwrap();
    assert!(preparation.complete, "{preparation:#?}");
    assert_eq!(preparation.profile.artifacts_read, 2);
    assert!(outcome.runtime.is_some());
    assert!(analyzer.analyzer().semantic_model_overlay().is_some());
    assert_eq!(
        analyzer.analyzer().project().all_files().unwrap(),
        files_before
    );
}
