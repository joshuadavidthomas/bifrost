# Replace stringly qualified names with interned, kind-tagged segments (FqName)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.
This document must be maintained in accordance with `.agents/PLANS.md` (repository root
relative path), the canonical rules for ExecPlans.

## Purpose / Big Picture

Bifrost identifies every declaration by a qualified name stored as a plain string
(`package_name` + `short_name` on `CodeUnit`, e.g. `log4cxx.HTMLLayout.getContentType`
or `cutlass::gemm::warp.OperandSharedStorage.OperandLayout`). The structure of that
string — where one segment ends and the next begins, and what kind of segment it is —
is not recorded anywhere. Every consumer re-infers it by splitting on a guessed set of
delimiter characters (`.` `::` `$` `/` `#` `+`), per call site, and the per-language
spelling conventions differ (C++ stores a `::` namespace head with a `.` member tail;
Scala appends `$` for companion objects; file-stem segments may contain literal dots).

That inference is a recurring bug factory. In one week of campaign work the following
all reduced to it: rust raw identifiers containing `#` colliding with `file#symbol`
anchor splitting (issue 1128); the anchor split point itself (issue 1131); a bare
`DbColumn.r#type` misrouted as a `.r` FILE anchor; `::`-qualified references never
matching the shared resolver's `.`-composed candidates (issue 1162); C++'s
mixed-separator store discovered only when normalizing the scope side broke a cutlass
test, leaving a confirmed reachable false "outside the workspace" claim (issue 1163);
and Scala `$`-spelling inconsistencies between surfaces. Counts in the current tree:
about 144 `format!("{parent}.{name}")`-style construction sites and about 227
separator-split sites under `src/analyzer`.

After this change, a qualified name is a `FqName`: a small vector of interned segment
IDs, where each ID identifies a `(text, kind)` pair — kind being Path, Package, Type,
Member, or Companion. Structure is recorded once at construction (where the language
extractor knows exactly what it is emitting) and never inferred again. Native
delimiters remain accepted at the MCP input edge and rendered at the output edge, but
the interior of the system stops splitting strings entirely. Observable outcomes: the
delimiter-bug regression suites (`tests/issue_1128_rust_raw_identifiers.rs`,
`tests/issue_1162_separator_aware_enclosing_scope.rs`, the anchor tests in
`tests/searchtools_definition_selectors.rs`) keep passing with the inference code
deleted; the pinned false-boundary test
`cpp_qualified_nested_namespace_type_current_behavior` in
`tests/issue_1162_separator_aware_enclosing_scope.rs` flips from documenting the bug
to asserting correct resolution (this is issue 1163's fix falling out of the
representation); and a guard test fails the build if separator-splitting reappears in
the analyzer tree.

## Progress

- [x] (2026-07-24) M0: `FqName` + interner module with unit tests and a
      memory/size measurement. Landed as `src/analyzer/fq_name.rs` (registered
      in `src/analyzer/mod.rs`). Ten `--lib fq_name` tests pass; measurement
      recorded in Surprises & Discoveries.
- [ ] M1: dual representation — emission points populate `FqName` alongside the
      legacy strings, with an equivalence check (per language; check off individually).
  - [ ] rust  - [ ] cpp  - [ ] java  - [ ] python  - [x] go (2026-07-24)  - [ ] php
  - [ ] ruby  - [ ] scala  - [ ] csharp  - [ ] javascript  - [ ] typescript
- [ ] M2: shared services and selectors consume `FqName`; input parsing produces it.
- [ ] M3: persistence flip — store schema carries segments; single epoch salt bump.
- [ ] M4: retire string inference; grep-gate; issue-1163 pilot flip.

## Surprises & Discoveries

- Observation (M0 memory measurement): interning is a large win because Bifrost's
  qualified names share heavy prefixes (directory/package heads, owner types).
  Evidence — the `measure_interned_vs_legacy_bytes` test builds a real corpus
  from this crate's own `src/` tree (527 `.rs` files → 1581 synthetic fq names:
  each path component a `Path` segment, the file stem a `Type`, two `Member`
  leaves):

        [fq_name measurement] corpus: 527 files, 1581 fq names
        [fq_name measurement] summed legacy string bytes: 43285
        [fq_name measurement] interner entries: 371, unique text bytes: 3646 (+1484 bytes of ids)
        [fq_name measurement] interned/legacy text ratio: 0.084

  Interned unique text is ~8.4% of the summed legacy string bytes (3646 vs
  43285); adding the 4-byte-per-entry id table (1484 bytes) it is still ~12%.
  The memory question is answered with numbers: the interned representation is
  far smaller than the concatenated strings even before counting that each
  `FqName` now stores 4-byte ids instead of owning its own `String`.

