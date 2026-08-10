//! #1748 / #1774: the shared candidate-discovery engine used to charge one
//! `definition_candidates` store read per `use` statement in the workspace.
//!
//! `find_direct_importers_with_cancellation` visits every workspace file and
//! asks the language's import provider "could this file import the target".
//! Rust answered by resolving each `use` path to an fq name and asking the
//! store which files define it, so the cost was
//! O(workspace files x imports per file) point lookups inside one
//! `scan_usages` query -- 397k to 662k of them on the rustc tree.
//!
//! The fixture's shape is load-bearing in two ways. Half its files never
//! import the target at all, because that is the majority case on a real
//! workspace and it is the only case that pays for every one of its imports:
//! `could_import_file` is an `any(..)`, so a file that imports the target
//! stops at the first `use` that matches. The files that DO import the target
//! spell that import last, for the same reason. A fixture whose files all
//! import the target first charges nine lookups where this one charges
//! sixty-odd, and would have pinned nothing.
//!
//! The module graph is deliberately acyclic (everything imports one shared
//! `support` module, nothing imports a sibling). Cyclic module imports make
//! the Rust usage walks blow up superlinearly -- about 1 s at eight modules
//! with two neighbours each, over 600 s at twenty-four with four -- which is a
//! separate cost recorded in the v2 plan's Surprises, and would swamp what
//! these counters measure.

use brokk_bifrost::analyzer::CodeUnitIndex;

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::AnalyzerQueryScope;
use brokk_bifrost::path_utils::rel_path_string;
use brokk_bifrost::usages::{UsageFinder, UsageHitKind};
use brokk_bifrost::{
    AnalyzerConfig, AnalyzerDelegate, CancellationToken, CodeUnit, IAnalyzer, Language,
    MultiAnalyzer, ProjectFile, RustAnalyzer, WorkspaceAnalyzer,
};

const CALLER_COUNT: usize = 8;
const BYSTANDER_COUNT: usize = 8;
const IMPORTS_PER_FILE: usize = 4;

