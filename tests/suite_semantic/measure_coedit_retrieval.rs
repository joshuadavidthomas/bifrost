//! Does the co-edit retrieval leg carry signal on a repository with real git
//! history? (bifrost#2127)
//!
//! The CodeScale corpus cannot answer this. Every one of its repositories is a
//! squashed single-commit mirror, so `most_relevant_files_history_only` there
//! ranks over one commit that added every file and has nothing to rank on.
//!
//! This harness runs the leg against prometheus, which has real history, under
//! a temporal split that makes leakage impossible:
//!
//! - The workspace is checked out at a split commit T. The ranker walks the
//!   1,000 commits ending at T, so every commit it sees is older than T.
//! - Every test case is a commit newer than T. The ranker cannot have seen it.
//!
//! Task: given all but one file of a real commit, rank the held-out file.
//!
//! The harness only produces rankings. Scoring and the baselines live in
//! `score.py` next to the case file, so a baseline can change without a
//! rebuild.
//!
//! Run it with a prepared case file:
//!
//!     BIFROST_COEDIT_EVAL_CASES=<dir>/cases.json \
//!       cargo test --test suite_semantic -- measure_coedit_retrieval --ignored --nocapture

use brokk_bifrost::path_utils::rel_path_string;
use brokk_bifrost::searchtools::{MostRelevantFilesParams, most_relevant_files_history_only};
use brokk_bifrost::{
    AnalyzerConfig, Language, Project, ProjectFile, TestProject, WorkspaceAnalyzer,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Matches `brokk_bifrost_nlp::COEDIT_HALF_LIFE`, so this measures the leg as
/// `semantic_search` actually configures it. The nlp crate is not in a
/// featureless build, so the value is restated rather than imported.
const COEDIT_HALF_LIFE: f64 = 250.0;
/// Deep enough to cover every k the semantic_search callers ask for.
const LIMIT: usize = 50;
const CASES_ENV: &str = "BIFROST_COEDIT_EVAL_CASES";

#[derive(Deserialize)]
struct CaseFile {
    repo: PathBuf,
    lang: String,
    split_commit: String,
    cases: Vec<Case>,
}

/// The corpus tags each repo with the extension family that dominates it, so
/// the harness has to map that back onto the analyzer language.
fn language_for(tag: &str) -> Language {
    match tag {
        "java" => Language::Java,
        "py" => Language::Python,
        "go" => Language::Go,
        "rs" => Language::Rust,
        "rb" => Language::Ruby,
        "php" => Language::Php,
        "kt" => Language::Kotlin,
        "scala" => Language::Scala,
        "cs" => Language::CSharp,
        "ts" => Language::TypeScript,
        "cpp" => Language::Cpp,
        other => panic!("no analyzer language for corpus tag {other}"),
    }
}

#[derive(Deserialize)]
struct Case {
    commit: String,
    seeds: Vec<String>,
    heldout: String,
}

#[derive(Serialize)]
struct Ranking {
    commit: String,
    seeds: Vec<String>,
    heldout: String,
    ranked: Vec<String>,
    /// `false` when the leg declined to rank, which is itself a result.
    complete: bool,
    /// Files the seeds import, and files that import the seeds. The structural
    /// signal `semantic_search` currently declines to pay for: query.rs turns
    /// import ranking off because the full graph can expand for minutes. This
    /// is the depth-1 neighbourhood only, which is cheap.
    imports: Vec<String>,
    importers: Vec<String>,
}

/// Depth-1 import neighbourhood of the seeds, both directions.
fn import_neighbourhood(
    workspace: &WorkspaceAnalyzer,
    root: &std::path::Path,
    seeds: &[String],
) -> (Vec<String>, Vec<String>) {
    let analyzer = workspace.analyzer();
    let Some(provider) = analyzer.import_analysis_provider() else {
        return (Vec::new(), Vec::new());
    };
    let mut imports = std::collections::BTreeSet::new();
    let mut importers = std::collections::BTreeSet::new();
    for seed in seeds {
        let file = ProjectFile::new(root.to_path_buf(), seed.as_str());
        for unit in provider.imported_code_units_of(&file).iter() {
            imports.insert(rel_path_string(unit.source()));
        }
        for referencing in provider.referencing_files_of(&file) {
            importers.insert(rel_path_string(&referencing));
        }
    }
    (
        imports.into_iter().collect(),
        importers.into_iter().collect(),
    )
}

#[test]
#[ignore = "research harness; needs a prepared case file and a repo with real git history"]
fn measure_coedit_retrieval_on_real_history() {
    let Ok(cases_path) = std::env::var(CASES_ENV) else {
        panic!("set {CASES_ENV} to a cases.json built by build_cases.py");
    };
    let case_file: CaseFile =
        serde_json::from_str(&std::fs::read_to_string(&cases_path).expect("read cases"))
            .expect("parse cases");
    assert!(
        case_file.repo.join(".git").exists(),
        "workspace {} is not a git checkout",
        case_file.repo.display()
    );

    let started = Instant::now();
    let project: Arc<dyn Project> = Arc::new(TestProject::new(
        case_file.repo.clone(),
        language_for(&case_file.lang),
    ));
    let workspace = WorkspaceAnalyzer::build(project, AnalyzerConfig::default());
    eprintln!(
        "analyzer built in {:.1}s over {} ({}), split at {}",
        started.elapsed().as_secs_f64(),
        case_file.repo.display(),
        case_file.lang,
        &case_file.split_commit[..9]
    );

    let mut rankings = Vec::with_capacity(case_file.cases.len());
    let ranking_started = Instant::now();
    for (index, case) in case_file.cases.iter().enumerate() {
        let result = most_relevant_files_history_only(
            workspace.analyzer(),
            MostRelevantFilesParams {
                seed_file_paths: case.seeds.clone(),
                // Uniform. semantic_search passes normalized dense scores, but
                // this harness has no dense arm, and inventing weights would
                // measure the invention.
                seed_weights: None,
                recency_half_life: Some(COEDIT_HALF_LIFE),
                ranking_mode: Default::default(),
                limit: LIMIT,
            },
        )
        .expect("co-edit ranking");
        let (imports, importers) = import_neighbourhood(&workspace, &case_file.repo, &case.seeds);
        rankings.push(Ranking {
            commit: case.commit.clone(),
            seeds: case.seeds.clone(),
            heldout: case.heldout.clone(),
            ranked: result.files.into_iter().map(|file| file.path).collect(),
            complete: result.complete,
            imports,
            importers,
        });
        if index.is_multiple_of(25) {
            eprintln!(
                "  {index}/{} cases, {:.1}s elapsed",
                case_file.cases.len(),
                ranking_started.elapsed().as_secs_f64()
            );
        }
    }

    // Derived from the input path, not a fixed name beside it. A shared
    // `rankings.json` made two concurrent runs of this harness overwrite each
    // other's output, which is silent and produces a plausible-looking file.
    let out = PathBuf::from(&cases_path).with_extension("rankings.json");
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&rankings).expect("encode"),
    )
    .expect("write");
    let declined = rankings.iter().filter(|r| !r.complete).count();
    eprintln!(
        "wrote {} rankings to {} in {:.1}s ({declined} incomplete)",
        rankings.len(),
        out.display(),
        ranking_started.elapsed().as_secs_f64()
    );
}
