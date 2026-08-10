//! Issue #1908: the C++ reconcile candidate storm.
//!
//! Measured incident (ccx-incident-108, llvm+clang, warm cache): one
//! `get_symbol_sources` request ran 270 s and one `get_summaries` request ran
//! 73 s, both on a single bare identifier that arrived as a semantic
//! `context_fetch` hit name. A third request never returned before the
//! container was killed. 11.0M `cpp.reconcile.candidate` spans in the window.
//!
//! The mechanism is not the #1566 include-closure fan-out. It is a quadratic
//! caller of a per-identifier computation:
//!
//! 1. A bare single-identifier symbol resolves through the fuzzy resolver,
//!    which `get_symbol_sources` and `get_summaries` called `unbounded()`.
//! 2. `resolution_from_matches` then re-expands every deduplicated fq key by
//!    calling `definitions(fq)` once per key.
//! 3. On C++ each of those calls ran `cpp_reconciled_definitions`, which
//!    repeated the identical identifier-index store read and re-scanned the
//!    identical candidate set, because its memo was keyed by the *queried fq
//!    name* -- and all K keys are distinct by construction.
//!
//! K keys x N candidates, with K ~= N. Measured for the identifier `g`:
//! 1,277 keys x 2,898 candidates = 3.70M candidate evaluations, plus 1,277
//! repetitions of one 57 ms store read.
//!
//! Three fixes are pinned here.
//!
//! Fix A -- the fan-out gate. Both tools now resolve under a real
//! `FuzzyResolveBudget`; an over-cap bare identifier reports its true count
//! through `too_broad` and expands nothing.
//!
//! Fix B -- the K x N. The reconcile memo is keyed by member identifier and
//! owner terminal instead of by queried fq name, so K queries that share one
//! identifier share one candidate scan.
//!
//! Fix D -- the deadline. The request token reaches the fuzzy resolver's
//! per-key poll and the reconcile candidate loop, and a cancelled partial
//! reconcile is never published to the memo.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::CodeUnitIndex;
use brokk_bifrost::searchtools::{
    SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES, SummariesParams, SymbolLookupParams, TooBroadMatch,
    get_summaries, get_symbol_sources, get_symbol_sources_with_source_budget,
};
use brokk_bifrost::{CancellationToken, CppAnalyzer, Language};

/// One header holding `count` classes, each in its own namespace, each
/// declaring the same member identifier.
///
/// One file, not `count` files: the cap this exercises is on resolution
/// fan-out, and a two-hundred-file workspace would measure analysis instead
/// (the #1839 fixture makes the same choice).
fn namesake_header(count: usize) -> String {
    let mut header = String::new();
    for index in 0..count {
        header.push_str(&format!(
            "namespace n{index} {{\nclass C{index} {{\n public:\n  int g() const;\n}};\n}}\n"
        ));
    }
    header
}

