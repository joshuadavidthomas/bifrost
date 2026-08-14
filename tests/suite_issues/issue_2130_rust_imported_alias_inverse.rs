//! Issue #2130: Rust inverse lookup retains grouped import names and imported
//! type aliases across Cargo package boundaries.

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupRequest, DefinitionLookupStatus, resolve_definition_batch_with_source,
};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use std::sync::Arc;

fn token_after(source: &str, anchor: &str, token: &str) -> usize {
    let anchor_start = source.find(anchor).expect("anchor");
    anchor_start
        + source[anchor_start..]
            .find(token)
            .expect("token after anchor")
}

fn assert_exact_inverse(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    anchor: &str,
    token: &str,
    expected_target_path: &str,
    expected_fqn: &str,
) {
    let start = token_after(source, anchor, token);
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let file = project.file(path);
    let outcome = resolve_definition_batch_with_source(
        workspace.analyzer(),
        vec![DefinitionLookupRequest {
            file: file.clone(),
            line: None,
            column: None,
            start_byte: Some(start),
            end_byte: Some(start + token.len()),
        }],
        file.clone(),
        Arc::from(source),
    )
    .remove(0);
    assert_eq!(
        outcome.status,
        DefinitionLookupStatus::Resolved,
        "forward outcome: {outcome:#?}"
    );
    assert_eq!(
        outcome.definitions.len(),
        1,
        "forward outcome: {outcome:#?}"
    );
    let target = outcome.definitions.into_iter().next().expect("definition");
    assert_eq!(target.source(), &project.file(expected_target_path));
    assert_eq!(target.fq_name(), expected_fqn);

    let analyzer = workspace.analyzer();
    let query = UsageFinder::new()
        .with_file_filter(|candidate| candidate == &file)
        .with_authoritative_scope(true)
        .query(analyzer, &[target], 1000, 1000);
    assert!(
        query.candidate_files.contains(&file),
        "candidate files: {:#?}",
        query.candidate_files
    );
    let hits = query.result.all_hits_including_imports();
    assert!(
        hits.iter().any(|hit| {
            hit.file == file && hit.start_offset == start && hit.end_offset == start + token.len()
        }),
        "inverse hits for {path}:{start}: {hits:#?}"
    );
}

#[test]
fn nested_grouped_import_leaf_round_trips_to_exact_type() {
    let consumer = r#"use crate::*;
use ast_elements::{Decoy, params::{Other, Target}};

fn consume(_: Target) {}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2130_grouped\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file(
            "src/lib.rs",
            "mod consumer;\npub mod types;\npub use types::*;\nmod sibling;\n",
        )
        .file("src/consumer.rs", consumer)
        .file(
            "src/types.rs",
            "pub mod ast_elements { pub struct Decoy; pub mod params { pub struct Other; pub struct Target; } }\n",
        )
        .file("src/sibling.rs", "pub struct Target;\n")
        .build();

    assert_exact_inverse(
        &project,
        "src/consumer.rs",
        consumer,
        "params::{Other, Target}",
        "Target",
        "src/types.rs",
        "issue_2130_grouped.types.ast_elements.params.Target",
    );
}

#[test]
fn cross_package_and_cfg_alternative_aliases_round_trip() {
    let consumer = r#"use provider_types::DirectAlias;
use provider_types::dsl::*;

pub fn direct(_: Option<DirectAlias>) {}
pub fn filtered(_: Filter<(), ()>) {}
"#;
    let bench = r#"use super::Bencher;

pub fn bench(_: &mut Bencher) {}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file("Cargo.toml", "[workspace]\nmembers = [\"provider\", \"consumer\"]\nresolver = \"2\"\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2021\"\n")
        .file(
            "provider/Cargo.toml",
            "[package]\nname = \"provider-types\"\nversion.workspace = true\nedition.workspace = true\n",
        )
        .file(
            "provider/src/lib.rs",
            r#"pub type DirectAlias = external::Uuid;
pub mod helper_types {
    pub type Filter<Source, Predicate> = external::Output<Source, Predicate>;
}
pub mod dsl {
    pub use crate::helper_types::*;
}
"#,
        )
        .file(
            "consumer/Cargo.toml",
            "[package]\nname = \"consumer\"\nversion.workspace = true\nedition.workspace = true\npublish = false\n[features]\ncriterion = []\n[dependencies]\nprovider-types = { path = \"../provider\" }\n[[bench]]\nname = \"suite\"\npath = \"benches/lib.rs\"\nharness = false\n",
        )
        .file(
            "consumer/src/lib.rs",
            format!(
                "{consumer}\nmod bench;\n#[cfg(feature = \"criterion\")]\npub type Bencher<'a> = criterion::Bencher<'a>;\n#[cfg(not(feature = \"criterion\"))]\npub type Bencher<'a> = test::Bencher<'a>;\n"
            ),
        )
        .file("consumer/src/bench.rs", bench)
        .file(
            "consumer/benches/lib.rs",
            "mod case;\n#[cfg(feature = \"criterion\")]\ntype Bencher<'a> = criterion::Bencher<'a>;\n#[cfg(not(feature = \"criterion\"))]\ntype Bencher<'a> = test::Bencher<'a>;\n",
        )
        .file("consumer/benches/case.rs", bench)
        .file("consumer/src/decoy.rs", "pub struct Alias;\npub struct Bencher;\n")
        .build();

    assert_exact_inverse(
        &project,
        "consumer/src/lib.rs",
        consumer,
        "Option<DirectAlias>",
        "DirectAlias",
        "provider/src/lib.rs",
        "provider_types.DirectAlias",
    );
    assert_exact_inverse(
        &project,
        "consumer/src/lib.rs",
        consumer,
        "Filter<(), ()>",
        "Filter",
        "provider/src/lib.rs",
        "provider_types.helper_types.Filter",
    );
    assert_exact_inverse(
        &project,
        "consumer/src/bench.rs",
        bench,
        "&mut Bencher",
        "Bencher",
        "consumer/src/lib.rs",
        "consumer.Bencher",
    );
    assert_exact_inverse(
        &project,
        "consumer/benches/case.rs",
        bench,
        "&mut Bencher",
        "Bencher",
        "consumer/benches/lib.rs",
        "consumer.benches.Bencher",
    );
}
