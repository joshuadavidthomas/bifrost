//! Issue #2128: Rust inverse usage lookup retains exact type identities through
//! grouped re-exports and indexed type aliases.

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
fn grouped_reexported_types_and_aliases_round_trip_through_inverse_lookup() {
    let lib = r#"mod consumer;
mod config;
mod error;
mod types;

mod decoy {
    pub struct Thing;
    pub type Alias = Thing;
}
"#;
    let config = "pub trait Config {}\n";
    let error = "pub enum OpenAIError { Failed }\n";
    let types = r#"mod declaration;

pub use declaration::*;
"#;
    let declaration = r#"mod r#struct;

pub use r#struct::*;
pub type Alias = Thing;
"#;
    let inner = "pub struct Thing;\n";
    let consumer = r#"use crate::{config::Config, error::OpenAIError, types::{self, Alias, Thing}};

trait Marker {}
impl Marker for Thing {}

fn borrowed(_: &Thing) {}
fn generic(_: Vec<Thing>) {}
fn alias(_: Option<Alias>) {}
fn direct_result(_: Result<Thing, OpenAIError>) {}
async fn direct_generic<P: AsRef<std::path::Path>>(_: P) -> Result<std::path::PathBuf, OpenAIError> {
    unreachable!()
}
fn bounded<C: Config>(_: C) {}
fn build() -> Thing { Thing }
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2128\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", lib)
        .file("src/config.rs", config)
        .file("src/error.rs", error)
        .file("src/types.rs", types)
        .file("src/types/declaration.rs", declaration)
        .file("src/types/declaration/struct.rs", inner)
        .file("src/consumer.rs", consumer)
        .build();

    for anchor in ["impl Marker", "fn borrowed", "fn generic", "fn build"] {
        assert_exact_inverse(
            &project,
            "src/consumer.rs",
            consumer,
            anchor,
            "Thing",
            "src/types/declaration/struct.rs",
        );
    }
    assert_exact_inverse(
        &project,
        "src/consumer.rs",
        consumer,
        "fn alias",
        "Alias",
        "src/types/declaration.rs",
    );
    assert_exact_inverse(
        &project,
        "src/consumer.rs",
        consumer,
        "fn direct_result",
        "OpenAIError",
        "src/error.rs",
    );
    assert_exact_inverse(
        &project,
        "src/consumer.rs",
        consumer,
        "async fn direct_generic",
        "OpenAIError",
        "src/error.rs",
    );
    assert_exact_inverse(
        &project,
        "src/consumer.rs",
        consumer,
        "fn bounded",
        "Config",
        "src/config.rs",
    );
}
