//! Issue #2118: C# census sampling must use the same directive-aware syntax
//! tree as declaration extraction, navigation, and inverse usage analysis.

use crate::common::InlineTestProject;
use brokk_bifrost::reference_differential::{
    ExactReferenceSite, ProbeSeed, ReferenceDifferentialConfig, run_reference_differential,
};
use brokk_bifrost::{AnalyzerConfig, Language};

fn config() -> ReferenceDifferentialConfig {
    ReferenceDifferentialConfig {
        corpus_language: "csharp".to_string(),
        max_files: 10,
        max_sites: 1_000,
        max_candidates_per_file: 1_000,
        max_source_bytes: 20_000,
        max_targets: 1_000,
        max_usage_files: 10,
        max_usages: 1_000,
        probe_seed: ProbeSeed::Census,
        ..ReferenceDifferentialConfig::default()
    }
}

#[test]
fn csharp_census_excludes_the_same_inactive_branch_as_the_analyzer() {
    let source = r#"namespace Example;

public sealed class Tree
{
    public Tree()
    {
#if NET6_0_OR_GREATER
        Root = BuildTree(new Span<int>());
#else
        Root = BuildTree(new ArraySegment<int>());
#endif
    }

    public int Root { get; }

#if NET6_0_OR_GREATER
    private int BuildTree(Span<int> values) => values.Length;
#else
    private int BuildTree(ArraySegment<int> values) => values.Count;
#endif
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Tree.cs", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let report = run_reference_differential(workspace.analyzer(), &config())
        .expect("run C# census differential");
    let active_call = source
        .find("BuildTree(new Span")
        .expect("active branch call");
    let inactive_call = source
        .find("BuildTree(new ArraySegment")
        .expect("inactive branch call");

    assert!(
        report
            .sites
            .iter()
            .any(|site| site.start_byte == active_call),
        "the selected branch must remain in the census: {report:#?}"
    );
    assert!(
        report
            .sites
            .iter()
            .all(|site| site.start_byte != inactive_call),
        "the excluded branch must not be sampled from a raw parse: {report:#?}"
    );
    assert!(
        !report.has_actionable_findings(),
        "selected-branch references must round trip: {report:#?}"
    );

    let exact_error = run_reference_differential(
        workspace.analyzer(),
        &ReferenceDifferentialConfig {
            exact_site: Some(ExactReferenceSite {
                path: "Tree.cs".to_string(),
                start_byte: inactive_call,
                end_byte: Some(inactive_call + "BuildTree".len()),
            }),
            ..config()
        },
    )
    .expect_err("an excluded branch is not a structured reference site");
    assert!(
        exact_error.contains("exact site did not match"),
        "unexpected exact-site error: {exact_error}"
    );
}
