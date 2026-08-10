//! The demand-sweep fleet runner (#1871 milestone 4.5).
//!
//! This drives the pure demand-sweep core in
//! `brokk_bifrost_semantic_packs::summary_foundry::demand` over a fixed slice of
//! the benchmark corpus. For each repo it builds a workspace analyzer, runs the
//! `require-model` taint policy through the production evaluator, records the
//! run's completion, and, for an `Inconclusive` conclusion, extracts the blocking
//! boundaries the taint report emitted so the core can rank them and split them
//! into summary-closable and engine-blocked buckets.
//!
//! It lives in the facade for the same reason the milestone-3 fixture runner
//! does: it needs both the foundry IR from `brokk-bifrost-semantic-packs` and the
//! evaluator from `brokk-bifrost-policy`, and the workspace dependency rules keep
//! the packs crate below policy. The facade is the only package that sees both.
//!
//! The corpus sweep is a measurement run once, not a test: the pure ranking,
//! bucketing, and baseline logic it feeds is proven hermetically in the core's
//! own unit tests. What this module adds is the live boundary extraction and the
//! per-repo outcome bucketing, both of which a caller runs against a real corpus.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use brokk_bifrost_analysis::CancellationToken;
use brokk_bifrost_analysis::analyzer::{
    AnalyzerConfig, FilesystemProject, JvmDependencyDiscoveryMode, Project, WorkspaceAnalyzer,
};
use brokk_bifrost_policy::{
    PolicyAnalysisType, PolicyEvaluationDate, PolicyEvaluationInput, PolicyEvaluationOptions,
    PolicyRunCompletion, PolicySourceIdentity, evaluate_policy_inputs_with_analyzer,
};
use brokk_bifrost_semantic_packs::summary_foundry::demand::{
    BlockerObservation, CompletionBucket, DemandSweepReport, RepoOutcome, RepoRunOutcome,
    SliceMeta, SliceRepo, blockers_from_report, summarize,
};

/// The evaluation date the sweep pins. The taint policy carries no temporal
/// clause, so the only requirement is that two sweeps pin the same date.
const SWEEP_EVALUATION_DATE: (i32, u32, u32) = (2026, 1, 1);

/// The `require-model` taint policy the sweep runs.
///
/// It is the checked-in
/// `tests/fixtures/policies/attacker-controlled-to-sensitive-sinks.rqlp` made
/// self-contained and forced to `require-model`: the same finding-combination and
/// the same two matching endpoint definitions
/// (`bifrost.sources.http-request-parameter` and
/// `bifrost.sinks.sensitive-user-pii`), inlined so they resolve without the
/// checked-in endpoint directory, which does not exist under a foreign repo root.
/// `require-model` is the mode this milestone rests on: it fails closed on an
/// unmodeled call, which is what turns an unmodeled callee into an `Inconclusive`
/// conclusion with an attributable boundary rather than an over-approximated
/// finding (`paranoid`, the checked-in mode) or a silent pass-through
/// (`optimistic`).
///
/// The endpoint selectors are the checked-in policy's own: Python `(language
/// python ...)` callee-name matches on `request_parameter` and
/// `store_user_profile`. They are faithful to the only checked-in taint policy;
/// they are also why the Java-corpus signal is empty, which the sweep records
/// rather than papers over. This constant does not invent a new source/sink
/// vocabulary; changing that vocabulary is Milestone 5's decision, not this
/// harness's.
pub const REQUIRE_MODEL_TAINT_POLICY: &str = r#"(policy
  :schema-version 1
  :id "bifrost.security.attacker-controlled-to-sensitive-sinks.require-model"
  :name "Attacker-controlled data reaches a sensitive sink (require-model)"
  :message (generated-message :relation can-reach)
  :severity warning
  :analysis
    (analysis
      :type taint
      :mode may
      :call-modeling (call-modeling :unmodeled require-model)
      :sources
        (endpoint-set :entries [
          (source :id http-request-parameter
            :display-name "User-controlled I/O"
            :categories [input.user-controlled io.external]
            :selector (rql :schema-version 1
              (language python (call :callee (name "request_parameter"))))
            :bind return-value
            :labels [attacker-controlled])])
      :sinks
        (endpoint-set :entries [
          (sink :id sensitive-user-pii
            :display-name "sensitive user PII"
            :categories [data.pii data.sensitive]
            :selector (rql :schema-version 1
              (language python (call :callee (name "store_user_profile"))))
            :dangerous-operand (argument :index 0)
            :accepts [attacker-controlled])])
      :finding-combinations [
        (finding-combination
          :id "user-input-to-pii"
          :source (categories :all [input.user-controlled])
          :sink (categories :all [data.pii data.sensitive])
          :message "User-controlled I/O can reach sensitive user PII"
          :supersedes [])]))
