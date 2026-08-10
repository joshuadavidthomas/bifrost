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
    PolicyIncompleteReason, PolicyRunCompletion, PolicySourceIdentity,
    evaluate_policy_inputs_with_analyzer,
};
use brokk_bifrost_semantic_packs::summary_foundry::codeql::translate_codeql_taint_endpoints;
use brokk_bifrost_semantic_packs::summary_foundry::demand::{
    BlockerObservation, CompletionBucket, DemandSweepReport, RepoOutcome, RepoRunOutcome,
    SliceMeta, SliceRepo, blockers_from_report, summarize,
};
use brokk_bifrost_semantic_packs::summary_foundry::ir::FoundryTaintEndpoint;
use brokk_bifrost_semantic_packs::summary_foundry::taint_policy::{
    PolicyShape, build_require_model_java_taint_policy, import_detectable_packages,
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
        outcomes.push(run_repo(
            &repo.name,
            repo.java_files,
            &root,
            REQUIRE_MODEL_TAINT_POLICY,
            config,
        ));
    }
    let slice = SliceMeta {
        selection_rule,
        repos,
    };
    Ok(summarize(slice, &outcomes))
}

/// A realistic-sweep run: the deterministic report plus the policy it ran.
///
/// The policy is returned so the driver can write it beside the report, which
/// makes the run reproducible and the selectors auditable.
#[derive(Debug, Clone)]
pub struct RealisticSweep {
    pub report: DemandSweepReport,
    pub policy: String,
    pub policy_shape: PolicyShape,
    pub endpoint_count: usize,
    pub import_packages: Vec<String>,
}

/// Load and translate every CodeQL Models-as-Data file under `dir` into taint
/// endpoints. A `.model.yml` file that will not parse stops the load, because a
/// silently-skipped corpus file would understate the endpoint set.
pub fn load_codeql_endpoints(dir: &Path) -> Result<Vec<FoundryTaintEndpoint>, SweepError> {
    let mut files = Vec::new();
    let mut names = fs::read_dir(dir)
        .map_err(|error| SweepError::Io {
            path: dir.to_path_buf(),
            error,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".model.yml"))
        })
        .collect::<Vec<_>>();
    names.sort();
    for path in names {
        let bytes = fs::read(&path).map_err(|error| SweepError::Io {
            path: path.clone(),
            error,
        })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a .model.yml path has a file name")
            .to_owned();
        files.push((name, bytes));
    }
    let translation = translate_codeql_taint_endpoints(&files).map_err(|error| SweepError::Io {
        path: dir.to_path_buf(),
        error: io::Error::new(io::ErrorKind::InvalidData, error.to_string()),
    })?;
    Ok(translation.endpoints)
}

