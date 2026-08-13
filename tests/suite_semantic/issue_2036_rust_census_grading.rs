//! Issue #2036: Rust census grading distinguishes non-reference roles and
//! namespace/owner collisions from definition lookup gaps.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ProbeSeed, ReferenceClassification, ReferenceDifferentialConfig, ReferenceDifferentialReport,
    ReferenceDifferentialSite, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

fn site_at(report: &ReferenceDifferentialReport, start_byte: usize) -> &ReferenceDifferentialSite {
    report
        .sites
        .iter()
        .find(|site| site.start_byte == start_byte)
        .unwrap_or_else(|| panic!("census did not probe byte {start_byte}: {report:#?}"))
}

fn no_site_at(report: &ReferenceDifferentialReport, start_byte: usize) {
    assert!(
        report
            .sites
            .iter()
            .all(|site| site.start_byte != start_byte),
        "declaration byte {start_byte} entered the census report: {report:#?}"
    );
}

#[test]
fn rust_census_grades_only_namespace_and_owner_compatible_definition_evidence() {
    let source = r#"
trait Surface {
    type Ok;
    const FLAG: usize;
    fn declared(&self);
}

struct Host;

impl Surface for Host {
    type Ok = usize;
    const FLAG: usize = 1;
    fn declared(&self) {}
}

struct WrongOwner;

impl WrongOwner {
    const ITEM: usize = 1;
}

enum Kind {
    Solidus,
    Entry { key: usize },
}

use Kind::*;

fn use_sites<'a>(local: &'a str, kind: Kind) -> Result<&'a str, ()> {
    let bound = local.len();
    let key = bound;
    let _ = key;
    let _ = bound;
    let _ = local;
    let _ = Ok(local);
    let _ = Host::ITEM;
    match kind {
        Solidus => Ok(local),
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file("lib.rs", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            corpus_language: "rust".to_string(),
            max_files: 10,
            max_sites: 1_000,
            max_candidates_per_file: 1_000,
            max_source_bytes: 10_000,
            max_targets: 1_000,
            max_usage_files: 10,
            max_usages: 1_000,
            probe_seed: ProbeSeed::Census,
            ..ReferenceDifferentialConfig::default()
        },
    )
    .expect("run Rust census differential");

    for declaration in [
        "fn declared(&self) {}",
        "const FLAG: usize = 1",
        "type Ok = usize",
        "let bound = local.len();",
        "let key = bound;",
    ] {
        let start = source.find(declaration).expect("declaration shape")
            + declaration
                .find(['d', 'F', 'O'])
                .expect("declaration identifier");
        no_site_at(&report, start);
    }

    let lifetime = site_at(
        &report,
        source.find("<'a>").expect("lifetime declaration") + 2,
    );
    assert_eq!(
        lifetime
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.kind.as_str()),
        Some("local_variable_reference"),
        "lifetime: {lifetime:#?}"
    );
    assert_eq!(lifetime.tier, None, "lifetime: {lifetime:#?}");
    assert_eq!(
        lifetime.classification,
        ReferenceClassification::Inconclusive
    );

    let local = site_at(
        &report,
        source.find("_ = local").expect("local use") + "_ = ".len(),
    );
    assert_eq!(local.forward_status, "resolved", "local: {local:#?}");
    assert_eq!(local.tier, None, "local: {local:#?}");
    assert_eq!(local.classification, ReferenceClassification::Inconclusive);

    let prelude_ok = site_at(
        &report,
        source.find("Ok(local)").expect("prelude Ok constructor"),
    );
    assert_eq!(prelude_ok.tier, Some(3), "prelude Ok: {prelude_ok:#?}");
    assert_eq!(
        prelude_ok.classification,
        ReferenceClassification::Inconclusive,
        "a value use cannot borrow evidence from an associated type alias"
    );

    let wrong_owner = site_at(
        &report,
        source.find("Host::ITEM").expect("unresolved scoped owner") + "Host::".len(),
    );
    assert_eq!(
        wrong_owner.forward_status, "unresolvable_import_boundary",
        "wrong owner: {wrong_owner:#?}"
    );
    assert_eq!(wrong_owner.tier, None, "wrong owner: {wrong_owner:#?}");
    assert_eq!(
        wrong_owner.classification,
        ReferenceClassification::Inconclusive,
        "a scoped terminal cannot borrow evidence from another owner"
    );

    let wildcard_variant = site_at(
        &report,
        source.rfind("Solidus =>").expect("wildcard enum variant"),
    );
    if wildcard_variant.forward_status != "resolved" {
        assert!(
            wildcard_variant.tier.is_some(),
            "#2032's unresolved wildcard variant must remain gradeable: {wildcard_variant:#?}"
        );
        assert_eq!(
            wildcard_variant.classification,
            ReferenceClassification::Missing,
            "#2032's unresolved wildcard variant must remain actionable"
        );
    }
}
