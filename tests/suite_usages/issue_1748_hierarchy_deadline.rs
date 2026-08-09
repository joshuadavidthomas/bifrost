//! #1748: `scan_usages_by_reference` used to lose its own deadline at the type
//! hierarchy, and to build the half of the hierarchy index the request had
//! already excluded.
//!
//! The measured incident: a C++ workspace, `max_duration_secs: 30`,
//! `include_tests: false`. The call ran about 1,200 s and then died on the MCP
//! host's request budget with a transport error and zero results. Inside that
//! window, 44,971 `cpp.visible_types.build` spans over 23,216 distinct files --
//! the whole-workspace descendant-index build, iterating one class at a time
//! with no poll of the token the request was carrying, and 52.3% of those
//! builds under a test directory the answer would have discarded.
//!
//! Two things are pinned here, both through `CppAnalyzer`'s per-file
//! include-closure build counter, because that counter is the exact unit the
//! trace measured.
//!
//! Fix A: the build stops at the deadline, publishes nothing when it does, and
//! a later ask still gets the whole correct answer.
//!
//! Fix B: an `include_tests: false` request builds an index that was never
//! walked over the test classes at all, and asking the *same* analyzer with
//! test files included still answers with them -- the two index variants do not
//! poison each other.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::{
    CodeUnitIndex, DescendantIndexScope, IAnalyzer, TypeHierarchyProvider,
};
use brokk_bifrost::searchtools::{
    ScanUsagesByReferenceParams, ScanUsagesIncompleteReason, ScanUsagesResult,
    scan_usages_by_reference, scan_usages_by_reference_with_cancellation,
};
use brokk_bifrost::{CancellationToken, CodeUnit, CppAnalyzer, Language, ProjectFile};

/// Enough classes that a build which stops early is unmistakably short of a
/// build that finishes. Each lives in its own header, so one class is one
/// include-closure build, which is what the counter records.
const DERIVED_COUNT: usize = 60;

/// Test-directory subclasses in the Fix B fixture. Smaller than
/// `DERIVED_COUNT`, and a different number, so an assertion cannot pass by
/// confusing the two populations.
const TEST_DERIVED_COUNT: usize = 20;

/// A deep-ish include chain under every class header. Without it a closure walk
/// is a single pop and the per-pop poll would have nothing to bite on.
const CHAIN_DEPTH: usize = 4;

fn chain_headers(mut builder: InlineTestProject) -> InlineTestProject {
    // `chain_0.h` is the tail; each link includes the next one down, and
    // `base.h` includes the head, so every derived header's closure is
    // itself + base.h + CHAIN_DEPTH links.
    for depth in 0..CHAIN_DEPTH {
        let include = if depth == 0 {
            String::new()
        } else {
            format!("#include \"chain_{}.h\"\n", depth - 1)
        };
        builder = builder.file(
            format!("include/chain_{depth}.h"),
            format!("#pragma once\n{include}struct Chain{depth} {{\n  int value_{depth};\n}};\n"),
        );
    }
    builder
}

fn derived_header(index: usize, class_prefix: &str) -> String {
    format!(
        "#pragma once\n#include \"base.h\"\nclass {class_prefix}{index:02} : public Base {{\npublic:\n  int run();\n}};\n"
    )
}

/// A workspace whose only interesting structure is one base class with many
/// subclasses, each in its own header, each header pulling in a short include
/// chain.
fn hierarchy_project(test_subclasses: bool) -> crate::common::BuiltInlineTestProject {
    let mut builder = chain_headers(InlineTestProject::with_language(Language::Cpp))
        .file(
            "include/base.h",
            format!(
                "#pragma once\n#include \"chain_{}.h\"\nclass Base {{\npublic:\n  int run();\n}};\n",
                CHAIN_DEPTH - 1
            ),
        )
        .file("src/base.cpp", "#include \"base.h\"\nint Base::run() {\n  return 0;\n}\n")
        .file(
            "src/caller.cpp",
            "#include \"base.h\"\nint call_it(Base* value) {\n  return value->run();\n}\n",
        );

    for index in 0..DERIVED_COUNT {
        builder = builder
            .file(
                format!("include/derived_{index:02}.h"),
                derived_header(index, "Derived"),
            )
            .file(
                format!("src/derived_{index:02}.cpp"),
                format!(
                    "#include \"derived_{index:02}.h\"\nint Derived{index:02}::run() {{\n  return {index};\n}}\n"
                ),
            );
    }

    if test_subclasses {
        // A call site under `tests/`, so "did the scan answer from a test
        // file" is a question the fixture can actually answer. The subclass
        // headers alone only *declare* overrides; they never call `Base::run`.
        builder = builder.file(
            "tests/src/test_caller.cpp",
            "#include \"base.h\"\nint test_call_it(Base* value) {\n  return value->run();\n}\n",
        );
        for index in 0..TEST_DERIVED_COUNT {
            builder = builder
                .file(
                    format!("tests/include/test_derived_{index:02}.h"),
                    derived_header(index, "TestDerived"),
                )
                .file(
                    format!("tests/src/test_derived_{index:02}.cpp"),
                    format!(
                        "#include \"test_derived_{index:02}.h\"\nint TestDerived{index:02}::run() {{\n  return {index};\n}}\n"
                    ),
                );
        }
    }

    builder.build()
}