/// Run the realistic demand sweep: build a `require-model` Java taint policy from
/// the endpoints, select the slice by whether a repo imports the endpoint
/// libraries, run the policy over each selected repo, and aggregate.
///
/// This is the milestone-4.6 counterpart of [`run_sweep`]. It shares every piece
/// of the run machinery -- the per-repo runner, the outcome bucketing, the
/// deterministic aggregation -- and differs only in the two inputs milestone 4.5
/// could not supply: a policy whose selectors match real Java, and a slice
/// chosen for using the APIs that policy names.
pub fn run_realistic_sweep(
    config: &SweepConfig,
    endpoints: &[FoundryTaintEndpoint],
) -> Result<RealisticSweep, SweepError> {
    let (policy, policy_shape) = build_require_model_java_taint_policy(endpoints);
    let import_packages = import_detectable_packages(endpoints);
    let repos = select_java_slice_importing(config, &import_packages)?;
    let selection_rule = format!(
        "The first {} corpus directories, in sorted-name order, that carry a \
         root-level Maven or Gradle build file (pom.xml, build.gradle, or \
         build.gradle.kts), hold between one and {} .java files, and import at \
         least one endpoint library ({}). The import scan is a slice-selection \
         heuristic over `import` declarations; the taint analysis itself is fully \
         structural. `java.lang` endpoints (Runtime, ProcessBuilder, System) are \
         imported implicitly and so cannot gate selection, but they still fire in \
         a selected repo that uses them. A repo's git HEAD is recorded when the \
         clone is a git checkout.",
        config.max_repos,
        config.max_java_files,
        import_packages.join(", "),
    );
    let mut outcomes = Vec::with_capacity(repos.len());
    for repo in &repos {
        let root = config.corpus_root.join(&repo.name);
        outcomes.push(run_repo(
            &repo.name,
            repo.java_files,
            &root,
            &policy,
            config,
        ));
    }
    let slice = SliceMeta {
        selection_rule,
        repos,
    };
    let report = summarize(slice, &outcomes);
    Ok(RealisticSweep {
        report,
        policy,
        policy_shape,
        endpoint_count: endpoints.len(),
        import_packages,
    })
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
    policy: &str,
    config: &SweepConfig,
) -> RepoRunOutcome {
    let cancellation = CancellationToken::new().with_timeout(config.per_repo_timeout);
    let outcome = run_repo_inner(root, policy, &cancellation);
    let outcome = match outcome {
        Ok((completion, blockers, inconclusive_reasons)) => {
            return RepoRunOutcome {
                repo: name.to_owned(),
                java_files,
                outcome: completion,
                blockers,
                inconclusive_reasons,
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
        inconclusive_reasons: Vec::new(),
    }
}

/// The typed reasons an inconclusive completion gave, as the policy layer's own
/// snake_case labels. `PolicyIncompleteReason` is a unit-variant serde enum, so
/// serializing each yields its stable label; the reasons are already sorted and
/// deduplicated by the policy layer.
fn incomplete_reason_labels(reasons: &[PolicyIncompleteReason]) -> Vec<String> {
    reasons
        .iter()
        .filter_map(|reason| {
            serde_json::to_value(reason)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
        })
        .collect()
}

/// The inner run, returning either the concluded outcome plus blockers and any
/// inconclusive reasons, or a non-ran outcome bucket.
fn run_repo_inner(
    root: &Path,
    policy: &str,
    cancellation: &CancellationToken,
) -> Result<(RepoOutcome, Vec<BlockerObservation>, Vec<String>), RepoOutcome> {
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
        policy,
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
    // The run-level abstention reasons are captured alongside, because a real
    // require-model run over these workspaces abstains at a coarse run-level
    // reason (for example `capability_incomplete`) and retains no per-root
    // analysis, so the reasons are the only structured account of the verdict.
    let (blockers, inconclusive_reasons) = if matches!(completion, CompletionBucket::Inconclusive) {
        let mut blockers = Vec::new();
        for analysis in batch.taint_analysis_results() {
            blockers.extend(blockers_from_report(analysis.report()));
        }
        let reasons = match run.completion() {
            PolicyRunCompletion::Inconclusive { reasons } => incomplete_reason_labels(reasons),
            _ => Vec::new(),
        };
        (blockers, reasons)
    } else {
        (Vec::new(), Vec::new())
    };
    Ok((
        RepoOutcome::Ran {
            completion,
            findings,
        },
        blockers,
        inconclusive_reasons,
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

/// Select the fixed Java slice by whether a repo uses the endpoint libraries.
///
/// This is the milestone-4.6 selection rule and the lesson of milestone 4.5: a
/// slice chosen only by "has a build file" matched none of the sourced or sunk
/// APIs, so the policy could never fire. Here a repo is selected only if it also
/// imports at least one of `packages` -- the import-detectable endpoint libraries
/// (`java.sql`, `javax.servlet.http`, ...). The scan is bounded: only build-file
/// projects in the sorted-name prefix are walked, each repo's `.java` files are
/// read only until a matching import is found, and the walk stops at `max_repos`.
///
/// If, after selecting for import-use, coverage is still thin, that is a
/// reportable finding, not a reason to relax the rule.
pub fn select_java_slice_importing(
    config: &SweepConfig,
    packages: &[String],
) -> Result<Vec<SliceRepo>, SweepError> {
    let prefixes = packages
        .iter()
        .map(|package| format!("{package}."))
        .collect::<Vec<_>>();
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
        let scan = scan_repo_java(&root, config.max_java_files, &prefixes);
        if scan.java_files == 0 || scan.java_files > config.max_java_files || !scan.imports_endpoint
        {
            continue;
        }
        let commit = git_head(&root);
        selected.push(SliceRepo {
            name,
            java_files: scan.java_files,
            commit,
        });
    }
    Ok(selected)
}

/// The result of one repo's slice-selection scan: its `.java` count and whether
/// any of its `.java` files imports one of the endpoint libraries.
struct RepoJavaScan {
    java_files: usize,
    imports_endpoint: bool,
}

/// Walk `root` once, counting `.java` files and detecting an endpoint-library
/// import. The count stops growing past `cap` (the caller rejects an over-cap
/// repo anyway); the import flag latches on the first match and is not re-checked.
fn scan_repo_java(root: &Path, cap: usize, import_prefixes: &[String]) -> RepoJavaScan {
    const SKIP: &[&str] = &[
        ".git",
        "node_modules",
        "target",
        "build",
        ".gradle",
        "vendor",
    ];
    let mut stack = vec![root.to_path_buf()];
    let mut java_files = 0usize;
    let mut imports_endpoint = false;
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
                java_files += 1;
                if !imports_endpoint && file_imports_any(&path, import_prefixes) {
                    imports_endpoint = true;
                }
                if java_files > cap {
                    // The caller rejects an over-cap repo regardless of imports,
                    // so there is no reason to keep walking a giant tree.
                    return RepoJavaScan {
                        java_files,
                        imports_endpoint,
                    };
                }
            }
        }
    }
    RepoJavaScan {
        java_files,
        imports_endpoint,
    }
}

/// Whether a `.java` file has an `import` declaration for one of the endpoint
/// libraries. This reads `import` lines only, a well-defined lexical construct
/// at the top of a Java file, purely to pre-filter the slice; it makes no claim
/// about the file's semantics. Import declarations precede type declarations, so
/// the scan stops at the first type declaration.
fn file_imports_any(path: &Path, import_prefixes: &[String]) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("import ") {
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("static ").unwrap_or(rest);
            if import_prefixes
                .iter()
                .any(|prefix| rest.starts_with(prefix.as_str()))
            {
                return true;
            }
        } else if is_type_declaration_start(trimmed) {
            // Imports are all above the first type declaration; nothing below it
            // can be an import, so stop reading this file.
            break;
        }
    }
    false
}

/// A cheap check that a trimmed line begins a top-level type declaration, used
/// only to stop the import scan early. It never gates taint analysis.
fn is_type_declaration_start(trimmed: &str) -> bool {
    const STARTS: &[&str] = &[
        "public class ",
        "public final class ",
        "public abstract class ",
        "class ",
        "final class ",
        "abstract class ",
        "public interface ",
        "interface ",
        "public enum ",
        "enum ",
        "public record ",
        "record ",
    ];
    STARTS.iter().any(|start| trimmed.starts_with(start))
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

/// Write the built policy to `path`, creating parents. The policy is what makes
/// the realistic sweep reproducible and its selectors auditable, so it is
/// committed beside the report.
pub fn write_policy(policy: &str, path: &Path) -> Result<(), SweepError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| SweepError::Io {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    fs::write(path, policy.as_bytes()).map_err(|error| SweepError::Io {
        path: path.to_path_buf(),
        error,
    })
}