- Observation (go): a Go `package_name` is the canonical *import path*
  (`github.com/foo/bar`), so its `/`-separated components are `Path` segments and
  a component that itself contains a literal dot (`github.com`) must stay a single
  segment. Canonical `display()` therefore renders `/` between adjacent `Path`
  segments and `.` at the `Path`→name transition, which is exactly the legacy
  `fq_name()` shape. Evidence: `display_round_trips_go_import_path` asserts
  `github.com/foo/bar.Baz.method`, and every go analyzer/usages suite passes with
  the debug/test equivalence assertion active (it compares `fq.display()` to the
  legacy `package_name.short_name` join for every constructed unit).

- Observation (equivalence assertion fires): a deliberate mutation (appending a
  bogus `Member` segment `"BOGUS_MUTATION"` in `visit_go_type_spec`) made the
  `debug_assert_eq!` in `CodeUnit::with_signature_and_fq` fail loudly across the
  go suites before it was reverted. Evidence:

        thread panicked at src/analyzer/model.rs:1890:
        assertion `left == right` failed: FqName does not round-trip to the legacy
        qualified name (kind=Class, package_name="main", short_name="Target")
          left: "main.Target.BOGUS_MUTATION"
          right: "main.Target"

- Observation (`_module_` scope): Go package-level `var`/`const`/`type alias`
  units carry the synthetic scope segment `GO_MODULE_SCOPE_SEGMENT` (`_module_`)
  in their `short_name` (`_module_.name`). It is emitted as a `Package` segment
  (a module scope), which round-trips identically; its precise kind only matters
  once M2 walks owner chains, and can be revisited there without affecting the M1
  string equivalence.

## Decision Log

- Decision: internal representation is `SmallVec<[SegmentId; 8]>` where `SegmentId`
  is a `u32` interning a `(text, kind)` PAIR — kind is baked into the interned entry,
  not stored in a parallel per-position field.
  Rationale: Jonathan, 2026-07-24 — a parallel packed-kinds field is clunky; the cost
  of occasionally interning the same text under two kinds (two entries for "src") is
  negligible, and baking kind in keeps FqName to a single small vector with pure
  integer comparisons.
- Decision: no scope-trie / parent-pointer compression, now or as part of this plan.
  Rationale: Jonathan, 2026-07-24 — the trie's chain-construction machinery (shared
  mutable hash-consing across parallel extraction, grow-only arena, load-time rebuild)
  is not worth structural prefix sharing when interned IDs already cost 4 bytes per
  segment. If profiling ever shows prefix repetition matters, a trie can hide behind
  the same FqName API later.
- Decision: no canonical-string-plus-boundary-index representation.
  Rationale: Jonathan, 2026-07-24 — strictly clunkier than interned IDs.
- Decision: SegmentIds are process-local; persistence stores segment text+kind, never
  IDs. Rationale: IDs from a hash-consing interner are not stable across processes or
  runs; persisting them would couple the store to interner insertion order.
- Decision (M0): the interner is a single process-global `OnceLock<SegmentInterner>`
  (accessor `crate::analyzer::fq_name::segment_interner()`), not per-workspace.
  Rationale: threading a per-workspace interner through every `CodeUnit`
  constructor across eleven languages is a large mechanical cost with no
  correctness benefit while the legacy strings remain authoritative; entries are
  tiny and text-deduplicated, and the plan explicitly permits one interner per
  process. Date/Author: 2026-07-24, implementation (go/M0 wave).