fn base_class(analyzer: &CppAnalyzer, root: &std::path::Path) -> CodeUnit {
    let header = ProjectFile::new(root.to_path_buf(), "include/base.h");
    analyzer
        .declarations(&header)
        .into_iter()
        .find(|unit| unit.is_class() && unit.identifier() == "Base")
        .expect("fixture declares class Base")
}

fn descendant_identifiers(
    units: impl IntoIterator<Item = CodeUnit>,
) -> std::collections::BTreeSet<String> {
    units
        .into_iter()
        .map(|unit| unit.identifier().to_string())
        .collect()
}

/// Fix A, the whole claim reduced to a counter.
///
/// The token stops after a fixed number of `is_cancelled()` checks, which makes
/// this deterministic without reading a clock: nothing else on this code path
/// polls the token, so every check is one the descendant-index build made. The
/// budget is set well below what a complete build needs (one poll per candidate
/// plus one per file popped from each candidate's include closure, so several
/// per class over `DERIVED_COUNT` + 1 classes) and well above what it takes to
/// get started.
///
/// Fail-before: with the token plumbed all the way down but the poll inside
/// `build_cpp_visible_type_units` removed, the build counter reaches the full
/// class count and the third assertion below fails.
#[test]
fn issue_1748_a_cpp_descendant_index_build_stops_at_the_scan_deadline() {
    let project = hierarchy_project(false);
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let base = base_class(&analyzer, project.root());

    let stopping = CancellationToken::timeout_after_checks_for_test(40);
    analyzer.reset_visible_type_units_build_count_for_test();
    let stopped =
        analyzer.get_descendants_within(&base, &DescendantIndexScope::whole_workspace(&stopping));
    let stopped_builds = analyzer.visible_type_units_build_count_for_test();

    assert!(
        stopped.is_none(),
        "a build that ran out of budget must report that, not a partial subtype set"
    );
    assert!(
        stopping.is_timed_out(),
        "the fixture's budget must expire as a timeout, so the scan reports time_budget"
    );
    assert!(
        stopped_builds < DERIVED_COUNT / 2,
        "the build must stop well short of walking every class: {stopped_builds} include-closure \
         builds for {DERIVED_COUNT} subclasses"
    );

    // Nothing was published, so a later caller gets a complete answer rather
    // than the truncated index the stopped build had assembled.
    let uncancelled = CancellationToken::default();
    let complete = analyzer
        .get_descendants_within(&base, &DescendantIndexScope::whole_workspace(&uncancelled))
        .expect("an uncancelled build completes");
    let identifiers = descendant_identifiers(complete);
    assert_eq!(
        DERIVED_COUNT,
        identifiers.len(),
        "the rebuilt index must hold every subclass: {identifiers:?}"
    );
    for index in 0..DERIVED_COUNT {
        let expected = format!("Derived{index:02}");
        assert!(
            identifiers.contains(&expected),
            "missing {expected} from the rebuilt index: {identifiers:?}"
        );
    }
}

/// The same stop, seen from the tool surface a user calls.
///
/// This is the assertion the incident was about: the request carries a budget,
/// the budget expires inside candidate discovery, and the entry says so instead
/// of running until something else kills it. The check budget is generous
/// because resolution and the graph phase poll the same token; what matters is
/// that the scan terminates with a `time_budget` verdict rather than a complete
/// one.
#[test]
fn issue_1748_a_scan_reports_time_budget_when_the_hierarchy_build_runs_out() {
    let project = hierarchy_project(false);
    let analyzer = CppAnalyzer::from_project(project.project().clone());

    let stopping = CancellationToken::timeout_after_checks_for_test(60);
    let result = scan_usages_by_reference_with_cancellation(
        &analyzer as &dyn IAnalyzer,
        scan_params(true),
        stopping.clone(),
    );

    assert!(
        stopping.is_timed_out(),
        "the request's budget must be what stopped the scan"
    );
    let reasons: Vec<Option<ScanUsagesIncompleteReason>> = result
        .results
        .iter()
        .map(|entry| entry.incomplete_reason)
        .collect();
    assert!(
        reasons.contains(&Some(ScanUsagesIncompleteReason::TimeBudget)),
        "an out-of-budget scan must report time_budget: {reasons:?}"
    );
}