"#;

/// One sweep's configuration.
#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// The corpus root that holds the clone directories.
    pub corpus_root: PathBuf,
    /// How many Java repos the slice takes.
    pub max_repos: usize,
    /// The upper bound on a repo's Java-file count. The slice takes the smallest
    /// repos, so this only guards against a pathological giant slipping in.
    pub max_java_files: usize,
    /// The per-repo wall-clock budget. A run that exceeds it is a `TimedOut`
    /// datum, never a crash.
    pub per_repo_timeout: Duration,
}

impl SweepConfig {
    pub fn new(corpus_root: impl Into<PathBuf>, max_repos: usize) -> Self {
        Self {
            corpus_root: corpus_root.into(),
            max_repos,
            max_java_files: 400,
            per_repo_timeout: Duration::from_secs(180),
        }
    }
}

/// A harness failure that stops the whole sweep, distinct from a per-repo datum.
#[derive(Debug)]
pub enum SweepError {
    Io { path: PathBuf, error: io::Error },
}

impl std::fmt::Display for SweepError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => {
                write!(formatter, "sweep io error at {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for SweepError {}

/// Run the whole sweep: select the slice, run the policy over each repo, and
/// aggregate into the deterministic report.
pub fn run_sweep(config: &SweepConfig) -> Result<DemandSweepReport, SweepError> {
    let repos = select_java_slice(config)?;
    let selection_rule = format!(
        "The first {} corpus directories, in sorted-name order, that carry a \
         root-level Maven or Gradle build file (pom.xml, build.gradle, or \
         build.gradle.kts) and hold between one and {} .java files. A repo's git \
         HEAD is recorded when the clone is a git checkout.",
        config.max_repos, config.max_java_files
    );
    let mut outcomes = Vec::with_capacity(repos.len());
    for repo in &repos {
        let root = config.corpus_root.join(&repo.name);
        outcomes.push(run_repo(&repo.name, repo.java_files, &root, config));
    }
    let slice = SliceMeta {
        selection_rule,
        repos,
    };
    Ok(summarize(slice, &outcomes))
}

/// Run the `require-model` taint policy over one repo and bucket the outcome.
///
/// Every failure to run is its own outcome, never a dropped repo: a workspace
/// that will not build, an evaluation error, and a run that exceeds the time
/// budget each take a bucket.
pub fn run_repo(
    name: &str,
    java_files: usize,
    root: &Path,
    config: &SweepConfig,
) -> RepoRunOutcome {
    let cancellation = CancellationToken::new().with_timeout(config.per_repo_timeout);
    let outcome = run_repo_inner(root, &cancellation);
    let outcome = match outcome {
        Ok((completion, blockers)) => {
            return RepoRunOutcome {
                repo: name.to_owned(),
                java_files,
                outcome: completion,
                blockers,
            };
        }
        Err(error) => error,
    };
    // A timeout wins over the error's own text: a cancelled run reports whatever
    // partial state it reached, and that is not the datum we want here.
    let outcome = if cancellation.is_timed_out() {
        RepoOutcome::TimedOut
    } else {
        outcome
    };
    RepoRunOutcome {
        repo: name.to_owned(),
        java_files,
        outcome,
        blockers: Vec::new(),
    }
}

/// The inner run, returning either the concluded outcome plus blockers or a
/// non-ran outcome bucket.
fn run_repo_inner(
    root: &Path,
    cancellation: &CancellationToken,
) -> Result<(RepoOutcome, Vec<BlockerObservation>), RepoOutcome> {
    let project =
        FilesystemProject::new(root).map_err(|error| RepoOutcome::WorkspaceBuildFailed {
            detail: error.to_string(),
        })?;
    let project: Arc<dyn Project> = Arc::new(project);
    let workspace =
        WorkspaceAnalyzer::build_ephemeral(project, sweep_analyzer_config()).map_err(|error| {
            RepoOutcome::WorkspaceBuildFailed {
                detail: error.to_string(),
            }
        })?;

    let inputs = [PolicyEvaluationInput::embedded(
        PolicySourceIdentity::new("bifrost:summary-foundry/demand-sweep-policy"),
        REQUIRE_MODEL_TAINT_POLICY,
    )];
    let (year, month, day) = SWEEP_EVALUATION_DATE;
    let options = PolicyEvaluationOptions::new(
        PolicyEvaluationDate::from_ymd(year, month, day).expect("a fixed evaluation date"),
    );
    let batch = evaluate_policy_inputs_with_analyzer(
        root,
        &inputs,
        &workspace,
        &options,
        Some(cancellation),
    )
    .map_err(|error| RepoOutcome::EvaluationFailed {
        detail: error.to_string(),
    })?;

    // The taint run's public completion is the aggregate over its roots.
    let Some(run) = batch
        .report()
        .runs()
        .iter()
        .find(|run| run.analysis_type() == PolicyAnalysisType::Taint)
    else {
        // No taint run means the policy did not produce one; that is an
        // evaluation gap, not a conclusion.
        let runs = batch
            .report()
            .runs()
            .iter()
            .map(|run| format!("{:?}/{:?}", run.analysis_type(), run.completion()))
            .collect::<Vec<_>>();
        let diagnostics = batch
            .report()
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message().to_owned())
            .collect::<Vec<_>>();
        return Err(RepoOutcome::EvaluationFailed {
            detail: format!(
                "the taint policy produced no taint run; runs={runs:?}; diagnostics={diagnostics:?}"
            ),
        });
    };
    let completion = completion_bucket(run.completion());
    let findings = run.findings().len();

    // Blockers are attributed only to an inconclusive verdict; a clean run has
    // none to attribute. They come from every retained per-root analysis.
    let blockers = if matches!(completion, CompletionBucket::Inconclusive) {
        let mut blockers = Vec::new();
        for analysis in batch.taint_analysis_results() {
            blockers.extend(blockers_from_report(analysis.report()));
        }
        blockers
    } else {
        Vec::new()
    };
    Ok((
        RepoOutcome::Ran {
            completion,
            findings,
        },
        blockers,
    ))
}

/// The analyzer config the sweep builds workspaces with: source-only, with JVM
/// dependency discovery disabled.
///
/// The baseline measures taint over each repo's own source. Disabling dependency
/// discovery keeps the run bounded and deterministic, because it never resolves a
/// dependency against a local Maven or Gradle cache whose contents vary by host,
/// and it never runs a build tool. An unresolved dependency call simply becomes
/// one of the boundaries the sweep already attributes.
fn sweep_analyzer_config() -> AnalyzerConfig {
    let mut config = AnalyzerConfig::default();
    config.jvm.dependency_discovery.mode = JvmDependencyDiscoveryMode::Disabled;
    config
}

/// Project a policy run completion onto the sweep's bucket. This mapping lives
/// here because `PolicyRunCompletion` is above the packs crate that owns the
/// bucket.
pub fn completion_bucket(completion: &PolicyRunCompletion) -> CompletionBucket {
    match completion {
        PolicyRunCompletion::Complete => CompletionBucket::Complete,
        PolicyRunCompletion::ProvenBySummary => CompletionBucket::ProvenBySummary,
        PolicyRunCompletion::ProvenSubset { .. } => CompletionBucket::ProvenSubset,
        PolicyRunCompletion::Inconclusive { .. } => CompletionBucket::Inconclusive,
        PolicyRunCompletion::Unsupported { .. } => CompletionBucket::Unsupported,
        PolicyRunCompletion::Failed { .. } => CompletionBucket::Failed,
    }
}

/// The root-level build files that mark a Maven or Gradle Java project. Gating on
/// one keeps slice selection cheap: only a project with a build file is walked to
/// count its Java files, so the corpus's many non-Java clones are stat-ed, not
/// traversed.
const JAVA_BUILD_FILES: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts"];

/// Select the fixed Java slice deterministically.
///
/// The rule is reproducible and cheap: enumerate the corpus root's immediate
/// directories in sorted-name order, and take the first `max_repos` that carry a
/// root-level Maven or Gradle build file and hold between one and `max_java_files`
/// `.java` files. Sorted-name order with an early stop makes the scan bounded
/// (only the alphabetical prefix is examined, and only build-file projects are
/// walked), and the `.java` cap keeps each selected repo tractable on a shared,
/// loaded box. A repo's git HEAD is recorded when the clone is a git checkout, so
/// the slice pins an exact revision.
pub fn select_java_slice(config: &SweepConfig) -> Result<Vec<SliceRepo>, SweepError> {
    let mut names = fs::read_dir(&config.corpus_root)
        .map_err(|error| SweepError::Io {
            path: config.corpus_root.clone(),
            error,
        })?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();

    let mut selected = Vec::new();
    for name in names {
        if selected.len() >= config.max_repos {
            break;
        }
        let root = config.corpus_root.join(&name);
        if !JAVA_BUILD_FILES
            .iter()
            .any(|build_file| root.join(build_file).exists())
        {
            continue;
        }
        let java_files = count_java_files(&root, config.max_java_files);
        if java_files == 0 || java_files > config.max_java_files {
            continue;
        }
        let commit = git_head(&root);
        selected.push(SliceRepo {
            name,
            java_files,
            commit,
        });
    }
    Ok(selected)
}

/// Count `.java` files under `root` with an explicit stack, stopping once the
/// count exceeds `cap` (the caller rejects a repo over the cap anyway, so there
/// is no reason to keep walking a giant tree). Common vendor and VCS directories
/// are skipped so the count reflects the repo's own sources.
fn count_java_files(root: &Path, cap: usize) -> usize {
    const SKIP: &[&str] = &[
        ".git",
        "node_modules",
        "target",
        "build",
        ".gradle",
        "vendor",
    ];
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let name = entry.file_name();
                if SKIP.iter().any(|skip| name == *skip) {
                    continue;
                }
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "java")
            {
                count += 1;
                if count > cap {
                    return count;
                }
            }
        }
    }
    count
}

/// The git HEAD commit of a clone, when it is a git checkout, so the slice is
/// reproducible against an exact revision. A non-git clone records no commit.
fn git_head(root: &Path) -> Option<String> {
    if !root.join(".git").exists() {
        return None;
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?;
    let commit = commit.trim();
    (!commit.is_empty()).then(|| commit.to_owned())
}

/// Write the deterministic report to `path` as pretty JSON, creating parents.
/// No timestamp is written, so the same slice yields byte-identical bytes.
pub fn write_report(report: &DemandSweepReport, path: &Path) -> Result<(), SweepError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| SweepError::Io {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    let mut rendered = serde_json::to_string_pretty(report).expect("the report is serializable");
    rendered.push('\n');
    fs::write(path, rendered.as_bytes()).map_err(|error| SweepError::Io {
        path: path.to_path_buf(),
        error,
    })
}
