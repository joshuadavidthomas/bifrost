//! Issue #2131: Rust inverse lookup retains exact scoped type references in
//! aliases and associated-type positions.

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupRequest, DefinitionLookupStatus, resolve_definition_batch_with_source,
};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use std::sync::Arc;

fn assert_exact_inverse(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    anchor: &str,
    token: &str,
    expected_path: &str,
    expected_fqn: &str,
) {
    let anchor_start = source.find(anchor).expect("anchor");
    let start = anchor_start
        + source[anchor_start..]
            .find(token)
            .expect("token after anchor");
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
    assert_eq!(target.source(), &project.file(expected_path));
    assert_eq!(target.fq_name(), expected_fqn);

    let query = UsageFinder::new()
        .with_file_filter(|candidate| candidate == &file)
        .with_authoritative_scope(true)
        .query(workspace.analyzer(), &[target], 1000, 1000);
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
fn scoped_alias_and_associated_type_paths_round_trip_exactly() {
    let lib = r#"pub mod frame_clause;
pub mod nested;
pub mod sql_dialect;
mod sibling;

pub type FrameStartWith<S, T> = self::frame_clause::StartFrame<S, T>;
"#;
    let nested = r#"use crate::sql_dialect;

pub mod for_load {
    pub struct Posts;
}

pub trait Backend {
    type ArrayComparison;
}

pub struct Sqlite;

impl Backend for Sqlite {
    type ArrayComparison = sql_dialect::array_comparison::AnsiSqlArrayComparison;
}

pub type Loaded = Vec<for_load::Posts>;
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue-2131\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", lib)
        .file(
            "src/frame_clause.rs",
            "pub struct StartFrame<S, T>(pub S, pub T);\n",
        )
        .file("src/nested.rs", nested)
        .file(
            "src/sql_dialect.rs",
            "pub mod array_comparison { pub struct AnsiSqlArrayComparison; }\n",
        )
        .file(
            "src/sibling.rs",
            "pub struct StartFrame;\npub struct AnsiSqlArrayComparison;\npub struct Posts;\n",
        )
        .build();

    assert_exact_inverse(
        &project,
        "src/lib.rs",
        lib,
        "self::frame_clause::StartFrame",
        "StartFrame",
        "src/frame_clause.rs",
        "issue_2131.frame_clause.StartFrame",
    );
    assert_exact_inverse(
        &project,
        "src/nested.rs",
        nested,
        "sql_dialect::array_comparison::AnsiSqlArrayComparison",
        "AnsiSqlArrayComparison",
        "src/sql_dialect.rs",
        "issue_2131.sql_dialect.array_comparison.AnsiSqlArrayComparison",
    );
    assert_exact_inverse(
        &project,
        "src/nested.rs",
        nested,
        "for_load::Posts",
        "Posts",
        "src/nested.rs",
        "issue_2131.nested.for_load.Posts",
    );
}