- Decision (M0): the interner is sharded (16 shards), each an
  `RwLock<{ by_text: FxHashMap<String, SmallVec<[(kind,id);2]>>, entries:
  Vec<(&'static str, kind)> }>`. `SegmentId(u32)` encodes `index*SHARD_COUNT +
  shard` so a bare id resolves without a side table. Segment text is leaked once
  on first insert (`Box::leak`) so `resolve` can return a `&str` that outlives
  the read guard; the interner is grow-only for the process lifetime and bounded
  by the segment vocabulary, so the leak is an arena, not a leak-per-call.
  Rationale: extraction is file-parallel, so `intern` must be lock-cheap; the hot
  (hit) path takes only a read lock and the `String`-keyed map allows borrowed
  `&str` lookups with no allocation. Date/Author: 2026-07-24, implementation.
- Decision (M0): `SmallVec<[SegmentId; 8]>` sourced from a new direct dependency
  `smallvec = "1"` (already present transitively at 1.15.2 in `Cargo.lock`; no
  new crate is downloaded). Rationale: the plan mandates the type; adding it as a
  direct dependency is the sanctioned way to use it. Date/Author: 2026-07-24.
- Decision (M0): canonical `FqName::display` renders `/` between two adjacent
  `Path` segments, `$` before a `Companion` segment, and `.` otherwise;
  `display_native(Cpp, ..)` additionally renders `::` between adjacent `Package`
  segments. Rationale: the plan's "`.`-joined" is the common case, but go's
  legacy `fq_name()` already embeds `/` inside `package_name`
  (`github.com/foo/bar.Sym`), so the canonical display MUST reproduce `/` to
  round-trip. Path segments are always a leading prefix, so `Path`→name is `.`.
  The `Companion`/`::` rules are provisional until scala/cpp are migrated (they
  are unit-tested but unused by go). Date/Author: 2026-07-24.
- Decision (M1 CodeUnit field): `fq: FqName` is added to `CodeUnitInner` and is
  DELIBERATELY EXCLUDED from `CodeUnit`'s identity (`PartialEq`/`Eq`/`Hash`/`Ord`,
  which are hand-written and reference the string fields directly). The unused
  derived `PartialEq/Eq/Hash` on `CodeUnitInner` were dropped (kept `Debug`) so
  no path accidentally includes `fq` in identity. Rationale: `fq` is a redundant
  derived form of the strings, and during dual representation a freshly-extracted
  unit (populated `fq`) and a cache-loaded or not-yet-migrated unit (empty `fq`)
  describing the SAME declaration must compare equal, or every `HashMap<CodeUnit,
  ..>` lookup would break. Non-migrated languages stay compiling because
  `with_signature`/`new` default `fq` to `FqName::new()` (empty) and the
  equivalence assertion is skipped for empty `fq`. Go opts in via the new
  `pub(crate)` `CodeUnit::new_fq` / `with_signature_and_fq`. Date/Author:
  2026-07-24.
