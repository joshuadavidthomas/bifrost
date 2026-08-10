//! End-to-end semantic_search pipeline test with deterministic fake engines:
//! hydration -> bounded dense candidate scan -> per-hit liveness resolution ->
//! co-edit relevance, returned as two independent ranked lists.

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use brokk_bifrost::nlp::engine::{Embedder, FakeHashEmbedder};
use brokk_bifrost::nlp::indexer::{EngineProvider, FakeEngineProvider, SemanticIndexer};
use brokk_bifrost::nlp::query::{SemanticSearchParams, SemanticSearchResult, semantic_search};
use brokk_bifrost::{AnalyzerConfig, FilesystemProject, Project, WorkspaceAnalyzer};

fn all_legs_empty(result: &SemanticSearchResult) -> bool {
    result.vector_ranked.is_empty() && result.coedit_ranked.is_empty()
}

fn assert_normalized_symbol_scores(scores: impl IntoIterator<Item = f32>) {
    for score in scores {
        assert!(
            (0.01..=1.0).contains(&score),
            "symbol score should be normalized for caller-side fusion, got {score}"
        );
    }
}

fn write_java(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn snapshot_for(root: &Path) -> Arc<WorkspaceAnalyzer> {
    let project: Arc<dyn Project> = Arc::new(FilesystemProject::new(root.to_path_buf()).unwrap());
    Arc::new(WorkspaceAnalyzer::build(project, AnalyzerConfig::default()))
}

/// Semantic search now requires a git repo (the cache is keyed by blob OID), so
/// every fixture initializes one and commits the files written so far.
fn init_git(dir: &Path) {
    run_git(dir, &["init", "-q"]);
    run_git(dir, &["config", "user.email", "t@example.com"]);
    run_git(dir, &["config", "user.name", "T"]);
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

struct BlockingEmbedder {
    state: Mutex<BlockingState>,
    entered: Condvar,
    released: Condvar,
}

struct BlockingState {
    in_embed: bool,
    release: bool,
}

impl BlockingEmbedder {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(BlockingState {
                in_embed: false,
                release: false,
            }),
            entered: Condvar::new(),
            released: Condvar::new(),
        })
    }

    fn wait_until_embedding(&self) {
        let mut state = self.state.lock().expect("blocking embedder mutex poisoned");
        while !state.in_embed {
            state = self
                .entered
                .wait(state)
                .expect("blocking embedder mutex poisoned");
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("blocking embedder mutex poisoned");
        state.release = true;
        self.released.notify_all();
    }

    fn block_next(&self) {
        let mut state = self.state.lock().expect("blocking embedder mutex poisoned");
        state.in_embed = false;
        state.release = false;
    }
}

impl Embedder for BlockingEmbedder {
    fn dim(&self) -> usize {
        1
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        let mut state = self.state.lock().expect("blocking embedder mutex poisoned");
        state.in_embed = true;
        self.entered.notify_all();
        while !state.release {
            state = self
                .released
                .wait(state)
                .expect("blocking embedder mutex poisoned");
        }
        Ok(texts.iter().map(|_| vec![1.0]).collect())
    }

    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![1.0])
    }

    fn fingerprint(&self) -> String {
        "semantic-search-blocking-test-embedder:v1".to_string()
    }
}

struct BlockingEngineProvider {
    embedder: Arc<BlockingEmbedder>,
}

impl EngineProvider for BlockingEngineProvider {
    fn embedder(&self) -> Result<Arc<dyn Embedder>, String> {
        Ok(self.embedder.clone())
    }
}

struct PanickingEmbedder;

impl Embedder for PanickingEmbedder {
    fn dim(&self) -> usize {
        1
    }

    fn embed_passages(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        panic!("synthetic embed panic");
    }

    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, String> {
        Ok(vec![1.0])
    }

    fn fingerprint(&self) -> String {
        "semantic-search-panicking-test-embedder:v1".to_string()
    }
}

struct PanickingEngineProvider;

impl EngineProvider for PanickingEngineProvider {
    fn embedder(&self) -> Result<Arc<dyn Embedder>, String> {
        Ok(Arc::new(PanickingEmbedder))
    }
}

