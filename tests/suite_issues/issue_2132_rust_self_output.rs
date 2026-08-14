//! Issue #2132: `Self::Output` inside an impl resolves to that impl's exact
//! associated-type item rather than an unrelated same-name trait member.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupRequest, DefinitionLookupStatus, resolve_definition_batch_with_source,
};
use brokk_bifrost::usages::UsageFinder;
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use std::sync::Arc;

#[test]
fn self_output_round_trips_to_the_enclosing_generic_impl_item() {
    let source = r#"pub trait InternalJoinDsl {
    type Output;
}

pub trait BoxedDsl {
    type Output;
}

pub trait ThenOrderDsl {
    type Output;
}

pub struct Unrelated;

impl InternalJoinDsl for Unrelated {
    type Output = u8;
}

pub trait LimitDsl {
    type Output;
    fn limit(self) -> Self::Output;
}

pub struct Alias<S>(S);

impl<S> BoxedDsl for Alias<S> {
    type Output = Vec<S>;
}

impl<S> ThenOrderDsl for Alias<S> {
    type Output = Option<S>;
}

impl<S> LimitDsl for Alias<S> {
    type Output = S;

    fn limit(self) -> Self::Output {
        self.0
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue-2132\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", source)
        .build();
    let reference = source
        .find("Self::Output {")
        .expect("impl method associated type")
        + "Self::".len();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let file = project.file("src/lib.rs");
    let outcome = resolve_definition_batch_with_source(
        workspace.analyzer(),
        vec![DefinitionLookupRequest {
            file: file.clone(),
            line: None,
            column: None,
            start_byte: Some(reference),
            end_byte: Some(reference + "Output".len()),
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
    assert_eq!(target.source(), &file);
    assert_eq!(target.fq_name(), "issue_2132.Alias.Output");
    let impl_declaration = source
        .rfind("type Output = S;")
        .expect("impl associated type");
    assert!(
        workspace
            .analyzer()
            .ranges(&target)
            .iter()
            .any(|range| range.start_byte <= impl_declaration && impl_declaration < range.end_byte),
        "target ranges: {:#?}",
        workspace.analyzer().ranges(&target)
    );

    let query = UsageFinder::new()
        .with_file_filter(|candidate| candidate == &file)
        .with_authoritative_scope(true)
        .query(workspace.analyzer(), &[target], 1000, 1000);
    let hits = query.result.all_hits_including_imports();
    assert!(
        hits.iter().any(|hit| {
            hit.file == file
                && hit.start_offset == reference
                && hit.end_offset == reference + "Output".len()
        }),
        "inverse hits: {hits:#?}"
    );
}
