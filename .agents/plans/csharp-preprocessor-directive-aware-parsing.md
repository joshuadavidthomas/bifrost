# Parse C# through directive-aware included ranges so mid-declaration preprocessor conditionals stop destroying extraction

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md` at the repository root.

This plan resolves the core of GitHub issue #1803 ("FIRD escalation: C# preprocessor directives inside declarations silently destroy declaration extraction").

## Purpose / Big Picture

C# source files can contain preprocessor directives (`#if`, `#elif`, `#else`, `#endif`, `#region`, `#pragma`, and others). The tree-sitter-c-sharp grammar accepts these directives only at statement and declaration boundaries. When a directive appears INSIDE a declaration -- for example, when a member's modifier list differs between `#if NET` and `#else` branches, or when `#if` splits a ternary expression -- the parse breaks. Bifrost's declaration extraction then silently loses every member after the broken region.

Real damage observed in the FIRD differential campaign:

- markdig `src/Markdig/Helpers/StringSlice.cs` splits member modifier lists across `#if NET / #else / #endif` at four places. Bifrost indexes only the members declared before the first split (fields and the constructors); `TrimStart`, `SkipChar`, `NextChar`, `PeekChar`, `Match`, and every later method are absent from the index workspace-wide.
- commandlineparser `src/CommandLine/Core/TypeConverter.cs` has `#if !SKIP_FSHARP` inside a ternary expression. Tree-sitter's error recovery produces an ERROR node covering the whole compilation unit, so the file contributes zero structured declarations.

After this change, Bifrost parses C# files through tree-sitter "included ranges" that skip every preprocessor directive line and skip the source regions of inactive conditional branches. Both example shapes then parse cleanly: every member after a mid-declaration directive is extracted, named, and navigable. You can see it working by running the new integration test (described in Validation) which asserts that a `StringSlice`-shaped class exposes methods declared after a `#if/#else` modifier split, and that a `TypeConverter`-shaped file with `#if` inside a ternary still yields its class and methods.

## Strategy decision (why this option)

Issue #1803 lists four strategies: (a) pre-parse directive-line blanking keeping both branches, (b) evaluate-and-strip inactive branches, (c) grammar-level handling upstream, and (d) accept and diagnose. The selected solution is a structured variant of (b) plus the honesty diagnostic of (d), implemented WITHOUT transforming source bytes:

- Tree-sitter parsers accept a set of "included ranges" (`Parser::set_included_ranges`): byte ranges of the source that the parser reads; everything outside the ranges is invisible to the parse, while all node offsets still refer to the ORIGINAL file. The repository already uses this mechanism in `parse_source_range_with_cancellation` in `crates/bifrost-core/src/analyzer/common.rs`.
- We compute, per C# file, the ranges that exclude (1) every preprocessor directive line and (2) the full text of inactive conditional branches. The parse output is a clean single-configuration tree whose ranges map one-to-one onto the raw file. No transformed source ever exists, so cache identity remains the raw checkout bytes, consistent with the #1742 precedent ("Hash clean transformed worktree bytes" aligned persisted identities with the exact bytes visible through a checkout; we do not add a second identity).
- Branch selection is deterministic and needs no compilation-constant symbol model: within each `#if`/`#elif`/`#else` chain, the first branch is active, except a branch whose condition is the literal `false` (whitespace-trimmed exactly `false`), which is inactive; an `#if false` chain activates the `#else`/first non-`false` `#elif` branch instead. Nested chains inside an inactive region are wholly inactive.
- Option (a) was rejected because keeping both branches re-creates broken parses in exactly the motivating case (alternate modifier lists produce duplicate member headers). Option (c) is out of our control and would still require a configuration model. Option (d) alone abandons the recoverable declarations. Prior art trend: recent C/C++ work (commits `15a6cffe8`, `993d7232a`, `a751866c8`) models directives structurally over raw bytes rather than transforming sources; this plan follows the same philosophy at the parse layer, which is where C# (whose grammar, unlike C/C++, cannot represent mid-declaration directives) needs it.
- Index honesty: when a file has at least one inactive region excluded from the parse, extraction is by construction partial (declarations that exist only under the inactive configuration are absent). Milestone 4 surfaces a per-file "conditional compilation: inactive branches excluded from the index" diagnostic through the existing C# diagnostics path so consumers know.