#[test]
fn semantic_search_returns_every_live_function_in_the_dense_leg() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "ConfigLoader.java",
        "public class ConfigLoader {\n  public String loadConfig(String path) { return path; }\n}\n",
    );
    write_java(
        dir.path(),
        "HttpClient.java",
        "public class HttpClient {\n  public int fetchUrl(String url) { return url.length(); }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "where does loadConfig read the configuration".to_string(),
            k: 2,
        },
    )
    .expect("semantic_search succeeds");

    // Both functions are live and the leg is deeper than the corpus, so the
    // candidate walk must resolve both of them by fully-qualified name.
    let ranked: Vec<&str> = result
        .vector_ranked
        .iter()
        .map(|row| row.fqfn.as_str())
        .collect();
    assert!(
        ranked.iter().any(|fqfn| fqfn.contains("loadConfig")),
        "dense leg surfaces loadConfig: {ranked:?}"
    );
    assert!(
        ranked.iter().any(|fqfn| fqfn.contains("fetchUrl")),
        "dense leg surfaces fetchUrl: {ranked:?}"
    );
    assert_normalized_symbol_scores(result.vector_ranked.iter().map(|row| row.score));
    assert_eq!(
        result.vector_ranked.first().map(|row| row.score),
        Some(1.0),
        "top vector result should normalize to 1.0"
    );
    indexer.close();
}

#[test]
fn semantic_search_handles_initial_build_race() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );

    // Issued immediately after start: must not error while the first build is
    // still racing the query path. Slow Windows runners can legitimately hit
    // the query timeout and return the documented partial-result shape here, so
    // assert the no-error contract first and check ranking after explicit
    // readiness below.
    let _initial_result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 1,
        },
    )
    .expect("query issued during build does not fail");

    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 1,
        },
    )
    .expect("query after build readiness");
    assert_eq!(
        result.vector_ranked.len(),
        1,
        "the single greet() function chunk is ranked"
    );

    indexer.close();
}

#[test]
fn semantic_search_waits_for_the_initial_live_set() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = BlockingEmbedder::new();
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        BlockingEngineProvider {
            embedder: embedder.clone(),
        },
    );
    embedder.wait_until_embedding();

    let (result_tx, result_rx) = mpsc::channel();
    let query_snapshot = snapshot.clone();
    let query_indexer = indexer.clone();
    let query = std::thread::spawn(move || {
        result_tx
            .send(semantic_search(
                &query_snapshot,
                &query_indexer,
                SemanticSearchParams {
                    query: "greet a user by name".to_string(),
                    k: 1,
                },
            ))
            .unwrap();
    });

    assert!(
        result_rx
            .recv_timeout(Duration::from_millis(1_100))
            .is_err(),
        "the first query must not return an empty result after the old one-second timeout"
    );
    embedder.release();
    let result = result_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("query completes once the initial index is ready")
        .expect("initial query succeeds");
    query.join().unwrap();
    assert!(
        !result.vector_ranked.is_empty(),
        "the first query uses the newly hydrated live set"
    );

    indexer.close();
}

#[test]
fn semantic_search_returns_current_results_while_replacement_builds() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let initial_snapshot = snapshot_for(dir.path());
    let embedder = BlockingEmbedder::new();
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        initial_snapshot,
        BlockingEngineProvider {
            embedder: embedder.clone(),
        },
    );
    embedder.wait_until_embedding();
    embedder.release();
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    embedder.block_next();
    write_java(
        dir.path(),
        "Farewell.java",
        "public class Farewell {\n  public String goodbye(String name) { return name; }\n}\n",
    );
    run_git(dir.path(), &["add", "-A"]);
    run_git(dir.path(), &["commit", "-q", "-m", "add farewell"]);
    let replacement_snapshot = snapshot_for(dir.path());
    indexer.request_full_build(replacement_snapshot.clone());
    embedder.wait_until_embedding();

    let result = semantic_search(
        &replacement_snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 1,
        },
    )
    .expect("query should fall back to the active index while its replacement builds");
    assert!(
        !all_legs_empty(&result),
        "the prior active index remains searchable"
    );
    assert!(
        result
            .notes
            .iter()
            .any(|note| note.contains("still building")),
        "notes should explain the stale-index fallback: {:?}",
        result.notes
    );

    embedder.release();
    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    indexer.close();
}