fn namesake_analyzer(count: usize) -> (crate::common::BuiltInlineTestProject, CppAnalyzer) {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("namesakes.h", namesake_header(count))
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

#[test]
fn issue_1908_get_symbol_sources_reports_an_over_cap_bare_identifier_instead_of_expanding_it() {
    let over = SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES + 10;
    let (_project, analyzer) = namesake_analyzer(over);

    let result = get_symbol_sources(
        &analyzer,
        SymbolLookupParams {
            symbols: vec!["g".to_string()],
        },
    );

    assert_eq!(
        1,
        result.too_broad.len(),
        "an over-cap bare identifier is a too-broad selector: {result:#?}"
    );
    let too_broad = &result.too_broad[0];
    assert_eq!("g", too_broad.target);
    assert_eq!(TooBroadMatch::Declarations, too_broad.matched_kind);
    assert_eq!(
        over, too_broad.matched,
        "the reported count must be the true match count, not the cap"
    );
    assert_eq!(SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES, too_broad.cap);
    // Skipped, not truncated. Producing the list is the work the cap exists to
    // decline, so there can be no sample and no source blocks.
    assert!(too_broad.sample.is_empty(), "{too_broad:#?}");
    assert!(result.sources.is_empty(), "{result:#?}");
    assert!(result.ambiguous.is_empty(), "{result:#?}");
}

#[test]
fn issue_1908_get_summaries_reports_an_over_cap_bare_identifier_instead_of_expanding_it() {
    let over = SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES + 10;
    let (_project, analyzer) = namesake_analyzer(over);

    let result = get_summaries(
        &analyzer,
        SummariesParams {
            targets: vec!["g".to_string()],
        },
    );

    assert_eq!(1, result.too_broad.len(), "{result:#?}");
    let too_broad = &result.too_broad[0];
    assert_eq!("g", too_broad.target);
    assert_eq!(TooBroadMatch::Declarations, too_broad.matched_kind);
    assert_eq!(over, too_broad.matched);
    assert_eq!(SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES, too_broad.cap);
    assert!(result.summaries.is_empty(), "{result:#?}");
    assert!(result.ambiguous.is_empty(), "{result:#?}");
}

#[test]
fn issue_1908_an_under_cap_bare_identifier_is_still_answered_in_full() {
    let under = 5;
    let (_project, analyzer) = namesake_analyzer(under);

    let result = get_symbol_sources(
        &analyzer,
        SymbolLookupParams {
            symbols: vec!["g".to_string()],
        },
    );

    assert!(
        result.too_broad.is_empty(),
        "an under-cap identifier must not trip the gate: {result:#?}"
    );
    assert_eq!(
        1,
        result.ambiguous.len(),
        "five namesakes are ambiguous, not too broad: {result:#?}"
    );
    assert_eq!(
        under,
        result.ambiguous[0].matches.len(),
        "every candidate must still be offered: {result:#?}"
    );
}

/// Fix B's fixture: `count` classes that each declare `shared`, each in its own
/// namespace and its own header, plus one `.cpp` per class holding the
/// out-of-line definition under a `using namespace`.
///
/// The `using namespace` is what makes reconciliation do real work: extraction
/// records the definition's provisional identity as `Outer$Inner.shared` with
/// no namespace, and only the include-visible class table can re-key it onto
/// `ns.Outer$Inner.shared`. That is the #1121/#1134 shape the whole reconcile
/// path exists for, so each class contributes one genuine re-keying and the
/// candidate set for the identifier is `2 * count` declarations over `count`
/// distinct canonical names -- the storm's K x N shape in miniature.
///
/// `inner_name` decides whether those `count` owners share one terminal
/// segment. Both arrangements occur in real code and they exercise different
/// halves of the memo key.
fn divergent_namesakes(
    count: usize,
    inner_name: &dyn Fn(usize) -> String,
) -> (crate::common::BuiltInlineTestProject, CppAnalyzer) {
    let mut builder = InlineTestProject::with_language(Language::Cpp);
    for index in 0..count {
        let inner = inner_name(index);
        builder = builder.file(
            format!("h{index}.h"),
            format!(
                "namespace ns{index} {{\nclass Outer{index} {{\n public:\n  class {inner} {{ public: int shared() const; }};\n}};\n}}\n"
            ),
        );
        builder = builder.file(
            format!("c{index}.cpp"),
            format!(
                "#include \"h{index}.h\"\nusing namespace ns{index};\nint Outer{index}::{inner}::shared() const {{ return {index}; }}\n"
            ),
        );
    }
    let project = builder.build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

/// Every owner nests its member under the same terminal segment, so all
/// `count` canonical names fall in one reconcile group.
fn one_owner_terminal(_index: usize) -> String {
    "Inner".to_string()
}

/// Every owner has its own terminal segment, so #1566's pre-filter can narrow
/// a query to that owner's own two declarations.
fn distinct_owner_terminals(index: usize) -> String {
    format!("Inner{index}")
}

const DIVERGENT_COUNT: usize = 12;

/// Every canonical name in the fixture, in fixture order.
fn divergent_canonical_names(inner_name: &dyn Fn(usize) -> String) -> Vec<String> {
    (0..DIVERGENT_COUNT)
        .map(|index| format!("ns{index}.Outer{index}${}.shared", inner_name(index)))
        .collect()
}

fn resolved_sources(analyzer: &CppAnalyzer, fq_name: &str) -> Vec<(String, String)> {
    let mut sources: Vec<_> = analyzer
        .definitions(fq_name)
        .map(|unit| {
            (
                unit.fq_name(),
                unit.source().rel_path().to_string_lossy().to_string(),
            )
        })
        .collect();
    sources.sort();
    sources
}

#[test]
fn issue_1908_resolving_every_namesake_costs_one_candidate_scan_not_one_per_key() {
    let (_project, analyzer) = divergent_namesakes(DIVERGENT_COUNT, &one_owner_terminal);
    let names = divergent_canonical_names(&one_owner_terminal);
    // Warm the store-backed declaration reads so the counters measure
    // reconciliation only.
    analyzer.get_all_declarations();
    analyzer.reset_reconcile_counts_for_test();

    for name in &names {
        let definitions: Vec<_> = analyzer.definitions(name).collect();
        assert!(
            definitions.iter().any(|unit| unit
                .source()
                .rel_path()
                .to_string_lossy()
                .ends_with(".cpp")),
            "the out-of-line definition must reconcile onto {name}: {definitions:?}"
        );
    }

    assert_eq!(
        1,
        analyzer.reconcile_candidate_scan_count_for_test(),
        "K queries sharing one member identifier share one identifier-index scan"
    );
    // These K names share both halves of the memo key, so they share one group
    // build over the whole 2K-declaration candidate set. Before #1908 each of
    // the K queries rescanned that set on its own: K x N.
    assert_eq!(
        2 * DIVERGENT_COUNT,
        analyzer.reconcile_candidate_evaluation_count_for_test(),
        "candidate evaluations must be the candidate set, not K times it"
    );
}

#[test]
fn issue_1908_regrouping_reconciliation_answers_exactly_what_per_fq_reconciliation_did() {
    for inner_name in [
        &one_owner_terminal as &dyn Fn(usize) -> String,
        &distinct_owner_terminals,
    ] {
        let (_project, analyzer) = divergent_namesakes(DIVERGENT_COUNT, inner_name);

        for (index, name) in divergent_canonical_names(inner_name)
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                vec![
                    (name.clone(), format!("c{index}.cpp")),
                    (name.clone(), format!("h{index}.h")),
                ],
                resolved_sources(&analyzer, &name),
                "the header declaration and its out-of-line definition must both \
                 answer {name}, and nothing else may"
            );
        }
    }
}

