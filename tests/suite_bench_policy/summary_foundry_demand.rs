//! Milestone 4.5 of the procedure-summary foundry (#1871): the demand sweep.
//!
//! The pure ranking, bucketing, and baseline logic is proven hermetically in the
//! core's own unit tests (`summary_foundry::demand`). This suite proves the two
//! things those unit tests cannot reach without a live run:
//!
//! * the boundary extraction reads a real taint report. A fixture fail-before
//!   case is a workspace whose modelled call is unmodeled under `require-model`,
//!   so it concludes `Inconclusive` with exactly the open boundary the sweep
//!   attributes; the extraction must return a summary-closable blocker for it.
//! * the corpus sweep driver runs end to end. Gated behind an environment
//!   variable that names the corpus root, it selects the fixed Java slice, runs
//!   the require-model policy over each repo, and writes the deterministic report.
//!   It is `#[ignore]` because it is a measurement over real repos, not a unit
//!   test; the milestone's committed artifact comes from running it once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use brokk_bifrost::analyzer::semantic_model::{
    AuthoredSummaryExitKind, AuthoredSummaryInput, AuthoredSummaryOutput, AuthoredSummaryTransfer,
};
use brokk_bifrost::policy::{
    PolicyAnalysisType, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyIncompleteReason, PolicyRunCompletion, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer,
};
use brokk_bifrost::semantic_packs::summary_foundry::demand::{
    BlockerBucket, CompletionBucket, RepoOutcome, blockers_from_report,
};
use brokk_bifrost::semantic_packs::summary_foundry::fixture::{
    FixtureCase, FixturePolarity, JavaFixtureLanguage, generate_fixture_suite,
};
use brokk_bifrost::semantic_packs::summary_foundry::ir::{
    FoundryArtifactBinding, FoundryBoundary, FoundryClaim, FoundryCompleteness, FoundryCorpus,
    FoundryEntry, FoundrySignature, FoundryTarget, summary_id,
};
use brokk_bifrost::summary_foundry_demand::{
    SweepConfig, completion_bucket, run_sweep, select_java_slice, write_report,
};
use brokk_bifrost::{AnalyzerConfig, FilesystemProject, Project, WorkspaceAnalyzer};

/// A one-parameter propagation with a bodyless target, in the foundry IR. Its
/// fail-before case is a workspace with an unmodeled call, which is what the
/// sweep attributes a blocker to.
fn value_of_entry() -> FoundryEntry {
    let target = FoundryTarget {
        artifact_path: "java/lang/String.class".to_owned(),
        member: "valueOf".to_owned(),
        signature: FoundrySignature::Overload {
            types: vec!["Object".to_owned()],
        },
    };
    FoundryEntry {
        id: summary_id(FoundryCorpus::Derived, &target),
        corpus: FoundryCorpus::Derived,
        target,
        boundary: FoundryBoundary {
            has_receiver: false,
            parameter_count: 1,
        },
        claim: FoundryClaim::Flows,
        completeness: FoundryCompleteness::Partial,
        transfers: vec![AuthoredSummaryTransfer {
            input: AuthoredSummaryInput::Parameter { ordinal: 0 },
            exit_kind: AuthoredSummaryExitKind::Normal,
            output: AuthoredSummaryOutput::NormalReturn {},
        }],
        artifact: FoundryArtifactBinding::Unresolved,
        evidence: Vec::new(),
        notes: Vec::new(),
        derivation: None,
    }
}

/// Materialize one case's files beneath `root`.
fn materialize(case: &FixtureCase, root: &Path) {
    std::fs::create_dir_all(root).expect("case root");
    for file in &case.files {
        let path = root.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("case parent dir");
        }
        std::fs::write(&path, &file.contents).expect("case file");
    }
}

/// The fixture fail-before case is a real `Inconclusive` run whose only open
/// boundary is the unmodeled modelled call. The live extraction must attribute a
/// summary-closable blocker to it, which is the whole point of the sweep: the
/// unmodeled stdlib call is exactly what a procedure summary would close.
#[test]
fn the_extraction_attributes_a_summary_closable_blocker_to_an_unmodeled_call() {
    let entry = value_of_entry();
    let suite = generate_fixture_suite(&entry, &JavaFixtureLanguage).expect("the entry renders");
    let case = suite
        .cases
        .iter()
        .find(|case| case.polarity == FixturePolarity::FailBefore)
        .expect("a fail-before case exists");
    assert!(
        case.pack_source.is_none(),
        "the fail-before case carries no pack, so its modelled call is unmodeled"
    );

    let directory = tempfile::tempdir().expect("workspace root");
    let root = directory.path();
    materialize(case, root);

    let project = FilesystemProject::new(root).expect("filesystem project");
    let project: Arc<dyn Project> = Arc::new(project);
    let workspace = WorkspaceAnalyzer::build_ephemeral(project, AnalyzerConfig::default())
        .expect("the fixture workspace builds");

    let policy_source = case
        .files
        .iter()
        .find(|file| file.path == case.policy_path)
        .map(|file| file.contents.clone())
        .expect("the case carries its policy");
    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("bifrost:summary-foundry/demand-extraction-test"),
        &policy_source,
    )];
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(2026, 1, 1).expect("a fixed date"),
    );
    let outcome = evaluate_policy_inputs_with_analyzer(root, &inputs, &workspace, &options, None)
        .expect("evaluation succeeds");

    let run = outcome
        .report()
        .runs()
        .iter()
        .find(|run| run.analysis_type() == PolicyAnalysisType::Taint)
        .expect("a taint run");
    assert!(
        matches!(run.completion(), PolicyRunCompletion::Inconclusive { .. }),
        "the fail-before control must be inconclusive, was {:?}",
        run.completion()
    );
    assert_eq!(
        completion_bucket(run.completion()),
        CompletionBucket::Inconclusive
    );

    let mut blockers = Vec::new();
    for analysis in outcome.taint_analysis_results() {
        blockers.extend(blockers_from_report(analysis.report()));
    }
    assert!(
        !blockers.is_empty(),
        "an inconclusive require-model run over an unmodeled call must attribute a blocker"
    );
    // The unmodeled modelled call is a summary-closable blocker: a procedure
    // summary is exactly what would close it. The same tiny workspace also emits
    // engine-blocked boundaries (a provider envelope and an unresolved dispatch),
    // so this asserts the summary-closable one is present rather than that every
    // blocker is summary-closable -- the split itself is the point.
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker.bucket() == BlockerBucket::SummaryClosable),
        "the modelled stdlib call must surface as a summary-closable blocker, got {blockers:#?}"
    );
}