#[test]
fn semantic_index_status_counts_indexed_and_waiting_files() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );

    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    let status = indexer.status(&snapshot);
    assert_eq!(status.indexed_files, 1, "only Greeter.java is live");
    assert_eq!(status.pending_batches, 0);
    assert_eq!(status.phase, "ready");
    assert!(
        status.materialized_files > 0,
        "materialized files: {}",
        status.materialized_files
    );
    assert_eq!(
        status.materialized_files, status.materialize_total_files,
        "successful build should complete every materialization target"
    );
    indexer.close();
}

#[test]
fn semantic_index_worker_panic_surfaces_as_failed_status() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        PanickingEngineProvider,
    );

    let err = indexer
        .wait_ready(Duration::from_secs(30))
        .expect_err("panic should become a structured indexer failure");
    assert!(
        err.contains("panicked"),
        "error should describe the worker panic, got: {err}"
    );
    assert!(
        !err.contains("still building"),
        "panic path should not hit readiness timeout: {err}"
    );
    let status = indexer.status(&snapshot);
    assert_eq!(status.phase, "failed");
    assert_eq!(status.pending_batches, 0);
    indexer.close();
}

#[test]
fn revert_reuses_cached_blob_vectors() {
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().unwrap();
    let original =
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n";
    let edited = "public class Greeter {\n  public String greet(String name) { return \"hi \" + name; }\n}\n";
    write_java(dir.path(), "Greeter.java", original);
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider {
            embedder: embedder.clone(),
        },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let file = snapshot
        .analyzer()
        .analyzed_files()
        .into_iter()
        .next()
        .expect("one analyzed file");
    let changed: BTreeSet<_> = [file].into_iter().collect();

    // Edit + commit -> new blob OID -> the new content is embedded.
    write_java(dir.path(), "Greeter.java", edited);
    run_git(dir.path(), &["commit", "-aqm", "edit"]);
    let snapshot2 = Arc::new(snapshot.update(&changed));
    indexer.request_update(snapshot2.clone(), changed.clone());
    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    let after_edit = embedder.texts_embedded();
    assert!(after_edit > 0, "editing embeds the new content");

    // Revert + commit -> original blob OID already materialized -> no re-embed.
    write_java(dir.path(), "Greeter.java", original);
    run_git(dir.path(), &["commit", "-aqm", "revert"]);
    let snapshot3 = Arc::new(snapshot2.update(&changed));
    indexer.request_update(snapshot3, changed);
    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    assert_eq!(
        embedder.texts_embedded(),
        after_edit,
        "reverting to a cached blob must reuse vectors, not re-embed"
    );
    indexer.close();
}

#[test]
fn run_gc_blocking_completes_and_is_repeatable() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    // The forced GC runs on the worker and replies on its own channel; the
    // active worktree's blobs stay live, so a follow-up query still resolves.
    indexer.run_gc_blocking().expect("gc completes");
    indexer.run_gc_blocking().expect("gc is repeatable");

    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 1,
        },
    )
    .expect("query after gc");
    assert_eq!(result.vector_ranked.len(), 1, "live blob survives gc");
    indexer.close();
}