- Decision: the issue-1162 landing deliberately left scope-side fq strings verbatim
  (C++'s mixed `::`/`.` store) as a workaround; that workaround inverts at M3 when
  the store carries explicit segments and C++ emits tagged segments like every other
  language. Recorded so nobody "fixes" the workaround independently.

## Outcomes & Retrospective

(to be filled at milestone completions)

## Context and Orientation

Bifrost is a Rust code analyzer and MCP server. Each source declaration becomes a
`CodeUnit` (defined in `src/analyzer/model.rs`, fields around line 1807):
`package_name: String` (the namespace/package/module prefix, whose spelling is
per-language) and `short_name: String` (the owner-and-member tail, joined with `.`
and with `$` marking nested classes / Scala companions). The full qualified name
("fq name") is derived by joining the two. These strings are persisted in a SQLite
cache: table `code_units`, column `short_name` (see
`migrations/cache/0001-initial.sql` around line 75; `package_name` likewise). The
per-language "analysis epoch" (`src/analyzer/store/epoch.rs`) fingerprints extractor
behavior: when persisted output changes shape, the language's `SALT` string must get
a new `;`-separated token appended, which forces re-extraction of cached rows.

Three shared consumers matter most, all recently consolidated (which is what makes
this migration tractable now):

`parse_symbol_path` (`src/analyzer/symbol_lookup.rs`, around line 713, `pub(crate)`):
the input-edge splitter. Takes a user-supplied symbol string and a language, splits on
the full separator set (`::`, `.`, `\`, `/`, `+`) and applies per-language segment
normalization (rust `r#` stripping, go receiver forms, cpp `operator` names). This is
the ONLY place input strings should ever be split, and after this plan it returns a
`FqName` rather than `Vec<String>`.

`resolve_qualified_name_in_shrinking_scopes` and `resolve_in_enclosing_scopes`
(`src/analyzer/usages/get_definition/mod.rs`): the shared enclosing-scope resolution
service. Today it composes candidate strings `{scope}.{name}` and, since issue 1162,
normalizes the REFERENCE side into `.`-joined segments via `parse_symbol_path` while
leaving the SCOPE side verbatim (because C++'s store is mixed-separator — see the
Decision Log). After M2 it operates on segments.

`enclosing_owner_chain` (`src/analyzer/usages/common.rs`) and the trait method
`IAnalyzer::parent_of` (`src/analyzer/i_analyzer.rs`, default around line 681): owner
chain walking. The default `parent_of` currently walks the fq string looking for
`.`/`$`/`::`/`->` separators — a textbook inference site that M2 replaces with a
segment pop.

Anchor splitting: `split_definition_selector_with_resolver` in
`src/searchtools/selectors.rs` decides whether `a/b.rs#Foo.bar` is a file anchor plus
symbol. Since issue 1131 it only splits at a `#` whose left side names a real file;
issue 1128 added a carve-out for slash-free anchors. With kind-tagged segments the
path/symbol boundary is a tag transition, and this heuristic stack shrinks.

"Emission points" means the ~144 places in per-language extractors (each language's
`declarations.rs` and related visitors under `src/analyzer/<lang>/`) that build
`short_name`/`package_name` by string concatenation, e.g.
`format!("{}.{}", parent.short_name(), name)` in `visit_rust_module`
(`src/analyzer/rust/declarations.rs`) or the `$`-joining nested-class chains in
`split_cpp_name` (`src/analyzer/cpp/declarations.rs`). Each such site knows, at the
moment of concatenation, exactly what kind of segment it is appending — that
knowledge is what the current representation throws away and this plan preserves.

## Interfaces and Dependencies

In a new file `src/analyzer/fq_name.rs` (module registered in
`src/analyzer/mod.rs`), define:

    /// What a qualified-name segment denotes. Baked into the interned entry.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub(crate) enum SegmentKind {
        Path,      // a file/directory step (may contain literal dots)
        Package,   // namespace / package / module
        Type,      // class, struct, enum, trait, interface, object
        Companion, // scala companion-object spelling (renders with `$`)
        Member,    // function, method, field, const, alias, macro
    }

    /// Interned (text, kind) pair. u32; process-local; never persisted.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub(crate) struct SegmentId(u32);

    /// The qualified name. Ordered root-to-leaf. Comparisons are integer memcmp.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    pub(crate) struct FqName {
        segments: SmallVec<[SegmentId; 8]>,
    }

    pub(crate) struct SegmentInterner { /* sharded, concurrent */ }

    impl SegmentInterner {
        pub(crate) fn intern(&self, text: &str, kind: SegmentKind) -> SegmentId;
        pub(crate) fn resolve(&self, id: SegmentId) -> (&str, SegmentKind);
    }

    impl FqName {
        pub(crate) fn push(&mut self, id: SegmentId);
        pub(crate) fn parent(&self) -> Option<FqName>;         // slice, no alloc beyond SmallVec copy
        pub(crate) fn last(&self) -> Option<SegmentId>;
        pub(crate) fn starts_with(&self, prefix: &FqName) -> bool;
        pub(crate) fn segments(&self) -> &[SegmentId];
        /// Canonical display: `.`-joined, `$` before Companion segments — exactly
        /// today's user-facing convention, so display output does not change.
        pub(crate) fn display(&self, interner: &SegmentInterner) -> String;
        /// Native display: language-specific separators (`::` between cpp
        /// Package segments, etc.), for surfaces that render native spellings.
        pub(crate) fn display_native(&self, lang: Language, interner: &SegmentInterner) -> String;
    }

The interner lives on the analyzer/workspace object that already owns per-workspace
state (follow how existing per-workspace caches are threaded; one interner per
process is also acceptable since entries are tiny and text-deduplicated — decide in
M0 and record in the Decision Log). Concurrency: extraction runs file-parallel, so
`intern` must be lock-cheap — use a sharded RwLock<HashMap> or an existing concurrent
map already in the dependency tree (moka is present; a plain sharded map is fine).
Do not add new heavyweight dependencies without recording the decision.