#[test]
fn issue_1908_reconciliation_still_skips_a_same_named_member_of_an_unrelated_owner() {
    // The reason the memo key keeps the owner terminal instead of being a plain
    // per-identifier map: dropping it would reconcile every same-named
    // candidate in the workspace on the first query for the identifier, which
    // is the cost #1566 removed (chromium paid ~75 s per member query that
    // way). `reconcile_skips_same_named_members_of_unrelated_classes_1566`
    // guards the class-table half in-crate; this guards the
    // candidate-evaluation half.
    let (_project, analyzer) = divergent_namesakes(DIVERGENT_COUNT, &distinct_owner_terminals);
    analyzer.get_all_declarations();
    analyzer.reset_reconcile_counts_for_test();

    let name = format!("ns0.Outer0${}.shared", distinct_owner_terminals(0));
    assert_eq!(
        vec![
            (name.clone(), "c0.cpp".to_string()),
            (name.clone(), "h0.h".to_string()),
        ],
        resolved_sources(&analyzer, &name)
    );
    assert_eq!(
        2,
        analyzer.reconcile_candidate_evaluation_count_for_test(),
        "only the queried owner's own two declarations may be evaluated, not \
         all {} candidates",
        2 * DIVERGENT_COUNT
    );
}

/// Fix D: how many checks the token allows before it reports a timeout.
///
/// Enough that the request reaches reconciliation and evaluates some
/// candidates, few enough that it cannot finish all `2 * DIVERGENT_COUNT` of
/// them. The exact number is not load-bearing -- the assertions are "fewer
/// than all of them" and "the later call is still whole".
const CHECKS_BEFORE_TIMEOUT: usize = 12;

#[test]
fn issue_1908_a_cancelled_request_stops_the_reconcile_scan_and_publishes_nothing() {
    let (_project, analyzer) = divergent_namesakes(DIVERGENT_COUNT, &one_owner_terminal);
    let names = divergent_canonical_names(&one_owner_terminal);
    analyzer.get_all_declarations();
    analyzer.reset_reconcile_counts_for_test();

    let stopping = CancellationToken::timeout_after_checks_for_test(CHECKS_BEFORE_TIMEOUT);
    let stopped = get_symbol_sources_with_source_budget(
        &analyzer,
        SymbolLookupParams {
            symbols: vec![names[0].clone()],
        },
        usize::MAX,
        Some(&stopping),
    )
    .expect("the source byte budget is unbounded here");
    // Not a fixture assertion. This token only reports a timeout once something
    // has asked it, so it is the direct pin that the request polls its deadline
    // at all -- before fix D nothing on this path ever did, which is why
    // request 179 in the incident emitted spans until the container died.
    assert!(
        stopping.is_cancelled(),
        "the request must poll its own deadline: {stopped:#?}"
    );

    let stopped_evaluations = analyzer.reconcile_candidate_evaluation_count_for_test();
    assert!(
        stopped_evaluations < 2 * DIVERGENT_COUNT,
        "a cancelled request must stop the candidate scan short of the whole \
         candidate set: evaluated {stopped_evaluations} of {}",
        2 * DIVERGENT_COUNT
    );

    // Nothing partial may have been published. Every canonical name must still
    // resolve to both its declaration and its out-of-line definition, which is
    // only true if the stopped build was discarded and rebuilt whole.
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            vec![
                (name.clone(), format!("c{index}.cpp")),
                (name.clone(), format!("h{index}.h")),
            ],
            resolved_sources(&analyzer, name),
            "a later call must get the full answer, not the truncated memo"
        );
    }
}

#[test]
fn issue_1908_an_uncancelled_request_still_answers_the_same_name_in_full() {
    // The control for the test above: same fixture, same call, no deadline.
    let (_project, analyzer) = divergent_namesakes(DIVERGENT_COUNT, &one_owner_terminal);
    let names = divergent_canonical_names(&one_owner_terminal);

    let result = get_symbol_sources_with_source_budget(
        &analyzer,
        SymbolLookupParams {
            symbols: vec![names[0].clone()],
        },
        usize::MAX,
        None,
    )
    .expect("the source byte budget is unbounded here");

    // One block, not two: the tool prefers the definition over the declaration
    // it unified with, so a reconciled name answers with its `.cpp` body. That
    // block exists at all only because reconciliation ran and was published.
    let paths: Vec<_> = result
        .sources
        .iter()
        .map(|block| block.path.clone())
        .collect();
    assert_eq!(vec!["c0.cpp".to_string()], paths, "{result:#?}");
}
