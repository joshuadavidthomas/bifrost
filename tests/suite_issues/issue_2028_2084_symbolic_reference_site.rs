//! Issues #2028 and #2084: parsed leaves, not an ASCII byte scan, define exact
//! reference-token boundaries.

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupOutcome, DefinitionLookupRequest, DefinitionLookupStatus,
    resolve_definition_batch_with_source,
};
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use std::sync::Arc;

fn definition_range(
    project: &BuiltInlineTestProject,
    workspace: &WorkspaceAnalyzer,
    path: &str,
    source: &str,
    start_byte: usize,
    end_byte: usize,
) -> DefinitionLookupOutcome {
    let file = project.file(path);
    let request = DefinitionLookupRequest {
        file: file.clone(),
        line: None,
        column: None,
        start_byte: Some(start_byte),
        end_byte: Some(end_byte),
    };
    resolve_definition_batch_with_source(
        workspace.analyzer(),
        vec![request],
        file,
        Arc::from(source),
    )
    .pop()
    .expect("one outcome")
}

fn nth_start(source: &str, needle: &str, occurrence: usize) -> usize {
    source
        .match_indices(needle)
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {needle:?}"))
        .0
}

#[test]
fn php_unicode_identifier_leaves_are_complete_reference_tokens() {
    let source = r#"<?php
namespace App;

final class Matrix
{
    private float $x₀ = 1.0;
    private float $d₁ = 2.0;
    private float $Aᵀ = 3.0;
    private float $A⁻¹ = 4.0;

    public function total(): float
    {
        return $this->x₀ + $this->d₁ + $this->Aᵀ + $this->A⁻¹;
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Matrix.php", source)
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());

    for name in ["x₀", "d₁", "Aᵀ", "A⁻¹"] {
        let start = nth_start(source, name, 1);
        let result = definition_range(
            &project,
            &workspace,
            "src/Matrix.php",
            source,
            start,
            start + name.len(),
        );
        assert_eq!(
            result.status,
            DefinitionLookupStatus::Resolved,
            "{name}: {result:#?}"
        );
        assert_eq!(
            result.definitions[0].fq_name(),
            format!("App.Matrix.{name}")
        );
    }

    let start = nth_start(source, "x₀", 1);
    let end = nth_start(source, "d₁", 1) + "d₁".len();
    let crossing = definition_range(&project, &workspace, "src/Matrix.php", source, start, end);
    assert_eq!(
        crossing.status,
        DefinitionLookupStatus::InvalidLocation,
        "{crossing:#?}"
    );
}

#[test]
fn scala_symbolic_identifier_leaves_resolve_exact_census_ranges() {
    let source = r#"package demo

object Reusability {
  def by_==[A]: Int = 1
  def byRefOr_==[A]: Int = 2
}

object EitherT {
  final class Switching_\/[A]
  def make[A]: Switching_\/[A] = new Switching_\/[A]
}

class Use {
  val equality = Reusability.by_==[Int]
  val reference = Reusability.byRefOr_==[Int]
  val switching = EitherT.make[Int]
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("src/main/scala/demo/Names.scala", source)
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let path = "src/main/scala/demo/Names.scala";

    for name in ["by_==", "byRefOr_=="] {
        let start = nth_start(source, name, 1);
        let result = definition_range(
            &project,
            &workspace,
            path,
            source,
            start,
            start + name.len(),
        );
        assert_eq!(
            result.status,
            DefinitionLookupStatus::Resolved,
            "{name}: {result:#?}"
        );
        assert!(
            result.definitions[0].fq_name().ends_with(name),
            "{name}: {result:#?}"
        );
    }

    let switching = "Switching_\\/";
    let start = nth_start(source, switching, 1);
    let truncated_end = start + "Switching_\\".len();
    let result = definition_range(&project, &workspace, path, source, start, truncated_end);
    assert_eq!(
        result.status,
        DefinitionLookupStatus::Resolved,
        "{result:#?}"
    );
    assert!(
        result.definitions[0].fq_name().ends_with("Switching_\\"),
        "{result:#?}"
    );

    let start = nth_start(source, "by_==", 1);
    let end = source[start..].find('[').expect("type arguments") + start + 1;
    let crossing = definition_range(&project, &workspace, path, source, start, end);
    assert_eq!(
        crossing.status,
        DefinitionLookupStatus::InvalidLocation,
        "{crossing:#?}"
    );
}