SmallVec is already available transitively; if not a direct dependency, add it to
Cargo.toml (record in Decision Log). Eight inline segments covers observed real fq
names (path head + package + owner chain + member); measure in M0.

## Plan of Work

The migration is strictly staged so the tree is green at every commit. The legacy
strings remain authoritative until M3; `FqName` rides alongside and is
equivalence-checked against them, so any construction bug surfaces as a test failure
while the strings still drive behavior.

M0 builds the module in isolation: interner, FqName, unit tests for push/parent/
starts_with/display round-trips including segments containing literal dots, `::`,
`$`, and `#` (the point of the design is that segment text is free-form). Add a
`#[cfg(test)]` size/memory measurement: intern the fq names of a representative
fixture workspace (reuse an existing large test fixture) and print interner entry
count and approximate bytes versus the sum of legacy string lengths, so the memory
question is answered with numbers, not vibes.

M1 makes every emission point ALSO produce the structured form. Add to `CodeUnit` an
`fq: FqName` field populated at construction. Do this language by language, one
commit each, in this order (smallest/cleanest first to shake out the API, the two
known-messy ones last): go, python, ruby, php, java, javascript, typescript, csharp,
rust, scala, cpp. For each language, find every constructor call of `CodeUnit` in its
extractors and thread segment pushes where the strings are concatenated today: the
package prefix becomes Path segments (one per path component, from the workspace-
relative file path already used to build it) plus Package segments (from
namespace/module declarations); owners push Type (or Companion for scala's
`$`-spelled objects); leaves push Member or Type per the unit kind. The equivalence
check: a debug/test-only assertion (behind `#[cfg(any(test, debug_assertions))]`)
that `fq.display(interner)` equals the legacy joined string for every constructed
unit; run each language's full test suite and fix mismatches at the emission point.
Two known reconciliations, to be handled deliberately rather than discovered: C++'s
package strings keep a `::` head today — its emission points push proper Package
segments and the equivalence assertion for cpp compares against a `::`-aware join
(write a cpp-specific expected-join helper in the test support, documenting that the
LEGACY string is the compatibility target until M3); Scala companion objects append
`$` inside short_name — the Companion kind reproduces that in `display`.

M2 moves the consolidated consumers onto segments. `parse_symbol_path` gains a
sibling `parse_symbol_path_fq(language, value, &interner) -> FqName` (input segments
get their kinds assigned best-effort: file-extension-bearing or slash-delimited heads
become Path, the final segment Member-or-Type-unknown — introduce
`SegmentKind::Unknown` ONLY if matching genuinely needs it; prefer kind-insensitive
matching for user input, since users type spellings, not kinds: matching compares
text IDs where kind is unknown. Record whichever choice is made in the Decision
Log with the reasoning). Then migrate, in order: the default `IAnalyzer::parent_of`
(segment pop instead of separator scan); `enclosing_owner_chain` callers that
currently split fq strings; `resolve_qualified_name_in_shrinking_scopes` (compose
candidate FqNames by push instead of `format!("{scope}.{reference}")` — this deletes
the issue-1162 reference-normalization shim and the verbatim-scope workaround
because both sides are now segments); the anchor splitter in
`src/searchtools/selectors.rs` (a selector parses into an optional Path-kind prefix
plus symbol segments; the "does the left side name a real file" resolver check
remains as the semantic validation, but the `.r`-lookalike heuristics go). Each
migration step keeps the old string path compiling and the full test suite green;
behavior must not change in M2 (the regression suites named in Purpose are the
canaries).

M3 flips persistence. Schema migration `migrations/cache/00NN-fq-segments.sql`
(next free number; register in `src/cache_db.rs` per the existing migration
pattern): store segments as a compact serialized column on `code_units` — a single
TEXT/BLOB column holding length-prefixed or JSON-array `(kind, text)` pairs; keep
`short_name`/`package_name` columns populated (they remain useful for indexes and
human inspection) but the structured column becomes authoritative on load. Append
one salt token to EVERY language's `SALT` in `src/analyzer/store/epoch.rs`
(`;fq-interned-segments-2026-07`) since load-side interpretation changes for all
persisted rows. On load, segments are interned into the process interner and the
`FqName` is attached; the legacy-string derivation of structure (any remaining
split-based parsing of stored names) is deleted. C++'s stored `::`-headed
package_name strings stop mattering: cpp reads/writes segments like everyone else,
which is issue 1163's root fix.