/// The completion mapping is total and preserves the clean/inconclusive split.
#[test]
fn the_completion_mapping_covers_every_variant() {
    assert!(completion_bucket(&PolicyRunCompletion::Complete).is_clean_conclusion());
    assert!(completion_bucket(&PolicyRunCompletion::ProvenBySummary).is_clean_conclusion());
    assert_eq!(
        completion_bucket(
            &PolicyRunCompletion::inconclusive(vec![PolicyIncompleteReason::PartialDiscovery,])
                .expect("canonical")
        ),
        CompletionBucket::Inconclusive
    );
    assert!(
        !completion_bucket(
            &PolicyRunCompletion::inconclusive(vec![PolicyIncompleteReason::PartialDiscovery,])
                .expect("canonical")
        )
        .is_clean_conclusion()
    );
}

/// The corpus sweep, run once as a measurement. It selects the fixed Java slice
/// named by the corpus root, runs the require-model policy over each repo, and
/// writes the deterministic report to the path in `BIFROST_DEMAND_SWEEP_OUT` (or
/// a default beside the artifact tree).
///
/// Run it with, for example:
///     BIFROST_DEMAND_SWEEP_CORPUS=/path/to/clones \
///     BIFROST_DEMAND_SWEEP_MAX_REPOS=12 \
///     BIFROST_DEMAND_SWEEP_OUT=semantic-packs/summary-foundry/demand-sweep-java.json \
///     cargo test --features release-tooling --test suite_bench_policy -- \
///       --ignored summary_foundry_demand::runs_the_corpus_demand_sweep
#[test]
#[ignore = "measurement over the real corpus; run once to produce the committed artifact"]
fn runs_the_corpus_demand_sweep() {
    let Ok(corpus_root) = std::env::var("BIFROST_DEMAND_SWEEP_CORPUS") else {
        panic!("set BIFROST_DEMAND_SWEEP_CORPUS to the corpus root");
    };
    let max_repos = std::env::var("BIFROST_DEMAND_SWEEP_MAX_REPOS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(12);
    let out = std::env::var("BIFROST_DEMAND_SWEEP_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("semantic-packs/summary-foundry/demand-sweep-java.json")
        });

    let mut config = SweepConfig::new(corpus_root, max_repos);
    config.per_repo_timeout = Duration::from_secs(240);
    let slice = select_java_slice(&config).expect("the slice selects");
    eprintln!("selected {} java repos for the slice", slice.len());
    for repo in &slice {
        eprintln!("  {} ({} java files)", repo.name, repo.java_files);
    }

    let report = run_sweep(&config).expect("the sweep runs");
    for outcome in &report.repo_outcomes {
        eprintln!("  repo {} -> {:?}", outcome.repo, outcome.outcome);
    }
    eprintln!(
        "baseline clean-conclusion: {}/{} runs ({} permille); blocked verdicts: {}; \
         engine-blocked verdict share: {} permille",
        report.clean_conclusion_runs,
        report.evaluated_runs,
        report.clean_conclusion_permille,
        report.verdict_split.blocked_verdicts,
        report.verdict_split.engine_blocked_verdict_permille,
    );
    for blocker in &report.ranked_blockers {
        eprintln!(
            "  blocker {} [{}] callee={:?} blocks {} verdicts",
            blocker.kind.label(),
            blocker.bucket.label(),
            blocker.callee,
            blocker.blocked_verdicts,
        );
    }

    write_report(&report, &out).expect("the report writes");
    eprintln!("wrote {}", out.display());

    // The sweep must have run every selected repo, and every repo is accounted
    // for in exactly one outcome bucket (no silent drop).
    let tallies = &report.outcomes;
    let accounted = tallies.complete
        + tallies.proven_by_summary
        + tallies.proven_subset
        + tallies.inconclusive
        + tallies.unsupported
        + tallies.failed
        + tallies.workspace_build_failed
        + tallies.evaluation_failed
        + tallies.timed_out;
    assert_eq!(accounted, report.repo_count, "every repo takes one bucket");
    // A non-ran outcome is a datum, so assert only that the accounting holds.
    let _ = RepoOutcome::TimedOut;
}