Out of scope, tracked separately (do not implement here): the C/C++ faces in the issue's comments are different mechanisms (macro tokens in declarators, export-macro class heads, include-closure poisoning) and are filed or to be filed as their own issues (#1811, #1812, #1824 already exist; the C++ export-macro class-head and phantom-declaration faces need their own issues if none exist when this plan completes).

## Progress

- [x] (2026-08-11) Plan authored; strategy selected and recorded.
- [x] (2026-08-11) Milestone 1: probe tests recorded the real raw-parse damage; `crates/bifrost-csharp/src/preprocessor.rs` holds the scanner and 18 unit tests, all passing.
- [x] (2026-08-11) Milestone 2: `parse_csharp`, `csharp_included_ranges` and `csharp_parse_spec` exist; every C# parse site outside the central indexing path routes through one of them.
- [x] (2026-08-11) Milestone 3: `LanguageAdapter::parser_included_ranges` exists, C# overrides it, `set_parser_for_file` applies it at all three central parse sites, and the C# store epoch salt carries `preprocessor-directive-aware-parsing-2026-08`.
- [x] (2026-08-11) Milestone 4: the C# semantic diagnostics report an informational incomplete reason with the exact detail `conditional compilation: inactive branches excluded from the index` when a branch was excluded, and nothing when none was.
- [x] (2026-08-11) Milestone 5: 12 new integration tests (6 declaration, 3 usage, 3 diagnostics) plus 18 unit tests. `cargo fmt` applied, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, and eight suites green: suite_analyzers 787, suite_usages 499, suite_cross_language 274, suite_issues 107, suite_semantic 885, suite_smells 335, suite_persistence 1319, suite_symbols (in the same run) all with 0 failed.
- [x] (2026-08-11) Plan complete. One pre-existing test, `csharp_method_preprocessor_condition_is_a_terminal_typed_boundary`, pinned the damage as a contract and was rewritten; see the Decision Log and Outcomes.

## Surprises & Discoveries

- Observation (2026-08-11, Milestone 1): the raw parse does NOT simply drop later members, as the plan's Purpose section assumed. Tree-sitter recovers, but it recovers wrongly. For a signature split the grammar accepts a `preproc_if` node inside the declaration list, puts the `#if` copy of the member in it as a body-less declaration with a `MISSING ";"`, and nests every later member inside the `preproc_else` branch under a `MISSING "#endif"`. The member therefore appears TWICE and its siblings sit under directive nodes.
  Evidence: `raw methods: ["NextChar", "PeekChar", "TrimStart", "TrimStart"]` from the probe transcript in Artifacts.
  Implication: the loss the issue reports happens in the declaration walk, which does not descend through `preproc_*` wrappers, not in tree-sitter itself. The unit probes therefore assert tree distortion (duplicate member, `preproc` nodes, ERROR count), and the end-to-end proof of the fix is the Milestone 5 integration test over the real declaration walk. The probes were rewritten accordingly rather than asserting a claim that is not true.

- Observation (2026-08-11, Milestone 5): the end-to-end damage is exactly what the issue reports, confirmed by disabling only the adapter hook and re-running the new declaration test. The `StringSlice` file then indexed `["StringSlice", "StringSlice.Start", "StringSlice.Text"]` -- the type and the two fields declared BEFORE the split, and nothing after it. With the hook restored all seven members are indexed.
  Evidence: `StringSlice.TrimStart must be indexed; the file declares ["StringSlice", "StringSlice.Start", "StringSlice.Text"]`, from `cargo test --test suite_analyzers -- csharp_preprocessor` with `parser_included_ranges` forced to `None`.

- Observation (2026-08-11, Milestone 5): the `TypeConverter` ternary shape does NOT lose its declarations in this repository today, contrary to the issue's claim of "zero structured declarations". Tree-sitter keeps both method declarations around the ERROR nodes, and the declaration walk finds them. The three declaration assertions for that shape therefore pass before and after the change; what the fix removes is the ERROR nodes, which is what the semantic-diagnostics pass gates on before it checks any name in the file.
  Evidence: `a_directive_inside_a_ternary_still_yields_the_type_and_its_methods` passes with `parser_included_ranges` forced to `None`; the probe transcript shows the raw tree keeping `ChangeType` and `ParseInt`.

