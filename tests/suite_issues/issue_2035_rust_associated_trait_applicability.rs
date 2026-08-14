//! Issue #2035: scoped associated trait calls use argument type applicability
//! instead of returning every same-name implementation for the receiver.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::analyzer::usages::get_definition::{
    DefinitionLookupOutcome, DefinitionLookupRequest, DefinitionLookupStatus,
    resolve_definition_batch_with_source,
};
use brokk_bifrost::usages::{RustExportUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{AnalyzerConfig, Language, WorkspaceAnalyzer};
use serde_json::{Value, json};
use std::sync::Arc;

fn definition_after(
    project: &BuiltInlineTestProject,
    source: &str,
    anchor: &str,
    token: &str,
) -> Value {
    let anchor_start = source
        .find(anchor)
        .unwrap_or_else(|| panic!("missing anchor {anchor:?}"));
    let offset = anchor_start
        + source[anchor_start..]
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} after {anchor:?}"));
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": "src/lib.rs", "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn signatures(result: &Value) -> Vec<String> {
    let mut signatures = result["definitions"]
        .as_array()
        .unwrap_or_else(|| panic!("missing definitions: {result:#}"))
        .iter()
        .map(|definition| {
            definition["signature"]
                .as_str()
                .unwrap_or_else(|| panic!("missing signature: {definition:#}"))
                .to_string()
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures
}

fn exact_definition_after(
    project: &crate::common::BuiltInlineTestProject,
    source: &str,
    anchor: &str,
    token: &str,
) -> DefinitionLookupOutcome {
    let anchor_start = source.find(anchor).expect("anchor");
    let start = anchor_start
        + source[anchor_start..]
            .find(token)
            .expect("token after anchor");
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let file = project.file("src/lib.rs");
    resolve_definition_batch_with_source(
        workspace.analyzer(),
        vec![DefinitionLookupRequest {
            file: file.clone(),
            line: None,
            column: None,
            start_byte: Some(start),
            end_byte: Some(start + token.len()),
        }],
        file,
        Arc::from(source),
    )
    .remove(0)
}

fn exact_signatures(result: &DefinitionLookupOutcome) -> Vec<String> {
    let mut signatures = result
        .definitions
        .iter()
        .map(|definition| definition.signature().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    signatures.sort();
    signatures
}

fn token_after(source: &str, anchor: &str, token: &str) -> usize {
    let anchor_start = source.find(anchor).expect("anchor");
    anchor_start
        + source[anchor_start..]
            .find(token)
            .expect("token after anchor")
}

#[test]
fn associated_trait_calls_filter_by_structured_argument_applicability() {
    let source = r#"trait Convert<T> {
    fn from(value: T) -> Self;
}

struct Output;

enum FileState {
    Writer(u32),
}

impl Convert<f32> for Output {
    fn from(_: f32) -> Self { Output }
}

impl Convert<f64> for Output {
    fn from(_: f64) -> Self { Output }
}

impl Convert<&f32> for Output {
    fn from(_: &f32) -> Self { Output }
}

impl Convert<&[f32]> for Output {
    fn from(_: &[f32]) -> Self { Output }
}

fn exact_f32(value: f32) {
    let _ = Output::from(value);
}

fn exact_f64(value: f64) {
    let _ = Output::from(value);
}

fn coercible_mutable_reference(value: &mut f32) {
    let _ = Output::from(value);
}

fn coercible_array_reference(value: &[f32; 2]) {
    let _ = Output::from(value);
}

fn generic<T>(value: T)
where
    Output: Convert<T>,
{
    let _ = Output::from(value);
}

fn enum_variant(value: u32) {
    let _ = FileState::Writer(value);
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2035\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", source)
        .build();

    for (anchor, expected) in [
        ("fn exact_f32", "impl Convert<f32> for Output"),
        ("fn exact_f64", "impl Convert<f64> for Output"),
        (
            "fn coercible_mutable_reference",
            "impl Convert<&f32> for Output",
        ),
        (
            "fn coercible_array_reference",
            "impl Convert<&[f32]> for Output",
        ),
    ] {
        let result = exact_definition_after(&project, source, anchor, "from");
        assert_eq!(
            result.status,
            DefinitionLookupStatus::Resolved,
            "{anchor}: {result:#?}"
        );
        let selected = exact_signatures(&result);
        assert_eq!(selected.len(), 1, "{anchor}: {result:#?}");
        assert!(selected[0].starts_with(expected), "{anchor}: {result:#?}");
    }

    let generic = exact_definition_after(&project, source, "fn generic", "from");
    assert_eq!(
        generic.status,
        DefinitionLookupStatus::Resolved,
        "{generic:#?}"
    );
    assert_eq!(exact_signatures(&generic).len(), 4, "{generic:#?}");

    let f32_target = exact_definition_after(&project, source, "fn exact_f32", "from")
        .definitions
        .into_iter()
        .next()
        .expect("f32 implementation");
    let f64_target = exact_definition_after(&project, source, "fn exact_f64", "from")
        .definitions
        .into_iter()
        .next()
        .expect("f64 implementation");
    let variant_target = exact_definition_after(&project, source, "fn enum_variant", "Writer")
        .definitions
        .into_iter()
        .next()
        .expect("enum variant");
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let analyzer = workspace.analyzer();
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let strategy = RustExportUsageGraphStrategy::new();
    let f32_hits = strategy
        .find_usages(analyzer, &[f32_target], &candidates, 1000)
        .into_either()
        .expect("f32 inverse lookup");
    let f64_hits = strategy
        .find_usages(analyzer, &[f64_target], &candidates, 1000)
        .into_either()
        .expect("f64 inverse lookup");
    let variant_hits = strategy
        .find_usages(analyzer, &[variant_target], &candidates, 1000)
        .into_either()
        .expect("enum variant inverse lookup");
    let exact_f32 = token_after(source, "fn exact_f32", "from");
    let exact_f64 = token_after(source, "fn exact_f64", "from");
    let generic = token_after(source, "fn generic", "from");
    let variant = token_after(source, "fn enum_variant", "Writer");
    assert!(
        f32_hits
            .iter()
            .any(|hit| hit.start_offset == exact_f32 && hit.end_offset == exact_f32 + 4),
        "f32 inverse hits: {f32_hits:#?}"
    );
    assert!(
        f32_hits
            .iter()
            .all(|hit| hit.start_offset != exact_f64 && hit.start_offset != generic),
        "f32 inverse hits: {f32_hits:#?}"
    );
    assert!(
        f64_hits
            .iter()
            .any(|hit| hit.start_offset == exact_f64 && hit.end_offset == exact_f64 + 4),
        "f64 inverse hits: {f64_hits:#?}"
    );
    assert!(
        f64_hits
            .iter()
            .all(|hit| hit.start_offset != exact_f32 && hit.start_offset != generic),
        "f64 inverse hits: {f64_hits:#?}"
    );
    assert!(
        variant_hits
            .iter()
            .any(|hit| hit.start_offset == variant && hit.end_offset == variant + 6),
        "enum variant inverse hits: {variant_hits:#?}"
    );
}

#[test]
fn equally_applicable_traits_remain_ambiguous() {
    let source = r#"trait First<T> { fn choose(value: T) -> Self; }
trait Second<T> { fn choose(value: T) -> Self; }
struct Output;
impl First<f32> for Output { fn choose(_: f32) -> Self { Output } }
impl Second<f32> for Output { fn choose(_: f32) -> Self { Output } }

fn run(value: f32) {
    let _ = Output::choose(value);
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2035_ambiguous\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", source)
        .build();

    let result = definition_after(&project, source, "fn run", "choose");
    assert_eq!(result["status"], "ambiguous", "{result:#}");
    assert_eq!(signatures(&result).len(), 2, "{result:#}");
}