M4 retires inference and locks the door. Delete remaining separator-split call
sites in `src/analyzer` (the ~227 count from Purpose is the worklist; each is either
migrated to FqName ops or documented as legitimately operating on non-name text).
Add the guard: a test (e.g. `tests/no_stringly_name_parsing.rs`) that walks
`src/analyzer` source files and fails on banned patterns (`split("::")`,
`split('.')` and friends) outside an explicit allowlist file — the mechanical
enforcement of the existing CLAUDE.md rule against separator mini-parsers. Flip the
issue-1163 pins: `cpp_qualified_nested_namespace_type_current_behavior` in
`tests/issue_1162_separator_aware_enclosing_scope.rs` now asserts RESOLUTION of the
sibling-namespace shape, and the two pinned `boundary_unchecked` sites in
`src/analyzer/usages/get_definition/cpp.rs` (near the strengthened NOTEs from the
issue-1162 landing) become live `gated_boundary` closures. Close issue 1163 with
that evidence.

## Concrete Steps

Work in the repository root (`/home/jonathan/Projects/bifrost2` or a worktree).
After every milestone (and every M1 language):

    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python --lib
    BIFROST_SEMANTIC_INDEX=off cargo test --features nlp,python \
      --test get_definition_test --test searchtools_service \
      --test searchtools_definition_selectors \
      --test issue_1128_rust_raw_identifiers \
      --test issue_1162_separator_aware_enclosing_scope \
      --test mcp_property_fuzzer_service

(plus the touched language's analyzer/usages suites in M1; plus the FULL suite before
the M3 and M4 pushes — `default = []`, so a featureless `cargo test` silently skips
the nlp-gated integration suites; always pass `--features nlp,python`). Commit each
language/milestone separately with why-focused messages. Isolated builds for
validation experiments go through `scripts/with-isolated-cargo-target.sh`.

## Validation and Acceptance

M0: `cargo test --lib fq_name` passes; the measurement test prints interner entries
and byte totals for a large fixture (expect interned bytes well under the summed
legacy string bytes; record the numbers in Surprises & Discoveries).

M1 (per language): the language's full suites pass AND the equivalence assertion
never fires across them — meaning every constructed unit's structured form joins to
exactly the legacy string. A deliberately-broken push (wrong kind or missing segment)
must fail loudly in tests; verify once by mutation before trusting the assertion.

M2: zero behavior change — the six named regression suites pass unmodified; the
issue-1162 shim deletion is proven safe by `issue_1162_separator_aware_enclosing_scope`
staying green.

M3: after the salt bump, a warm workspace re-extracts (observe via the analyzer
persistence tests, `tests/analyzer_persistence*`); all suites green under
`--features nlp,python`.

M4: `tests/no_stringly_name_parsing.rs` passes (and demonstrably fails when a banned
split is introduced — verify once by mutation); the flipped cpp test asserts the
sibling-namespace type RESOLVES where it previously pinned a false boundary — that
flip is the user-visible payoff and closes issue 1163.

## Idempotence and Recovery

Every milestone is additive until M3; reverting any M0–M2 commit returns to a green
tree because legacy strings remain authoritative. M3 is the one-way step: it ships a
schema migration plus a salt bump, and recovery is re-running extraction (the store
is a cache; deleting `.brokk/bifrost_cache.db` is always safe). Never hand-edit the
migration after it lands; add a new one. Use unique scratch patch paths (not
/tmp/fix.patch — it gets clobbered across parallel agents) for any fail-before
verification dances.

## Artifacts and Notes

The delimiter-bug evidence file motivating this plan, for posterity: issues 1128,
1131, 1162, 1163, the `.r`-anchor misroute fixed inside 1128's landing, and the
Scala `$` spelling inconsistency noted in issue 1126's closing comment. The
consolidated chokepoints that make the migration cheap were landed by the 2026-07
cross-language duplication campaign (see `.agents/docs/cross-language-duplication-survey.md`,
whose backlog items 1–6 are all landed on master as of 2026-07-24).