- Observation (2026-08-11, Milestone 1): a directive shape matters more than the directive kind. Four shapes (signature split, modifier-only split, statement-body split, parameter-list split) all set `has_error`, but they degrade differently; the parameter split produces `preproc_if_in_attribute_list`, a node the grammar has for a completely different context.
  Evidence: probe run over the four shapes, recorded during Milestone 1.

The three design facts the plan told the implementer to verify early, all now confirmed:

- CONFIRMED: an empty range slice means "parse the whole file". The scanner emits one zero-width range for a wholly inactive file instead, and `a_wholly_inactive_file_emits_one_zero_width_range` asserts both the range and that the resulting tree has no named children. The same property is what lets `set_parser_for_file` clear a previous file's ranges with `set_included_ranges(&[])` when a language does not restrict the parse.
- CONFIRMED: points must agree with byte offsets. `advance_ts_point` was widened to `pub` and is the only byte-to-point math in the scanner. `range_points_match_their_byte_offsets` recomputes every range's start point from the raw prefix and compares, and `recovered_nodes_keep_raw_file_offsets` asserts a recovered `TrimStart` identifier sits at its real byte offset in the untransformed file.
- CONFIRMED: the probe ran first, against tree-sitter-c-sharp 0.23.1. Both transcripts are in Artifacts, and they corrected the plan's model of the damage (see the first Surprise).

## Decision Log

