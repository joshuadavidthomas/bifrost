# MCP property fuzzer (oracle-free contract fuzzing of the searchtools surface)

**Status: the M4 acceptance campaign is complete through tier 7 (2026-08-13).** The
fuzzer is the permanent contract harness for the searchtools MCP surface; this file is
its operating runbook. Campaign history lives in git history (commit messages are the
source of truth) and in the committed ledgers under `.agents/plans/mcp-property-fuzzer/`.

## Purpose

Drive the same MCP tool surface agents use
(`src/mcp_core.rs::symbol_tool_descriptors(render_line_numbers)`, both modes), generate
queries from Bifrost's own index, and check self-consistency properties that need no
external ground truth. Every violation is self-evident from the responses in hand, so an
autonomous agent can triage, file, fix, and rerun mechanically. Failure signature =
`(invariant, language, tool, syntactic shape)`; the ledger dedupes on it, so a corpus
with ten thousand instances yields one entry. Shrinking means reducing a failing case
to the smallest reproduction that still violates the invariant (dropping batch
entries, trimming contexts) before it is recorded.

## The invariants

- **I1 — Range integrity.** For every indexed symbol: (a) a container's range contains
  its indexed members; (b) the text at the symbol's range contains its terminal name
  token; (c) `get_symbol_sources` returns text identical to the file content at the
  reported range; (d) a class declaration whose range ends adjacent to a large
  tree-sitter ERROR node is truncated-at-parse-error. Motivation: #1016.
- **I2 — Selector-form equivalence.** The spellings an agent plausibly writes (terminal,
  display fq, `path#terminal`, `path#display_fq`) must resolve consistently: a more
  specific spelling never fails where a less specific one succeeds; resolved spellings
  name the same declaration; single-entry and batched `get_definitions_by_reference`
  agree. Motivation: #1018.
- **I3 — Cross-tool round-trips.** (a) a symbol `get_summaries` lists under file F
  round-trips via `F#symbol`; (b) a symbol `scan_usages_by_reference` resolves appears
  in `search_symbols` for its terminal name; (c) no response both renders a target and
  reports it `not_found`. Also cross-cutting: structured payloads must not drift between
  render modes. Motivation: the doctrine/orm 483-call retry loop.
