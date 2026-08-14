# Recover C keyword argument arity

This ExecPlan is a living document maintained under `.agents/PLANS.md`. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

## Purpose / Big Picture

Plain-C identifiers that collide with C++ keywords must remain usable as call arguments even though Bifrost intentionally parses C with tree-sitter-cpp. After this fix, a keyword token displaced from an enclosing C function parameter can contribute one structurally proven call argument without turning arbitrary parser errors into guessed arity.

## Progress

- [x] (2026-08-14) Reduced libarchive `__archive_mktempx` bytes 10026..10043, traced call arity 1 versus candidate arity 2, searched open issues, and filed self-assigned #2144.
- [x] (2026-08-14) Added C-only structured displaced-parameter argument recovery to exact and macro-aware call arity.
- [x] (2026-08-14) Added low-level and InlineTestProject positive/near-miss coverage for bound, unbound, and C++ keyword uses.
- [x] (2026-08-14) Ran focused and broad gates and replayed the production row from the clean pushed head. Focused tests, all 91 C/C++ crate tests, all seven callable-activation controls, formatting, focused Clippy, dependency validation, and the exact production replay pass.
- [x] (2026-08-14) Committed and pushed implementation commit `15e5ef579`; preserved compact checksummed clean-head evidence. Issue closure follows the evidence-plan commit.

## Surprises & Discoveries

- Observation: the declaration remains correctly indexed with signature `(const char *, wchar_t *)`; only call-site arity is wrong.
  Evidence: a temporary resolver trace reported `call_arity=Some(1)` and candidate `CallableArity { required: 2, total: 2, repeated: false }`.
- Observation: tree-sitter-cpp makes the second argument a direct extra `ERROR` containing the comma and the unnamed `template` keyword token.
  Evidence: XML CST for `__archive_mktempx(NULL, template)` contains `null` followed by `ERROR(',', template)`; `argument_children` deliberately filters extra nodes.

## Decision Log

- Decision: recognize the argument only when the same keyword token is structurally proven as a displaced unnamed parameter of the enclosing function in a `.c` file.
  Rationale: the enclosing parameter-list `ERROR(template)` immediately follows a nameless `parameter_declaration`, while the call-site `ERROR(',', template)` supplies the matching use. This uses CST relationships, needs no keyword list or source parser, and rejects unrelated malformed expressions and C++ files.
  Date/Author: 2026-08-14 / Codex

## Outcomes & Retrospective

Implementation commit `15e5ef579` is pushed to `origin/master`. The clean release runner SHA-256 is `96505cf30fca9f949263c300d285ebe32452a790f7698c26e0eb3580f85945df`. The exact libarchive row completed as resolved and consistent with an exact inverse hit, actionable zero, no skipped or truncated targets, no candidate-limit overflow, no inverse-precision findings, and no file errors. The compact manifest is `/mnt/optane/tmp/bifrost-fird/final-15e5ef57/issue-2144-clean-replay-manifest-15e5ef57.jsonl` (SHA-256 `b1b7379360da271dd1d6e2a18cac243b30cef13dc25ece08a0d61c6dca590376`); its checksum inventory has SHA-256 `dbde2dcecc710c510b26ab482d85aa5db6794beca9e878e62856955da8df1591`. The purpose is met.

## Context and Orientation

`crates/bifrost-cpp/src/graph/resolver.rs::VisibilityIndex::call_arity_evidence` counts `argument_children`, which excludes extra CST nodes. `__archive_mktempx(NULL, template)` therefore counts only `NULL`. The enclosing wrapper parameter has the corresponding structured split: `parameter_declaration(wchar_t *)` followed immediately by `ERROR(template)`.

## Plan of Work

Add a small resolver helper that, for `.c` files only, finds keyword tokens displaced from the enclosing function parameter list and counts direct call argument errors shaped as one comma plus the same leaf keyword token. Add that proven count to both the fast exact-arity path and the macro-aware path. Do not change generic `argument_children` or reinterpret arbitrary errors.

Add a low-level CST test for the recovered and rejected shapes. Register an InlineTestProject regression with the resolving `helper(tmpdir, NULL)` control, the formerly missing `helper(NULL, template)` call, an unrelated malformed keyword expression, and a `.cpp` near miss.

## Concrete Steps

From `/mnt/optane/bifrost-fird`, run:

    cargo test -p brokk-bifrost-cpp c_keyword_argument
    cargo test --test suite_issues -- issue_2144
    cargo test --test suite_analyzers -- cpp_callable_activation_visibility
    cargo test -p brokk-bifrost-cpp
    cargo fmt --all -- --check
    scripts/with-isolated-cargo-target.sh cargo clippy -p brokk-bifrost-cpp -p brokk-bifrost-analysis --all-targets -- -D warnings
    node scripts/check-workspace-dependencies.mjs
    git diff --check

Then rebuild `bifrost_reference_differential` in release mode and replay libarchive `archive_util.c` bytes 10026..10043 with `--cache-mode ephemeral`.

## Validation and Acceptance

The C keyword-parameter call and ordinary control must resolve. The malformed and C++ near misses must remain unresolved. The production replay must be `completed`, `resolved`, `consistent`, inverse-exact, free of truncation and file errors, and exit with actionable zero.

## Idempotence and Recovery

Tests are read-only and replays use ephemeral caches. Preserve raw outputs until a compact checksummed clean-head manifest exists. Retry interrupted runs to a new revision-specific output file.

## Artifacts and Notes

The current exact failure is `/tmp/fird-libarchive-mktempx-current.jsonl`; the current-head confirmation is `/tmp/issue-mktempx-second-5a115bd8.jsonl`. The successful first-call control is `/tmp/issue-mktempx-first-5a115bd8.jsonl`.

Plan revision note (2026-08-14): Created after closing #2142 and tracing the last remaining libarchive tier-1 row.

Plan revision note (2026-08-14): Recorded implementation commit, complete local gates, and checksummed clean pushed-head production evidence.

## Interfaces and Dependencies

Change resolver-only C call-arity recovery and tests. Add no dependency, schema, epoch, identity, or generic C++ argument-list change.