fn import_heavy_files(builder: InlineTestProject) -> InlineTestProject {
    let mut lib = String::from("pub mod target;\npub mod support;\n");
    for index in 0..CALLER_COUNT {
        lib.push_str(&format!("pub mod caller_{index};\n"));
    }
    for index in 0..BYSTANDER_COUNT {
        lib.push_str(&format!("pub mod bystander_{index};\n"));
    }

    let mut support = String::new();
    for index in 0..IMPORTS_PER_FILE {
        support.push_str(&format!("pub struct Helper{index};\n"));
    }

    let mut builder = builder
        .file(
            "Cargo.toml",
            "[package]\nname = \"importheavy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .file("src/lib.rs", lib)
        .file("src/support.rs", support)
        .file("src/target.rs", "pub fn collect_it() -> i32 {\n    1\n}\n");

    for index in 0..CALLER_COUNT {
        let mut caller = String::new();
        for helper in 0..(IMPORTS_PER_FILE - 1) {
            caller.push_str(&format!("use crate::support::Helper{helper};\n"));
        }
        // Last, so the `any(..)` short circuit cannot hide the other imports.
        caller.push_str("use crate::target::collect_it;\n");
        for helper in 0..(IMPORTS_PER_FILE - 1) {
            caller.push_str(&format!(
                "pub fn hold_{index}_{helper}() -> Helper{helper} {{ Helper{helper} }}\n"
            ));
        }
        caller.push_str(&format!(
            "pub fn call_{index}() -> i32 {{\n    collect_it()\n}}\n"
        ));
        builder = builder.file(format!("src/caller_{index}.rs"), caller);
    }

    for index in 0..BYSTANDER_COUNT {
        let mut bystander = String::new();
        for helper in 0..IMPORTS_PER_FILE {
            bystander.push_str(&format!("use crate::support::Helper{helper};\n"));
        }
        for helper in 0..IMPORTS_PER_FILE {
            bystander.push_str(&format!(
                "pub fn keep_{index}_{helper}() -> Helper{helper} {{ Helper{helper} }}\n"
            ));
        }
        builder = builder.file(format!("src/bystander_{index}.rs"), bystander);
    }

    builder
}

fn import_heavy_project() -> (crate::common::BuiltInlineTestProject, RustAnalyzer) {
    let project = import_heavy_files(InlineTestProject::with_language(Language::Rust)).build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

/// The same Rust fixture with a Python corner, so the workspace analyzer is a
/// `MultiAnalyzer` over two delegates rather than a single-language one.
///
/// The Python half imports something of its own: the merge layer groups the
/// candidate files by language and asks each delegate separately, so a second
/// group that answers nothing is what keeps the Rust group from being the
/// whole loop.
fn mixed_language_import_heavy_project() -> crate::common::BuiltInlineTestProject {
    import_heavy_files(InlineTestProject::new())
        .file("tools/support.py", "def helper():\n    return 1\n")
        .file(
            "tools/report.py",
            "from tools.support import helper\n\n\ndef report():\n    return helper()\n",
        )
        .build()
}

fn collect_it_target(analyzer: &dyn IAnalyzer, root: &std::path::Path) -> CodeUnit {
    let target_file = ProjectFile::new(root.to_path_buf(), "src/target.rs");
    analyzer
        .declarations(&target_file)
        .into_iter()
        .find(|unit| unit.identifier() == "collect_it")
        .expect("fixture declares collect_it")
}

#[test]
fn issue_1748_a_usage_query_resolves_import_targets_in_one_batched_read() {
    let (project, analyzer) = import_heavy_project();
    let target = collect_it_target(&analyzer, project.root());

    // Warm: the first query fills the cross-request caches this pin is not
    // about, so the second query measures steady-state candidate discovery.
    let _ = UsageFinder::new().query(&analyzer, std::slice::from_ref(&target), 1000, 1000);

    analyzer
        .test_hooks()
        .reset_definition_candidates_query_count_for_test();
    analyzer
        .test_hooks()
        .reset_definition_prefetch_batch_count_for_test();
    let query = UsageFinder::new().query(&analyzer, std::slice::from_ref(&target), 1000, 1000);
    let batches = analyzer
        .test_hooks()
        .definition_prefetch_batch_count_for_test();
    let point_lookups = analyzer
        .test_hooks()
        .definition_candidates_query_count_for_test();

    let hits = query.result.all_hits_including_imports();
    assert!(
        hits.iter()
            .any(|hit| hit.kind == UsageHitKind::Reference || hit.kind == UsageHitKind::Import),
        "the fixture must still resolve its usages: {hits:#?}"
    );

    assert_eq!(
        1, batches,
        "candidate discovery must resolve every import target in one batched read"
    );

    // Before the batch this query charged one point lookup per `use` statement
    // it inspected, which is the same shape that charged 397k-662k on the
    // rustc tree. The bound is a fraction of the import-statement count rather
    // than an exact figure because the graph phase after discovery
    // legitimately resolves a few names of its own.
    let import_statements = CALLER_COUNT * IMPORTS_PER_FILE + BYSTANDER_COUNT * IMPORTS_PER_FILE;
    assert!(
        point_lookups * 4 < import_statements,
        "a query must not charge a point lookup per import statement: \
         {point_lookups} lookups for {import_statements} imports"
    );
}

/// The batched answer must be the same set of usages the point lookups
/// produced: every caller found, and no bystander admitted.
#[test]
fn issue_1748_batched_discovery_finds_the_same_usages() {
    let (project, analyzer) = import_heavy_project();
    let target = collect_it_target(&analyzer, project.root());

    let query = UsageFinder::new().query(&analyzer, std::slice::from_ref(&target), 1000, 1000);
    let hit_files: std::collections::BTreeSet<String> = query
        .result
        .all_hits_including_imports()
        .iter()
        .map(|hit| rel_path_string(hit.enclosing.source()))
        .collect();

    for index in 0..CALLER_COUNT {
        let expected = format!("src/caller_{index}.rs");
        assert!(
            hit_files.iter().any(|file| file.ends_with(&expected)),
            "every caller must still be found: missing {expected} in {hit_files:?}"
        );
    }
    for index in 0..BYSTANDER_COUNT {
        let unexpected = format!("src/bystander_{index}.rs");
        assert!(
            !hit_files.iter().any(|file| file.ends_with(&unexpected)),
            "a file that never names the target must not become a hit: \
             {unexpected} in {hit_files:?}"
        );
    }
}

fn rust_delegate(multi: &MultiAnalyzer) -> &RustAnalyzer {
    match multi
        .delegates()
        .get(&Language::Rust)
        .expect("the mixed fixture must build a Rust delegate")
    {
        AnalyzerDelegate::Rust(analyzer) => analyzer,
        _ => panic!("the Rust language slot must hold a Rust analyzer"),
    }
}

fn mixed_language_workspace(project: &crate::common::BuiltInlineTestProject) -> Box<MultiAnalyzer> {
    assert_eq!(
        std::collections::BTreeSet::from([Language::Python, Language::Rust]),
        project.languages(),
        "the fixture must be a two-language workspace for this pin to mean anything"
    );
    match project.workspace_analyzer(AnalyzerConfig::default()) {
        WorkspaceAnalyzer::Multi(multi) => multi,
        WorkspaceAnalyzer::Empty(_) => {
            panic!("a two-language workspace must build a MultiAnalyzer")
        }
    }
}

/// #1748, second instance: the batch above never fired on a workspace with
/// more than one language, which is the shape the issue was opened about.
///
/// `find_direct_importers_with_cancellation` resolves its provider through
/// `analyzer.import_analysis_provider()`, which on a multi-language workspace
/// is the `MultiAnalyzer` itself. `MultiAnalyzer` overrode
/// `import_infos_for_files` but not `prefetch_import_targets`, so the trait's
/// no-op default answered and `RustAnalyzer`'s batch was unreachable -- zero
/// `prefetch_definitions` spans against 9,648 point `definition_candidates`
/// reads in one rustc gate cell, measured at `0086f1e5`.
///
/// The counters come off the Rust delegate, not off the merged analyzer:
/// `RustAnalyzer`'s `IAnalyzer` impl does not forward
/// `definition_candidates_query_count_for_test`, so `MultiAnalyzer`'s sum of
/// its delegates reports 0 for a Rust delegate no matter how many reads it
/// took. Reading the merged counter here would have made this pin vacuous.
///
/// This query is the fixture's first, deliberately. The batch's saving is in
/// the reads a cold request would otherwise take one at a time; a repeat query
/// against the same names is served from cross-request caches and charges
/// nothing on either side of the change, which measures neither.
///
/// Fail-before, with the `MultiAnalyzer::prefetch_import_targets` override
/// removed: 0 batches and 71 point lookups, more than one per import
/// statement in the fixture. After: 1 batch and 18.
#[test]
fn issue_1748_a_multi_language_workspace_gets_the_same_batched_read() {
    let project = mixed_language_import_heavy_project();
    let multi = mixed_language_workspace(&project);
    let analyzer: &dyn IAnalyzer = multi.as_ref();
    let target = collect_it_target(analyzer, project.root());

    rust_delegate(&multi)
        .test_hooks()
        .reset_definition_candidates_query_count_for_test();
    rust_delegate(&multi)
        .test_hooks()
        .reset_definition_prefetch_batch_count_for_test();
    let query = UsageFinder::new().query(analyzer, std::slice::from_ref(&target), 1000, 1000);
    let batches = rust_delegate(&multi)
        .test_hooks()
        .definition_prefetch_batch_count_for_test();
    let point_lookups = rust_delegate(&multi)
        .test_hooks()
        .definition_candidates_query_count_for_test();

    assert_eq!(
        1, batches,
        "candidate discovery must reach the language delegate's batch through the merge layer"
    );

    let import_statements = CALLER_COUNT * IMPORTS_PER_FILE + BYSTANDER_COUNT * IMPORTS_PER_FILE;
    assert!(
        point_lookups * 2 < import_statements,
        "a multi-language query must not charge a point lookup per import statement: \
         {point_lookups} lookups for {import_statements} imports"
    );

    // Parity with the single-language answer: routing through the merge layer
    // must find every caller and admit no bystander.
    let hit_files: std::collections::BTreeSet<String> = query
        .result
        .all_hits_including_imports()
        .iter()
        .map(|hit| rel_path_string(hit.enclosing.source()))
        .collect();
    for index in 0..CALLER_COUNT {
        let expected = format!("src/caller_{index}.rs");
        assert!(
            hit_files.iter().any(|file| file.ends_with(&expected)),
            "every caller must still be found: missing {expected} in {hit_files:?}"
        );
    }
    for index in 0..BYSTANDER_COUNT {
        let unexpected = format!("src/bystander_{index}.rs");
        assert!(
            !hit_files.iter().any(|file| file.ends_with(&unexpected)),
            "a file that never names the target must not become a hit: \
             {unexpected} in {hit_files:?}"
        );
    }
}

/// The prefix discipline `946710c4` established survives the delegation: a
/// prefetch issued on behalf of a request whose deadline is already gone
/// publishes nothing, so the name it did not read is not memoized as absent,
/// and the merge layer's own stopping point -- between language groups -- is
/// equally silent.
///
/// A property pin, not a fail-before one. Three layers refuse the batch on a
/// spent deadline (this override between groups, the delegate before it
/// resolves, `prefetch_definitions` before it reads) and any one of them is
/// enough, so no single removal turns this red -- removing all three still
/// leaves the resolver's per-file poll producing no names. What it does hold
/// is the property itself, through the layer this change adds: the delegation
/// must not become a way to issue that read, or to memoize its absence.
#[test]
fn issue_1748_a_stopped_multi_language_prefetch_memoizes_no_absence() {
    let project = mixed_language_import_heavy_project();
    let multi = mixed_language_workspace(&project);
    let analyzer: &dyn IAnalyzer = multi.as_ref();
    let target = collect_it_target(analyzer, project.root());
    let fq_name = target.fq_name();
    let files: Vec<ProjectFile> = analyzer.analyzed_files().into_iter().collect();
    let provider = analyzer
        .import_analysis_provider()
        .expect("a multi-language workspace exposes an import provider");

    let outer = AnalyzerQueryScope::new(analyzer);
    {
        let spent = CancellationToken::default();
        spent.cancel();
        let _inner = AnalyzerQueryScope::with_cancellation(analyzer, &spent);
        rust_delegate(&multi)
            .test_hooks()
            .reset_definition_prefetch_batch_count_for_test();
        provider.prefetch_import_targets(&files, None, &spent);
        assert_eq!(
            0,
            rust_delegate(&multi)
                .test_hooks()
                .definition_prefetch_batch_count_for_test(),
            "a prefetch past the request's deadline must not issue its batched read"
        );
    }

    let answered: Vec<_> = analyzer.definitions(&fq_name).collect();
    assert!(
        !answered.is_empty(),
        "the stopped prefetch must not have memoized proven absence for {fq_name}"
    );
    drop(outer);
}

/// #1809 (third instance): a read taken on behalf of a scan whose budget is
/// already gone is the scan's deadline overshoot.
///
/// Candidate discovery polls its own loops -- once per candidate file in the
/// importer walk, once per overload in the finder -- and that was not enough.
/// The walk's per-candidate question is answered by `definitions(import
/// target)`, and on the rustc tree one of those reads is the longest single
/// thing the request does: `main` is 22k rows and 1.14 s. Run 10 measured the
/// consequence exactly. The scan budget window was 3.67 s against a 3.00 s
/// budget, `usages::candidate_discovery` owned 97.8 % of it, and the last span
/// before the window closed was one `sql_definition_candidates.rows[main]` of
/// 1,141.9 ms. The loops stopped on time; the read they had already asked for
/// did not.
///
/// So the request's deadline now reaches the reads, through the query scope
/// the scan already opens. Two halves, both pinned here: a read is not started
/// once the deadline has passed, and the nothing it returns is not memoized as
/// this name's answer for the rest of the request.
///
/// Before the fix the inner read runs (`row_reads` is 1), returns the real
/// declarations, and memoizes them -- so the first two assertions fail.
#[test]
fn issue_1809_a_read_past_the_deadline_is_neither_taken_nor_memoized() {
    let (project, analyzer) = import_heavy_project();
    let target = collect_it_target(&analyzer, project.root());
    let fq_name = target.fq_name();

    // The outer scope is the request; the inner one is the same request under
    // a spent budget. Nesting is what makes "not memoized" observable: the
    // request memos survive the inner scope and are read again after it.
    let outer = AnalyzerQueryScope::new(&analyzer);
    {
        let spent = CancellationToken::default();
        spent.cancel();
        let _inner = AnalyzerQueryScope::with_cancellation(&analyzer, &spent);
        analyzer
            .test_hooks()
            .reset_definition_candidate_row_read_count_for_test();
        let stopped: Vec<_> = analyzer.definitions(&fq_name).collect();
        assert_eq!(
            0,
            analyzer
                .test_hooks()
                .definition_candidate_row_read_count_for_test(),
            "a candidate-row read must not start once the request's deadline has passed"
        );
        assert!(
            stopped.is_empty(),
            "a stopped read has no answer to give: {stopped:#?}"
        );
    }

    let answered: Vec<_> = analyzer.definitions(&fq_name).collect();
    assert!(
        !answered.is_empty(),
        "the stopped read must not have memoized proven absence for {fq_name}"
    );
    drop(outer);
}

/// The completing path is unchanged: a deadline that never expires produces
/// the same query result, candidate set and hits as no deadline at all.
///
/// This is the parity half of the change. Every scan now runs its reads under
/// a token, so the polling code path is the *only* path a real query takes;
/// this pins that taking it changes nothing about the answer.
#[test]
fn issue_1809_a_deadline_that_never_expires_does_not_change_the_answer() {
    let (project, analyzer) = import_heavy_project();
    let target = collect_it_target(&analyzer, project.root());

    let plain = UsageFinder::new().query(&analyzer, std::slice::from_ref(&target), 1000, 1000);
    let deadlined = UsageFinder::new()
        .with_cancellation(
            CancellationToken::default().with_timeout(std::time::Duration::from_secs(600)),
        )
        .query(&analyzer, std::slice::from_ref(&target), 1000, 1000);

    assert_eq!(plain.completion, deadlined.completion);
    assert_eq!(
        sorted_paths(&plain.candidate_files),
        sorted_paths(&deadlined.candidate_files),
        "a deadline the query never reaches must not change candidate discovery"
    );
    assert_eq!(
        hit_summary(&plain),
        hit_summary(&deadlined),
        "a deadline the query never reaches must not change the usages found"
    );
    assert!(
        !hit_summary(&plain).is_empty(),
        "the fixture must produce hits for this comparison to mean anything"
    );
}

/// The wiring, pinned separately from the mechanism: the scan's own token has
/// to reach the request boundary, or nothing below it can see the deadline.
///
/// The provider spends the budget and then issues exactly the read candidate
/// discovery issues at that point -- `definitions` for a name it has not read
/// yet. With `UsageFinder` opening its query scope without the token (the
/// shape before this change) that read is taken and the counter is 1.
#[test]
fn issue_1809_a_scan_passes_its_own_deadline_to_the_reads_below_it() {
    struct CancelThenRead {
        cancellation: CancellationToken,
        read_name: String,
    }

    impl brokk_bifrost::usages::CandidateFileProvider for CancelThenRead {
        fn find_candidates(
            &self,
            _target: &brokk_bifrost::CodeUnit,
            analyzer: &dyn IAnalyzer,
        ) -> brokk_bifrost::hash::HashSet<ProjectFile> {
            self.cancellation.cancel();
            let _ = analyzer.definitions(&self.read_name).count();
            brokk_bifrost::hash::HashSet::default()
        }
    }

    let (project, analyzer) = import_heavy_project();
    let target = collect_it_target(&analyzer, project.root());
    let cancellation = CancellationToken::default();
    let provider = CancelThenRead {
        cancellation: cancellation.clone(),
        read_name: target.fq_name(),
    };

    analyzer
        .test_hooks()
        .reset_definition_candidate_row_read_count_for_test();
    let query = UsageFinder::new()
        .with_cancellation(cancellation)
        .query_with_provider(
            &analyzer,
            std::slice::from_ref(&target),
            Some(&provider),
            1000,
            1000,
        );

    assert_eq!(
        0,
        analyzer
            .test_hooks()
            .definition_candidate_row_read_count_for_test(),
        "a read issued after the scan's budget expired must not reach the store"
    );
    assert_eq!(
        brokk_bifrost::usages::UsageQueryCompletion::Cancelled,
        query.completion,
        "and the scan must still report the budget, not an empty success"
    );
}

fn sorted_paths(
    files: &std::collections::HashSet<ProjectFile, impl std::hash::BuildHasher>,
) -> Vec<String> {
    let mut paths: Vec<String> = files.iter().map(|file| file.to_string()).collect();
    paths.sort();
    paths
}

fn hit_summary(query: &brokk_bifrost::usages::QueryResult) -> Vec<String> {
    let mut hits: Vec<String> = query
        .result
        .all_hits_including_imports()
        .iter()
        .map(|hit| {
            format!(
                "{}:{:?}:{}",
                hit.enclosing.source(),
                hit.kind,
                hit.enclosing.fq_name()
            )
        })
        .collect();
    hits.sort();
    hits
}