/// Issue #1446: semantic materialization must dispatch every file through its
/// owning language delegate. A Java-dominant workspace containing a Rust file
/// once routed that file into the Java analyzer, which panicked indexing its
/// generation map with the foreign storage key. The workspace handle is now
/// always a MultiAnalyzer, so both files must come out the other end of the
/// production build path as chunks for their own language, with nothing
/// cross-dispatched, dropped, or panicking.
#[test]
fn mixed_language_workspace_materializes_chunks_for_both_languages() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "ConfigLoader.java",
        "public class ConfigLoader {\n  public String loadConfig(String path) { return path; }\n}\n",
    );
    std::fs::write(
        dir.path().join("manifest.rs"),
        "pub fn parseManifest(text: &str) -> usize {\n    text.len()\n}\n",
    )
    .unwrap();
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let status = indexer.status(&snapshot);
    assert_eq!(status.phase, "ready");
    assert_eq!(
        (status.materialized_files, status.materialize_total_files),
        (2, 2),
        "both language files materialize; neither is dropped as foreign"
    );

    // Each language's distinctive symbol must be retrievable, which proves that
    // language produced its own chunks and that they resolve as live.
    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "where are configuration and manifests handled".to_string(),
            k: 4,
        },
    )
    .expect("semantic_search succeeds");
    for symbol in ["loadConfig", "parseManifest"] {
        assert!(
            result
                .vector_ranked
                .iter()
                .any(|row| row.fqfn.contains(symbol)),
            "dense leg surfaces {symbol}: {:?}",
            result.vector_ranked
        );
    }
    indexer.close();
}

#[test]
fn semantic_search_caps_requested_k() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );

    indexer.wait_ready(Duration::from_secs(30)).unwrap();
    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: usize::MAX,
        },
    )
    .expect("oversized k is clamped before internal candidate math");
    assert_eq!(result.vector_ranked.len(), 1);
    indexer.close();
}

/// Issue #1929: the cache keeps the chunks of every blob it has ever seen, so
/// after an edit the old function is still stored under the old blob OID.
/// Retrieval no longer projects the cache through the worktree up front; it
/// checks liveness per hit instead. The stale symbol must therefore never come
/// back, and the symbol that replaced it must.
#[test]
fn an_edited_file_retrieves_the_new_symbol_and_never_the_cached_old_one() {
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greetTheStaleWay(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let file = snapshot
        .analyzer()
        .analyzed_files()
        .into_iter()
        .next()
        .expect("one analyzed file");
    let changed: BTreeSet<_> = [file].into_iter().collect();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greetTheLiveWay(String name) { return name; }\n}\n",
    );
    run_git(dir.path(), &["commit", "-aqm", "rename the greeting"]);
    let edited = Arc::new(snapshot.update(&changed));
    indexer.request_update(edited.clone(), changed);
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let result = semantic_search(
        &edited,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 5,
        },
    )
    .expect("query after the edit");

    let ranked: Vec<&str> = result
        .vector_ranked
        .iter()
        .map(|row| row.fqfn.as_str())
        .collect();
    assert!(
        ranked.iter().any(|fqfn| fqfn.contains("greetTheLiveWay")),
        "the live symbol must be retrievable: {ranked:?}"
    );
    assert!(
        !ranked.iter().any(|fqfn| fqfn.contains("greetTheStaleWay")),
        "the previous blob's chunk is still cached but must not be returned: {ranked:?}"
    );
    indexer.close();
}

/// The readiness contract the anvil hook polls: once `phase` is "ready" with no
/// pending batches, the very next query is answerable without a further wait.
#[test]
fn ready_status_means_the_first_query_is_answerable() {
    let dir = tempfile::tempdir().unwrap();
    write_java(
        dir.path(),
        "Greeter.java",
        "public class Greeter {\n  public String greet(String name) { return name; }\n}\n",
    );
    init_git(dir.path());
    let snapshot = snapshot_for(dir.path());
    let embedder = Arc::new(FakeHashEmbedder::new(16));
    let indexer = SemanticIndexer::start_with_provider(
        dir.path().to_path_buf(),
        snapshot.clone(),
        FakeEngineProvider { embedder },
    );
    indexer.wait_ready(Duration::from_secs(30)).unwrap();

    let status = indexer.status(&snapshot);
    assert_eq!(status.phase, "ready");
    assert_eq!(status.pending_batches, 0);

    let result = semantic_search(
        &snapshot,
        &indexer,
        SemanticSearchParams {
            query: "greet a user by name".to_string(),
            k: 1,
        },
    )
    .expect("a ready index answers immediately");
    assert!(result.notes.is_empty(), "notes: {:?}", result.notes);
    assert_eq!(result.vector_ranked.len(), 1);
    indexer.close();
}
