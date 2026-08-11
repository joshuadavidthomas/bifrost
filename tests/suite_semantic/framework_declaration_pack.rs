//! Acceptance for the shipped framework declaration packs (#1935, the
//! type-resolution prerequisite).
//!
//! The converter reads the framework declaration candidates, flattens each
//! type's nested members into the pack's flat member list, keeps the servlet
//! `extends` hierarchy, and assembles one `declaration_facts` pack per artifact:
//! `bifrost.jdk-framework-decls` pinned on the `jdk` toolchain and
//! `bifrost.javax.servlet-api-framework-decls` staged unpinned. This suite proves:
//!
//! 1. the conversion is deterministic (two runs are byte-identical);
//! 2. the checked-in packs under `semantic-packs/framework-decls/` are exactly
//!    the generator's output, so a drift is caught here rather than at release;
//! 3. every generated pack compiles through the production compiler;
//! 4. a servlet and a JDBC type resolve at the `ExternalIndexed` boundary once
//!    the packs are activated, exactly as #1900's external-surface proof does;
//! 5. the kept servlet hierarchy carries the shared getters: `getParameter` is
//!    declared on `ServletRequest` and reached from `HttpServletRequest` through
//!    the compiled `extends` closure, which is how member-spelling resolution
//!    answers an inherited external member.

use std::path::PathBuf;
use std::sync::Arc;

use brokk_bifrost::CancellationToken;
use brokk_bifrost::analyzer::WorkspaceAnalyzer;
use brokk_bifrost::analyzer::semantic_model::*;
use brokk_bifrost::analyzer::structural::BoundaryStatus;
use brokk_bifrost::semantic_packs::summary_foundry::framework_pack::{
    FRAMEWORK_AUDIT_FILE_NAME, convert_framework_candidates, serialize_audit,
};
use semver::Version;

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use crate::jvm_diagnostic_proof::{offline_config, report_for, resolved_at};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn candidates_dir() -> PathBuf {
    repo_root().join(".agents/foundry/candidates/framework-decls")
}

fn shipped_dir() -> PathBuf {
    repo_root().join("semantic-packs/framework-decls")
}

/// Register the two generated framework packs onto `analyzer` and activate them
/// with evidence for both the JDK toolchain and the servlet Maven coordinate.
fn activate_framework_packs(analyzer: &WorkspaceAnalyzer) {
    let conversion = convert_framework_candidates(&candidates_dir()).expect("conversion");
    let catalog = SemanticPackCatalog::open_ephemeral(CatalogOptions::default()).unwrap();
    // The authored framework packs declare `review_required`, so each one needs
    // an explicit enable control to activate, exactly as a host would grant after
    // review. Without it the pack resolves `ReviewRequired` and publishes nothing.
    let mut controls = Vec::new();
    for pack in &conversion.packs {
        let compiled = compile_source(
            SourceFormat::Json,
            pack.source_json.as_bytes(),
            &CompilerOptions::default(),
        )
        .unwrap_or_else(|diagnostics| {
            panic!("{} failed to compile: {diagnostics:#?}", pack.pack_id)
        });
        catalog
            .register_session_pack(
                &compiled,
                &SessionPackSource {
                    kind: SessionPackSourceKind::Embedded,
                    source_id: format!("framework-fixture:{}", pack.pack_id),
                },
            )
            .unwrap();
        controls.push(SemanticModelActivationControl {
            scope: SemanticModelControlScope::User,
            action: SemanticModelControlAction::Enable,
            selector: SemanticModelPackSelector {
                pack_id: pack.pack_id.clone(),
                version: None,
                manifest_digest: None,
            },
        });
    }
    let request = SemanticModelActivationRequest {
        bifrost_version: Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
        evidence: vec![
            SemanticModelActivationEvidence {
                language: "java".to_owned(),
                ecosystem: "jdk".to_owned(),
                package: None,
                module: None,
                toolchain: Some(CatalogCoordinate {
                    name: "jdk".to_owned(),
                    version: Some(Version::parse("21.0.2").unwrap()),
                }),
                target: Some("jvm".to_owned()),
                configuration: None,
                artifact_sha256: None,
            },
            SemanticModelActivationEvidence {
                language: "java".to_owned(),
                ecosystem: "maven".to_owned(),
                package: Some(CatalogCoordinate {
                    name: "javax.servlet:javax.servlet-api".to_owned(),
                    version: Some(Version::parse("4.0.1").unwrap()),
                }),
                module: None,
                toolchain: None,
                target: Some("jvm".to_owned()),
                configuration: None,
                artifact_sha256: None,
            },
        ],
        controls,
        limits: SemanticModelRuntimeLimits::default(),
    };
    let SemanticModelRuntimeOutcome::Ready { .. } = acquire_active_semantic_models(
        analyzer.analyzer(),
        &catalog,
        None,
        &request,
        &CancellationToken::default(),
    ) else {
        panic!("the framework declaration packs must activate");
    };
    assert!(
        analyzer.analyzer().semantic_model_overlay().is_some(),
        "activation must publish an overlay"
    );
}

fn framework_workspace(files: &[(&str, &str)]) -> (BuiltInlineTestProject, WorkspaceAnalyzer) {
    let mut project = InlineTestProject::new();
    for (path, contents) in files {
        project = project.file(*path, *contents);
    }
    let built = project.build();
    let analyzer = built.workspace_analyzer(offline_config());
    activate_framework_packs(&analyzer);
    (built, analyzer)
}