- **I4 — Diagnostic honesty.** A failure message must not claim non-indexing ("not
  indexed", "outside the indexed workspace", "external crate/module") when
  `search_symbols` finds an in-workspace declaration with the claimed name. Motivation:
  #1015.
- **I5 — Hint presence.** Every failure-status response must carry actionable next-step
  content (candidate list, note, or diagnostics); an empty refusal is a violation.
  Motivation: #1019.

I1(a)–(d) run in the engine phase and gate probe generation (range-invalid symbols are
excluded); I1(c) and I2–I5 run in the service phase through
`SearchToolsService::call_tool_output`, exactly as the MCP handler would.

## Operating a corpus pass

- Rebuild from current master at campaign start so a tier shares one analyzer
  generation: `cargo build --release --bin bifrost_mcp_property_fuzzer`.
- Standard run shape per repo: 5 shards x 200 service symbols —
  `--shard K/5 --max-service-symbols 200 --max-scan-probes 20 --jobs 4
  --cache-mode ephemeral --out <staging>.jsonl --dump-probes <staging>-dump.jsonl`.
  Fan out shard-level job lists via `xargs -P`; never whole-language fan-outs
  (memory-bounded: ~300 concurrent big-workspace shards OOMed 62G+15G in minutes on
  2026-07-29).
- `--cache-mode ephemeral` is required for campaign runs: `parse_errors` (I1(d)) is
  session-fresh only (warm caches silently disable the check), and persisted mode writes
  `.brokk/` state into the corpus clone.
- Local-copy-first: rsync the window's clones to `/tmp/local-clones` and point
  `--clones-root` at NVMe; never run against the NFS mount (latency wedges and outages
  cost days in tier 5). At most two language windows resident and `/tmp/local-clones`
  under 30G; a window over 30G (chromium/ClickHouse class) runs alone. Delete each
  repo's clone the moment its 5/5 records are merged and triaged — automate this in the
  merge step, not as a human chore. Backstop: a watchdog kills the youngest wave
  executor if `/` reaches 90%.
- Long shards: launch with task timeouts disabled (wrapper timeouts killed two end-run
  executors mid-shard in tier 6); renice whales.
- Key knobs: `--symbol-filter` / `--path-filter` (targeted confirmations),
  `--rerun <ledger-line>` (re-derives probes from the deterministic sample; prints
  `reproduced`/`MISSING` per signature and exits 2 on any MISSING — a disappeared
  violation is a signal, same severity as a new one), `--symbol-time-budget-ms`
  (per-fq cumulative wall-clock budget; over-budget probes are skipped unexecuted, feed
  no checker, and are counted as `calls_skipped_time_budget` in `probe_summary`;
  0 = unlimited, and the flag is what makes macro-family whales completable),
  `--repo-jobs` (cross-repo parallelism; memory is the binding constraint).
- Merge/survey/audit tooling is small campaign scripts rebuilt per tier (not committed);
  the semantics that must hold: repository records merge append-only, dedupe on
  `(repo_slug, run_fingerprint, report.config.shard.index)`; a run is complete when its
  (language, repo) has *completed* shard indices 1..5 — fingerprint-agnostic, because
  config legitimately changes mid-tier; triage coverage means every (repo_slug,
  signature) violation pair has a triage line; the strict closeout audit keys coverage
  on (window language, repo), never repo alone (dual-language repos run once per
  language — the skipper lesson).

## Triage workflow

- Language-at-a-time in task-count order; triage never accumulates across languages.
- Rerun-confirm every genuinely new signature (filtered `--path-filter`/`--symbol-filter`
  run) before filing; classify product-defect vs expected-behavior — a raw
  classification is a triage input, not proof.
- One GitHub issue per confirmed signature, `FUZZ:` prefix, shrunk repro verbatim;
  assign to `jbellis` when actively working the fix; fix directly on master with a
  thorough commit message (the source of truth) containing `Fixes #N`; push each fix as
  it lands; confirm the signature gone via rerun before moving on.
- Generality: a defect affecting several languages gets one root-cause fix, never
  per-language point fixes.
- Escalation: an architecturally complex fix (significant design work, or huge blast
  radius) gets a comment explaining the problem and trade-offs, assignment to David
  (`DavidBakerEffendi`), and the campaign moves on — no open-ended architecture work
  inside the campaign.
- Record every disposition as a triage line in the tier ledger (issue link, fuzzer-fix
  commit, or escalation).

## Corpus and ranking

- Clones: `/mnt/minasmorgul/repo-clones` (via the
  `/home/jonathan/Projects/brokkbench/clones` symlink).
- **The canonical corpus ranking is the committed frozen snapshot
  `.agents/plans/mcp-property-fuzzer/corpus-ranking.json`** (generated 2026-08-13,
  11 languages, 528-1,691 repos each; top anchors verified against tier-1 records).
  Window construction needs only this file plus the committed ledgers: take the
  language's ordered list, subtract every (language, repo) already recorded in
  `m4-tier*.jsonl`, and the next N entries are the window. Running a campaign this way
  has no brokkbench dependency at all.
- Regenerate the snapshot only when the corpus materially changes (new clones, new SFT
  task data): `brokkbench/.venv/bin/python3 scripts/mcp-fuzzer-repo-rank.py
  --commits-root brokkbench/sft-tools-commits > <the snapshot path>`. The derivation it
  freezes: per-repo task count = `tasks.sft_count_for_repo` from `brokkbench/tasks.py`,
  accessed through its venv python helper (never parse task files manually — enforced
  by test). Ties: raw scan-record count, then slug. Languages where every sft_count is
  zero (scala) fall back to raw scan count. **Scala is exhausted at rank 84** — no
  further scala widening exists.
- The fuzzer binary's corpus-mode selection still shells out to the brokkbench helper
  at runtime; campaign runs bypass it entirely by driving explicit `--repo` lists from
  the frozen windows.
- Absent or broken clones: substitute the next ranked unprobed repo from the snapshot
  and record the substitution in the tier closeout.

## Known signature families

Fixed families — a reappearance is a regression; rerun-confirm and reopen: #1057
silent-preference/twins (the dominant historical class: build-tag, version-tree,
fixture, and partial-class duplicates), #1059 sigil scan-absence, #1063 C# generic
arity, #1092 C header/source duality, #1093 display-vs-resolution separators, #1126
import-path gaps, #1016 parse truncation, #1194 csharp census explosion, #1336
render-mode drift race, #1347 rust alias-chain stack overflow, #1431 CR-only-line-ending
source text, #1524/#1566/#1573 (cpp whale-class recovery/reconcile/panic),
#1689/#1698/#1707 (phalcon/chromium memory, reparse, hydration).

Open families — expected to keep firing; triage new instances as known-issue with an
exemplar note, no new issue: **#1775** (I1 source-text-differs: `#`-directive comment
overrun; interior/mid-line mismatch in class blocks), **#1927** (phalcon-class per-call
latency: per-request TU re-parse in `CppIdentityRenderCache` + row-by-row
`hydrate_file_state_with_source`; minutes-to-hours per call on macro-heavy generated
C), **#1928** (chromium `SegmentInterner` resolve/intern + park/unpark churn).

## Checker conventions (expected behavior; do not re-file)

- I1(a) does not containment-check `Class` parents in rust/go/c/cpp: `impl` blocks,
  receiver methods, and out-of-line `Foo::bar` declare members outside the type's
  range by design. Callable parents are checked everywhere.