- Decision: use tree-sitter included ranges rather than blanking source bytes.
  Rationale: no transformed bytes means no second cache identity (consistent with #1742), native positional fidelity (all node ranges are raw-file offsets), and reuse of a mechanism the repo already employs (`parse_source_range_with_cancellation`).
  Date/Author: 2026-08-11 / Claude (planning) per issue #1803 escalation.
- Decision: deterministic first-active-branch model; literal `false` is the only condition evaluated.
  Rationale: no compilation-constant symbol model exists and inventing one is speculative (YAGNI). First-branch-active recovers the motivating corpora (`#if NET` keeps the modern branch; `#if !SKIP_FSHARP` keeps the F# branch). `#if false` is a common idiom for disabled code and is trivially decidable.
  Date/Author: 2026-08-11 / Claude (planning).
- Decision: the directive scanner is a hand-written lexical line scanner in `crates/bifrost-csharp/src/preprocessor.rs`, tracking comment and string context.
  Rationale: CLAUDE.md prohibits string scanning that REPLACES available structure. Pre-parse, no tree-sitter structure exists -- the scanner is the layer that creates parseability, exactly as a real C# lexer treats directives before the syntactic grammar. This is not a fallback for missing structured support; it is the structured support.
  Date/Author: 2026-08-11 / Claude (planning).
- Decision: apply included ranges at EVERY C# parse site via one shared helper, and bump the C# store epoch salt.
  Rationale: if the indexed declaration walk and (say) the usage-graph forward scan parse the same file differently, ranges and node shapes disagree and navigation breaks subtly. One helper (`parse_csharp`) is the invariant. The epoch bump forces persisted extractions to re-run under the new parse.
  Date/Author: 2026-08-11 / Claude (planning).
- Decision: do not lex the text of an inactive branch; only look for directive lines in it.
  Rationale: this is what a C# compiler does. Disabled text is skipped, not tokenized. If the scanner tracked strings and comments inside a disabled branch, an unterminated `@"` or `/*` in dead code would corrupt the lexical mode of the live code that follows.
  Date/Author: 2026-08-11 / Claude (Milestone 1).
- Decision: `has_inactive_regions` is true only when a NON-EMPTY source line was excluded by a conditional branch.
  Rationale: the flag drives the Milestone 4 user-visible diagnostic. A chain whose inactive branch holds nothing but blank lines hides no declaration, so a diagnostic there would be noise.
  Date/Author: 2026-08-11 / Claude (Milestone 1).
- Decision: widen `advance_ts_point` in `crates/bifrost-core/src/analyzer/common.rs` from private to `pub` instead of duplicating the byte-to-point math.
  Rationale: the plan's own guidance. A point that disagrees with its byte offset corrupts every node position, so one implementation must serve every producer of a `tree_sitter::Range`.
  Date/Author: 2026-08-11 / Claude (Milestone 1).
- Decision: carry the pre-parse with the grammar in a new core type `ParseSpec`, instead of threading a range slice through the language-blind parse helpers.
  Rationale: the usage-graph on-demand parse (`parse_and_collect` and friends) is shared by ten languages and takes a bare `&tree_sitter::Language`. A C#-only extra parameter would have been a flag every other caller passes `None` to, which CLAUDE.md warns against. `ParseSpec` is data: a grammar plus an optional `fn(&str) -> Option<Vec<Range>>`. Every other language calls `ParseSpec::whole(&language)` and C# calls `csharp_parse_spec(&language)`, so a C# parse cannot silently lose the pre-parse.
  Date/Author: 2026-08-11 / Claude (Milestone 2).
- Decision: route the structural-facts extraction through a new `StructuralSpec::parser_included_ranges` default method rather than changing `extract_file_facts`'s signature.
  Rationale: `extract_file_facts_limited` already receives the per-language spec beside the grammar, so the language-specific knowledge has a home there and eight call sites stay untouched. RQL `query_code` over C# now sees the same tree the declaration walk sees.
  Date/Author: 2026-08-11 / Claude (Milestone 2).
- Decision: collapse the "C# parser is unavailable" incomplete reason in `diagnostics.rs` into the existing "C# source did not parse" one.
  Rationale: the shared helper returns one `Option`. Loading a grammar that is compiled into the binary cannot fail in a way a caller can act on, and CLAUDE.md prohibits error handling with no specific recovery.
  Date/Author: 2026-08-11 / Claude (Milestone 2).
- Decision: express the index-honesty notice as `SemanticDiagnosticIncompleteReason::UnsupportedSemantics` with the plan's exact detail string, not as a `SemanticDiagnostic`.
  Rationale: in this repository's model a `SemanticDiagnostic` is a proven absence, which a host renders as an error. The plan requires the notice to be informational. An `Incomplete` outcome with no range is precisely the existing typed way to say "this pass's statement about this file is not complete", which is exactly true when a branch was excluded. The wording is a public constant, `CSHARP_INACTIVE_BRANCHES_DETAIL`, so the producer and the message cannot drift.
  Date/Author: 2026-08-11 / Claude (Milestone 4).
- Decision: the integration tests live in `tests/suite_analyzers/` and `tests/suite_usages/`, not in the `tests/suite_declarations/` the plan names.
  Rationale: there is no `suite_declarations` harness in this tree. Declaration-extraction tests for every language live in `suite_analyzers`, and that is where the C# ones already are. Creating a new suite binary for three tests would contradict the harness-consolidation rule in CLAUDE.md.
  Date/Author: 2026-08-11 / Claude (Milestone 5).
- Decision: keep C and C++ untouched in this plan.
  Rationale: their grammars natively parse directives; their residual faces are macro-token and include-resolution mechanisms with different fixes, already tracked separately.
  Date/Author: 2026-08-11 / Claude (planning).
## Outcomes & Retrospective

Written 2026-08-11, after all five milestones landed, from real command output.

Measured against the Purpose section, the change does what it set out to do. A C# file whose member signature is split across `#if NET` / `#else` used to index the declarations BEFORE the split and nothing after it. The `StringSlice` fixture indexed exactly `["StringSlice", "StringSlice.Start", "StringSlice.Text"]`, which is the struct and its two fields. It now indexes all seven members, `TrimStart` appears once rather than once per branch, and its range points at the real declaration in the raw file. A file with `#if` inside a ternary parses without a single ERROR node. Cross-file navigation reaches the recovered members and does not conflate them with a same-named member of an unrelated interface.

What the plan got wrong, and what that cost: the plan modelled the damage as tree-sitter dropping the later members. It does not. It recovers them into `preproc_if` / `preproc_else` wrappers under a `MISSING "#endif"`, and it is the declaration walk, which does not descend through those wrappers, that loses them. The probe-first rule caught this within the first milestone, before any of the scanner was written, and the unit probes were written against what the parser actually does instead of against the plan's assumption. The cost was one rewrite of two test bodies. Had the probe come after the scanner, the same discovery would have arrived with a scanner already built around a wrong model.

The one change of shape the plan did not anticipate: routing every parse site meant touching the language-blind on-demand parse that ten languages share. `ParseSpec` -- a grammar plus an optional pre-parse -- was the answer that avoided giving nine other languages a parameter they always pass nothing to.

The one behavior change outside C# extraction: `csharp_method_preprocessor_condition_is_a_terminal_typed_boundary` in `tests/suite_semantic/semantic_language_conformance.rs` asserted the old damage as a contract -- that a `#if` in a method body terminates control flow and every statement after the `#endif` is unreachable. It was rewritten to assert the repaired topology. A test that pins a defect is the defect's last line of defence, and it is worth saying that it was found by running the suite rather than by reading the code.

What remains: the honesty notice is per-file and coarse. It says a branch was excluded; it does not say which declarations that cost. A consumer that wants to reason about a second configuration still cannot. That is deliberate (no compilation-constant model exists), and the first-active-branch rule recovers both motivating corpora, but a workspace whose real build defines `SKIP_FSHARP` gets the other branch indexed and only the notice to tell it so. Also out of scope and still open: the C and C++ faces in the issue's comments, which are macro-token and include-resolution mechanisms with their own fixes (#1811, #1812, #1824).

## Context and Orientation

Bifrost is a Rust workspace that indexes source code with tree-sitter and answers code-intelligence queries. Language knowledge lives in per-language crates (`crates/bifrost-csharp`, `crates/bifrost-cpp`, ...) beneath `crates/bifrost-analysis` (the analyzer engine) and above `crates/bifrost-core` (shared model types; depends on no other Bifrost crate).

Key places for this plan:

- `crates/bifrost-csharp/src/` -- C# language knowledge: `declarations.rs` (the declaration walk that turns a tree into `CodeUnit`s), `syntax.rs`, `diagnostics.rs`, `graph/extractor.rs` and `graph/resolver.rs` (usage graph), `clones.rs`. Several of these construct their own `tree_sitter::Parser` with `tree_sitter_c_sharp::LANGUAGE` (tree-sitter-c-sharp 0.23.1, `crates/bifrost-csharp/Cargo.toml`).
- `crates/bifrost-analysis/src/analyzer/tree_sitter_analyzer.rs` -- the central per-file parse for indexing: `prepare_syntax_from_source_cancellable` (near line 4800) builds a `PreparedSyntaxTree` via `parse_complete_file_bounded` (near line 186). The adapter object (`self.adapter`) supplies `parser_language_for_file`; this is where a per-language included-ranges hook belongs.
- `crates/bifrost-analysis/src/analyzer/usages/` -- C#-specific parse sites: `receiver_sites.rs:369`, `csharp_graph.rs:218`, `get_definition/csharp.rs`.
- `crates/bifrost-analysis/src/analyzer/csharp/` -- the analyzer-side C# shim (`mod.rs:1287` returns the parser language; `structural.rs:16`).
- `crates/bifrost-core/src/analyzer/common.rs` -- `parse_source_region_with_cancellation` / `parse_source_range_with_cancellation` (existing included-ranges precedent) and `advance_ts_point` (byte-to-Point math).
- Store epochs: each language folds a salt plus its query assets into a per-language store epoch; changing the salt invalidates that language's persisted extractions. See `crates/bifrost-csharp/src/queries.rs` module docs and the C++ precedent commit `b020725e0`.
- Tests: integration tests live in `tests/<suite>/<name>.rs` with a `mod` line in that suite's `main.rs` (see `.agents/docs/test-harness-consolidation-2026-07.md`). Small inline projects use `InlineTestProject` from `tests/common/inline_project.rs`.

Definitions:

- "Directive line": a source line whose first non-whitespace byte is `#` while the lexer is outside comments and strings, through the end of that line (C# directives are line-scoped; no line continuation).
- "Inactive region": the byte span between a conditional-chain directive line and the next directive line of the same chain, for every branch the first-active-branch model marks inactive, including everything nested inside it.
- "Included ranges": the `&[tree_sitter::Range]` given to `Parser::set_included_ranges`; the complement of (directive lines + inactive regions). `tree_sitter::Range` carries `start_byte`, `end_byte`, `start_point`, `end_point`; points count rows and byte columns.

## Plan of Work

Milestone 1 -- fixtures, probe, and the scanner (prototyping first). Add `crates/bifrost-csharp/src/preprocessor.rs` (module registered in `lib.rs`). First write a small probe unit test that parses the two motivating shapes raw and asserts the damage (ERROR shapes), then the scanner:

    pub struct PreprocessorScan {
        pub included_ranges: Option<Vec<tree_sitter::Range>>, // None = no directives, parse whole file
        pub has_inactive_regions: bool,                        // drives the Milestone 4 diagnostic
    }
    pub fn scan_preprocessor(source: &str) -> PreprocessorScan

The scanner walks lines once, tracking lexical mode across lines: block comment depth (C# block comments do not nest; a bool), verbatim string `@"..."` (double `""` escapes), raw string `"""..."""` (track the opening quote count >= 3 and match an equal-or-longer closer), interpolated variants (`$@"`, `@$"`, `$"""`). A line whose first non-whitespace byte is `#` in normal mode is a directive line. Directive kinds parsed: `if`, `elif`, `else`, `endif` manage a stack of chain states; every other directive (`region`, `endregion`, `pragma`, `nullable`, `define`, `undef`, `line`, `error`, `warning`) is excluded but does not affect branch state. Chain state per stack entry: whether an active branch has been chosen, and whether the current branch is active. First branch whose trimmed condition is not `false` and whose enclosing region is active becomes the chain's active branch. Unbalanced directives (missing `#endif` at EOF, stray `#endif`/`#else`) must not panic: treat a stray closer as a no-op excluded line and close open chains at EOF (their tails stay as scanned). Emit included ranges as the complement, merging adjacent ranges; compute `Point`s with the same byte math as `advance_ts_point`. Unit tests (in-module `#[cfg(test)]`) cover: no directives (None), the StringSlice shape, the TypeConverter ternary shape, `#elif` chains, `#if false` activating `#else`, nested chains, directives inside `//`, `/* */`, verbatim/raw/interpolated strings (not directives), unbalanced input, CRLF line endings, a directive on the last line without trailing newline, and whole-file-inactive.

Milestone 2 -- one shared parse helper, used everywhere C# parses. In the same module:

    pub fn parse_csharp(source: &str) -> Option<tree_sitter::Tree>
    pub fn parse_csharp_with_cancellation(source: &str, cancellation: Option<&CancellationToken>) -> Option<tree_sitter::Tree>

These create the parser, set `tree_sitter_c_sharp::LANGUAGE`, apply `scan_preprocessor(...).included_ranges` when present, and parse. Migrate every C# parse site to it: `crates/bifrost-csharp/src/clones.rs:108`, `diagnostics.rs:145`, `graph/extractor.rs:67`, `graph/resolver.rs:727`, plus the analysis-crate sites `usages/receiver_sites.rs:369`, `usages/csharp_graph.rs:218`, `usages/get_definition/csharp.rs`, `csharp/structural.rs:16`. Structural search creates parsers through the structural spec's language; if it parses C# files itself, route it through the helper too. After this milestone the declaration walk already sees the repaired tree wherever those sites feed it.

Milestone 3 -- the central indexing path and cache identity. Add to the `LanguageAdapter` trait (the trait `prepare_syntax_from_source_cancellable` reaches through; where `parser_language_for_file` is declared):

    fn parser_included_ranges(&self, file: &ProjectFile, source: &str) -> Option<Vec<tree_sitter::Range>> { None }

The C# adapter override calls `scan_preprocessor`. In `prepare_syntax_from_source_cancellable`, after `set_language`, apply the ranges before `parse_complete_file_bounded`. Bump the C# store epoch salt (a named constant beside the C# query-asset fold; add `CSHARP_PREPROCESSOR_SALT` or increment the existing salt) so persisted C# declarations re-extract. Do the same for any secondary prepared-parse path that bypasses the adapter hook (verify by searching for `parse_complete_file_bounded` callers).

Milestone 4 -- index honesty. When `scan_preprocessor` reports `has_inactive_regions`, surface one per-file diagnostic through the existing C# diagnostics path in `crates/bifrost-csharp/src/diagnostics.rs`, message exactly: `conditional compilation: inactive branches excluded from the index`. It must be informational (not an error), stable, and covered by a diagnostics test.

Milestone 5 -- integration proof and gates. New integration tests using `InlineTestProject`: `tests/suite_declarations/csharp_preprocessor_split_declarations_test.rs` (StringSlice shape: class with fields, a `#if NET`/`#else` modifier split mid-file, then methods `TrimStart`, `NextChar`, `PeekChar`; assert all are indexed with correct ranges; TypeConverter shape: `#if !SKIP_FSHARP` inside a ternary; assert the class and its methods are indexed) and `tests/suite_usages/usages_csharp_preprocessor_split_test.rs` (a second file calls `TrimStart` on the first type; assert forward navigation reaches the method, not an unrelated interface). Register each with a `mod` line in the suite's `main.rs`. Run the focused suites, then fmt and workspace clippy.

## Concrete Steps

All commands run at the repository worktree root.

    cargo test -p brokk-bifrost-csharp preprocessor        # Milestone 1/2 unit tests
    cargo test --test suite_analyzers -- csharp_preprocessor
    cargo test --test suite_usages -- usages_csharp_preprocessor
    cargo test --test suite_semantic -- csharp_semantic_diagnostics
    cargo fmt
    cargo clippy --workspace --all-targets --all-features -- -D warnings

Actual output, 2026-08-11:

    test result: ok. 18 passed; 0 failed; 1 ignored          # brokk-bifrost-csharp preprocessor
    test result: ok. 6 passed; 0 failed                      # csharp_preprocessor declarations
    test result: ok. 3 passed; 0 failed                      # usages_csharp_preprocessor
    test result: ok. 33 passed; 0 failed                     # csharp_semantic_diagnostics
    Finished `dev` profile ... in 1m 58s                     # clippy, no warning and no error

The eight suites run together as one command:

    cargo test --test suite_analyzers --test suite_usages --test suite_semantic \
        --test suite_symbols --test suite_cross_language --test suite_issues \
        --test suite_smells --test suite_persistence

    test result: ok. 787 passed; 0 failed; 0 ignored
    test result: ok. 499 passed; 0 failed; 0 ignored
    test result: ok. 274 passed; 0 failed; 0 ignored
    test result: ok. 107 passed; 0 failed; 0 ignored
    test result: ok. 885 passed; 0 failed; 19 ignored
    test result: ok. 335 passed; 0 failed; 0 ignored
    test result: ok. 1319 passed; 0 failed; 1 ignored

(Per CLAUDE.md, clippy needs the rustup toolchain on PATH, `--workspace` is mandatory, and no NLP build is required for this change.)

## Validation and Acceptance

Acceptance is behavioral:

1. `csharp_preprocessor_split_declarations_test` fails before Milestones 2-3 land (methods after the split are missing) and passes after: every method declared after a `#if/#else` modifier split is present with a range pointing at its real location in the raw file.
2. The TypeConverter-shaped file yields its class and methods instead of zero declarations.
3. `usages_csharp_preprocessor_split_test` proves cross-file navigation lands on the recovered method.
4. The diagnostics test observes exactly one `conditional compilation: inactive branches excluded from the index` diagnostic for a file with an `#else` branch and none for a directive-free file.
5. `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean; the focused C# suites pass; the pre-existing known failures listed in the session memory (cache_db, mcp_cli prewarm, nlp active_index) are the only tolerated failures in a full run.

## Idempotence and Recovery

All steps are additive and re-runnable. If a migration of a parse site breaks a suite, the helper accepts the old behavior by returning `None` ranges for directive-free files, so bisecting is per-site. The epoch salt bump is safe to apply repeatedly (any change re-extracts). No destructive operations.

## Artifacts and Notes

Milestone 1 probe, 2026-08-11. Run it again with:

    cargo test -p brokk-bifrost-csharp preprocessor::tests::probe_transcript -- --ignored --nocapture

The `StringSlice` shape (a member signature split across `#if NET` / `#else`), parsed RAW. The interesting parts are marked with `<<<`:

    (compilation_unit (namespace_declaration ... body: (declaration_list (struct_declaration ...
      body: (declaration_list
        (field_declaration ...) (field_declaration ...)
        (preproc_if condition: (identifier)                                   <<< directive node in the member list
          (method_declaration (attribute_list ...) (modifier) (modifier)
            returns: (predefined_type) name: (identifier)
            parameters: (parameter_list) (MISSING ";"))                       <<< first copy, no body
          alternative: (preproc_else
            (method_declaration (modifier) ... (ERROR) body: (block ...))     <<< second copy of TrimStart
            (method_declaration ...)                                          <<< NextChar, nested in the else branch
            (method_declaration ...))                                         <<< PeekChar, nested in the else branch
          (MISSING "#endif"))))))
    raw methods: ["NextChar", "PeekChar", "TrimStart", "TrimStart"]

The same source parsed through the included ranges this plan computes:

    (compilation_unit (namespace_declaration ... body: (declaration_list (struct_declaration ...
      body: (declaration_list
        (field_declaration ...) (field_declaration ...)
        (method_declaration (attribute_list ...) (modifier) (modifier)
          returns: (predefined_type) name: (identifier)
          parameters: (parameter_list) body: (block ...))
        (method_declaration ...) (method_declaration ...))))))
    repaired methods: ["NextChar", "PeekChar", "TrimStart"]

No `preproc_*` node, no ERROR, no MISSING, each member exactly once, and each member is a direct child of the declaration list again.

The `TypeConverter` shape (`#if !SKIP_FSHARP` inside a ternary), parsed RAW:

    ... body: (block (return_statement (conditional_expression
      condition: (binary_expression ...)
      (ERROR (invocation_expression ...))                                     <<< the true arm becomes an ERROR
      consequence: (preproc_if condition: (unary_expression ...) (MISSING "#endif"))
      alternative: (conditional_expression ...)))
      (ERROR) (expression_statement ...) (ERROR))                             <<< the else arm leaks into the block

Through the included ranges the same method is one clean nested `conditional_expression` with no ERROR node, and both `ChangeType` and `ParseInt` survive.

## Interfaces and Dependencies

No new external dependencies. tree-sitter and tree-sitter-c-sharp stay at workspace versions. New public surface, all in `brokk-bifrost-csharp`:

    crates/bifrost-csharp/src/preprocessor.rs:
        pub struct PreprocessorScan { pub included_ranges: Option<Vec<tree_sitter::Range>>, pub has_inactive_regions: bool }
        pub fn scan_preprocessor(source: &str) -> PreprocessorScan
        pub fn parse_csharp(source: &str) -> Option<tree_sitter::Tree>
        pub fn parse_csharp_with_cancellation(source: &str, cancellation: Option<&CancellationToken>) -> Option<tree_sitter::Tree>

    LanguageAdapter (crates/bifrost-analysis, where parser_language_for_file lives):
        fn parser_included_ranges(&self, file: &ProjectFile, source: &str) -> Option<Vec<tree_sitter::Range>> { None }

Revision note (2026-08-11): initial version, authored during planning for issue #1803. Nothing is implemented yet; the living sections start empty and must be filled with real evidence as work proceeds.

Revision note (2026-08-11, implementation): all five milestones are implemented and the living sections carry real evidence. Three substantive corrections to the plan as written, each recorded above with its reason. First, the model of the damage was wrong: tree-sitter recovers the members into `preproc_*` wrappers rather than dropping them, and the loss happens in the declaration walk, so the probe assertions describe tree distortion instead of absence (Surprises, first entry). Second, the parse-site migration needed a new core type, `ParseSpec`, because the on-demand parse the plan wanted routed is shared by ten languages (Decision Log). Third, the integration tests live in `tests/suite_analyzers/` and `tests/suite_usages/`, because the `tests/suite_declarations/` the plan names does not exist in this tree (Decision Log). One pre-existing conformance test asserted the defect as a contract and was rewritten.