/// Fix B: the excluded half is never built.
///
/// `include_tests: false` is decidable from the path before any include
/// closure is walked, so the classes under `tests/` must not reach
/// `get_direct_ancestors` at all. The counter difference is the pin: the
/// production-only build charges one include-closure build per production
/// class header, and none for the test ones.
///
/// Fail-before: with `DescendantIndexScope::admits` hardwired to `true`, the
/// production-only build charges the test headers too and the second assertion
/// fails.
#[test]
fn issue_1748_b_excluding_tests_never_builds_the_test_classes() {
    let project = hierarchy_project(true);
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let base = base_class(&analyzer, project.root());
    let excludes_tests = |file: &ProjectFile| file.to_string().contains("tests");
    let uncancelled = CancellationToken::default();

    analyzer.reset_visible_type_units_build_count_for_test();
    let production_only = analyzer
        .get_descendants_within(
            &base,
            &DescendantIndexScope::excluding_sources(&uncancelled, &excludes_tests),
        )
        .expect("an uncancelled build completes");
    let production_builds = analyzer.visible_type_units_build_count_for_test();

    let identifiers = descendant_identifiers(production_only);
    assert_eq!(
        DERIVED_COUNT,
        identifiers.len(),
        "the production-only index must still hold every production subclass: {identifiers:?}"
    );
    assert!(
        identifiers.iter().all(|name| name.starts_with("Derived")),
        "a production-only index must contain no test subclass: {identifiers:?}"
    );
    // One build per production class header (the `Base` header, the chain
    // headers reached through it, and the production subclass headers) and not
    // one for any of the `TEST_DERIVED_COUNT` test headers.
    assert!(
        production_builds <= DERIVED_COUNT + CHAIN_DEPTH + 1,
        "the production-only build must not walk the excluded classes: {production_builds} \
         include-closure builds with {TEST_DERIVED_COUNT} test headers excluded"
    );
}

/// The two variants are separate indexes, not one index that the first caller
/// gets to define.
///
/// A `KeyedPoolSafeMemo` keyed by the wrong thing would serve the
/// production-only index to a later `include_tests: true` request, and the test
/// subclasses would silently vanish for everyone until the analyzer was
/// rebuilt.
#[test]
fn issue_1748_b_including_tests_still_sees_test_descendants_on_the_same_analyzer() {
    let project = hierarchy_project(true);
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let base = base_class(&analyzer, project.root());
    let excludes_tests = |file: &ProjectFile| file.to_string().contains("tests");
    let uncancelled = CancellationToken::default();

    // Production-only first, so the shared cell for that variant is warm.
    let _ = analyzer
        .get_descendants_within(
            &base,
            &DescendantIndexScope::excluding_sources(&uncancelled, &excludes_tests),
        )
        .expect("an uncancelled build completes");

    let everything = analyzer
        .get_descendants_within(&base, &DescendantIndexScope::whole_workspace(&uncancelled))
        .expect("an uncancelled build completes");
    let identifiers = descendant_identifiers(everything);

    assert_eq!(
        DERIVED_COUNT + TEST_DERIVED_COUNT,
        identifiers.len(),
        "the whole-workspace index must hold both populations: {identifiers:?}"
    );
    for index in 0..TEST_DERIVED_COUNT {
        let expected = format!("TestDerived{index:02}");
        assert!(
            identifiers.contains(&expected),
            "the production-only variant must not have poisoned the whole-workspace one: \
             missing {expected} from {identifiers:?}"
        );
    }
}

/// The push-down does not change what a scan answers.
///
/// Fix B moves an exclusion earlier, and the finder's own `retain` stays as the
/// correctness backstop. Both spellings of the request must produce the same
/// verdict about test files that they did before the index knew anything about
/// them.
#[test]
fn issue_1748_b_test_exclusion_still_decides_which_files_a_scan_answers_from() {
    let project = hierarchy_project(true);
    let analyzer = CppAnalyzer::from_project(project.project().clone());

    let production_only = scan_usages_by_reference(&analyzer as &dyn IAnalyzer, scan_params(false));
    let with_tests = scan_usages_by_reference(&analyzer as &dyn IAnalyzer, scan_params(true));

    let production_files = scanned_files(&production_only);
    let all_files = scanned_files(&with_tests);
    assert!(
        production_files.iter().all(|file| !file.contains("tests/")),
        "an include_tests:false scan must answer from no test file: {production_files:?}"
    );
    assert!(
        all_files.iter().any(|file| file.contains("tests/")),
        "an include_tests:true scan on the same analyzer must still reach the test files: \
         {all_files:?}"
    );
}

fn scan_params(include_tests: bool) -> ScanUsagesByReferenceParams {
    ScanUsagesByReferenceParams {
        symbols: vec!["Base.run".to_string()],
        include_tests,
        paths: None,
        include_same_owner: false,
        // Generous, so a slow box cannot turn a completeness assertion into a
        // budget one; the deadline tests above set their own stop.
        max_duration_secs: Some(120),
    }
}

fn scanned_files(result: &ScanUsagesResult) -> Vec<String> {
    result
        .results
        .iter()
        .flat_map(|entry| entry.files.iter())
        .map(|group| group.path.clone())
        .collect()
}