#[test]
fn the_conversion_is_deterministic() {
    let first = convert_framework_candidates(&candidates_dir()).expect("first conversion");
    let second = convert_framework_candidates(&candidates_dir()).expect("second conversion");
    assert_eq!(
        first, second,
        "two conversions over the same candidates must be identical"
    );
}

#[test]
fn every_generated_pack_compiles_through_the_production_compiler() {
    let conversion = convert_framework_candidates(&candidates_dir()).expect("conversion");
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
    let conversion = convert_framework_candidates(&candidates_dir()).expect("conversion");
    let root = shipped_dir();
    for pack in &conversion.packs {
        let path = root.join(&pack.relative_path);
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read checked-in pack {}: {error}", path.display()));
        assert_eq!(
            on_disk,
            pack.source_json,
            "checked-in {} drifted from the generator; regenerate with framework-decl-pack",
            path.display()
        );
    }
    let audit_on_disk = std::fs::read_to_string(root.join(FRAMEWORK_AUDIT_FILE_NAME))
        .expect("read checked-in rejects.json");
    assert_eq!(
        audit_on_disk,
        serialize_audit(&conversion.audit),
        "checked-in rejects.json drifted from the generator"
    );
}

/// A servlet type and a JDBC type both resolve at the external boundary once the
/// framework packs are activated, even with the host JDK unavailable: the pack
/// is the only surface that spells them.
#[test]
fn servlet_and_jdbc_types_resolve_external_indexed_from_the_activated_packs() {
    let files = [
        (
            "src/app/UsesServlet.java",
            "package app; import javax.servlet.http.HttpServletRequest; \
             class UsesServlet { HttpServletRequest request; }",
        ),
        (
            "src/app/UsesJdbc.java",
            "package app; import java.sql.PreparedStatement; \
             class UsesJdbc { PreparedStatement statement; }",
        ),
    ];
    let (built, analyzer) = framework_workspace(&files);
    for (path, _) in files {
        let report = report_for(&analyzer, &built, path);
        assert!(
            report.diagnostics().is_empty(),
            "{path} must not report a framework type the pack declares: {:#?}",
            report.diagnostics()
        );
        assert!(
            resolved_at(&report, BoundaryStatus::ExternalIndexed),
            "{path} must resolve its framework type at the external boundary: {:#?}",
            report.outcomes()
        );
    }
}

/// The kept servlet hierarchy is faithful: `HttpServletRequest` reaches its
/// supertype `ServletRequest` through the compiled `extends` edge, and the
/// shared getter `getParameter` is declared on `ServletRequest`, so an inherited
/// member spelling on `HttpServletRequest` resolves through that closure rather
/// than needing every getter duplicated onto the subtype.
#[test]
fn the_kept_hierarchy_carries_inherited_servlet_getters() {
    let (_built, analyzer) =
        framework_workspace(&[("src/app/App.java", "package app; class App {}")]);
    let overlay = analyzer
        .analyzer()
        .semantic_model_overlay()
        .expect("active framework overlay");

    // The subtype resolves as one unique external declaration.
    let http = overlay.symbols_named("javax.servlet.http.HttpServletRequest");
    assert_eq!(http.disposition, SemanticModelOverlayDisposition::Unique);
    let http_symbol = http.records[0];

    // The compiled `extends` fact places ServletRequest among its ancestors.
    let ancestors = overlay.ancestors_of(http_symbol);
    assert!(
        ancestors
            .records
            .iter()
            .any(|symbol| symbol.qualified_name == "javax.servlet.ServletRequest"),
        "HttpServletRequest must inherit from ServletRequest: {:#?}",
        ancestors
            .records
            .iter()
            .map(|symbol| symbol.qualified_name.as_str())
            .collect::<Vec<_>>()
    );

    // The shared getter is declared on the supertype, not duplicated onto the
    // subtype, and is reached through the ancestor: this is the closure
    // `owner_surface` builds for member-spelling resolution.
    let servlet_request = overlay.symbols_named("javax.servlet.ServletRequest");
    assert_eq!(
        servlet_request.disposition,
        SemanticModelOverlayDisposition::Unique
    );
    let servlet_request_id = &servlet_request.records[0].id;
    let getter = overlay
        .members_of(servlet_request_id)
        .records
        .iter()
        .any(|member| member.qualified_name == "javax.servlet.ServletRequest.getParameter");
    assert!(getter, "getParameter must be declared on ServletRequest");
    // It is not also duplicated onto the subtype: keeping the hierarchy is what
    // makes the inherited member reachable.
    let duplicated = overlay
        .members_of(&http_symbol.id)
        .records
        .iter()
        .any(|member| member.name == "getParameter");
    assert!(
        !duplicated,
        "getParameter must be inherited, not flattened onto HttpServletRequest"
    );
}

/// Resolving an external framework type must not mint a workspace declaration:
/// it stays a dependency surface, so navigation and usage clients are never
/// pointed at a file that does not exist.
#[test]
fn resolving_a_framework_type_creates_no_workspace_declaration() {
    let (_built, analyzer) = framework_workspace(&[(
        "src/app/UsesServlet.java",
        "package app; import javax.servlet.http.HttpServletRequest; \
         class UsesServlet { HttpServletRequest request; }",
    )]);
    let workspace: Vec<Arc<str>> = analyzer
        .analyzer()
        .get_all_declarations()
        .iter()
        .map(|unit| Arc::from(unit.fq_name()))
        .collect();
    assert!(
        !workspace
            .iter()
            .any(|fqn| &**fqn == "javax.servlet.http.HttpServletRequest"),
        "the framework type is a dependency declaration and must not appear among \
         workspace declarations: {workspace:#?}"
    );
}
