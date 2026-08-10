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
use brokk_bifrost::searchtools::{
    SYMBOL_TOOL_MAX_RESOLUTION_CANDIDATES, SummariesParams, SymbolLookupParams, TooBroadMatch,
    get_summaries, get_symbol_sources,
};
use brokk_bifrost::{CppAnalyzer, Language};

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