- I1(b) checks the *terminal* segment of the display name agents see. Skips: module
  units (named after their file), auxiliary constructors (indexed under the class
  name), Go blank identifiers (`pkg._module_._`, unaddressable), and non-identifier
  names (`operator<<`, `<init>`). Nested/local classes display `$`-joined.
- I1(c) compares line-affixes (interior lines exact; first text line a suffix of the
  file's start line; last a prefix of the end line). Blocks with `note`/`presentation`
  (file outlines, `#include` listings) use synthetic coordinates and are skipped. Go
  embedded-field blocks deliberately re-insert the `type` keyword — accepted.
- Ambiguity is encoded differently per surface: `get_symbol_sources`/`get_summaries`
  use a structured `ambiguous` array; `get_definitions_by_reference` uses
  `status: "not_found"` plus `diagnostics[].kind == "ambiguous_symbol"`. Read the
  structured `kind`, never message text.
- I3(a) skips module/excerpt (their "path" is a convention) and include/import elements
  (they name file paths, not symbols); the listing's own anchored selector counts as
  resolvable; ambiguity offers windowed at the 25-match cap are undecidable.
- I3(b) skips truncated search result sets (absence from an incomplete set is
  unverifiable) and module scan targets; follow-up searches set `include_tests: true`.
- I4 tests the name the message *claims* is unindexed (the backticked subject), with
  qualification-aware contradiction; import-namespace role assertions are excluded.
- Scan probes run single-mode (the line-numbers-on surface replaces the tool with its
  `_by_location` variant); every other probe runs both render modes. A mode-drift pair
  where either side reports wall-clock partiality is incomparable, not a violation.

## Operational lessons

- **Big-vs-pathological is decidable cheaply.** Healthy giants burn a steady ~1.5-2
  cores and produce dump records at a steady rate; pathological families accelerate
  per-symbol cost and never produce. A 20-symbol cost probe settles any repo in ~5
  minutes.
- **Do not kill steady-rate shards regardless of CPU-hours.** Kill only what is
  provably flat: per-thread CPU deltas over 60-120s; on NFS, per-thread syscall
  progress (`/proc/<pid>/task/<tid>/syscall` twice, 30s apart).
- **`--jobs 4` captures essentially all single-process probe throughput** (flat curve
  2-24 after the #1054 WAL pool). `--shard`'s value is fault isolation and memory
  bounding; scale out with `--repo-jobs`/shard job lists.
- **Audit at every wave end**: diff the job list against landed records (silent
  stragglers happen), keyed on (language, repo).
- **Sibling shards are a free control group.** Same repo, same config, deterministic
  same-size slices: when shard 3 takes 3 days while shards 1/2/5 took 11 minutes, that
  is proof of symbol-specific pathology, not repo bigness — compare before profiling.
- **Probe latency is ~100x bimodal across tools on the same symbol.** On phalcon's
  `PHP_METHOD`: `get_definitions_by_reference` 26s, `get_symbol_sources` 38-61 min (the
  render path re-parses the TU). Cost-explore with definitions probes; verify with
  sources probes. Never estimate a family's cost from the cheap tool.
- **Budget-skip makes the dump lie by silence.** Skipped probes write no records, so a
  flat record count mid-run means either "slow wave grinding" or "skips flushing" —
  only the end-of-run `probe_summary` counters disambiguate. And the budget stops only
  *unstarted* probes: each pathological family's first worker wave always runs to
  completion, so any budget below the family's median probe time is equivalent — pick
  the value on trip-speed grounds only.
- **The merge step eats anything matching the staging glob.** `<tier>-*-shard*.jsonl`
  is swept into the ledger with no human in the loop; racing/duplicate attempts must
  get staging names that do not match, or they merge silently.
- **stime >> utime is the "do not add jobs" signal.** When kernel time dominates user
  time (futex/lock churn), the run is contention-bound and more workers buy nothing.
  Read `/proc/<pid>/stat` fields 14/15 on the *fuzzer* process — the task pid is a bash
  wrapper whose own %CPU reads zero while the grandchild burns N cores.

## Campaign record

M1-M3 (2026-07-21/22): harness built — engine + service phases, ledger, shrink/rerun,
dual-mode probing, parallel probes. M4 tiers, all complete (ledgers under
`.agents/plans/mcp-property-fuzzer/`): tier 1 (11 repos), tier 2 (55 repos), tier 3
(275 records), tier 4 (550), tier 5 (1,100), tier 6 (2,170), tier 7 (1,986 records /
397 runs / 283 triage lines / 35 distinct signatures, zero untriaged; corpus now covers
the top-120 repos per language by task count, scala exhausted at 84). Per-tier fixes,
escalations, and narratives: git history (`Fixes #N` commits are the source of truth)
plus each tier's closeout commit.
