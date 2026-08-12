# Keep inverse hits from unindexed files

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current. Maintain this plan according to `.agents/PLANS.md`.

## Purpose / Big Picture

An editor can ask for references in an open file that Bifrost did not index. A very long source line can cause this state. The usage scanner can still parse the file and find a reference. Seven language hit builders now discard that reference because no indexed declaration encloses it.

After this change, Go, Ruby, PHP, Java, and Kotlin keep that reference. They attribute it to `CodeUnit::file_scope`. Indexed files continue to use their real enclosing declaration. The audit records why C# and Python cannot reach the hit builder from this editor path.

## Progress

- [x] (2026-08-12) Confirmed the silent empty-enclosing return in all seven hit builders.
- [x] (2026-08-12) Added one cross-language behavior test for unindexed and indexed candidate files.
- [x] (2026-08-12) Used the shared file-scope identity in the five reachable hit builders.
- [x] (2026-08-12) Confirmed C# rejects the same file in `prepare_file` before hit construction.
- [x] (2026-08-12) Confirmed Python has no indexed import edge for the rejected file, so target binding stops before hit construction.
- [x] (2026-08-12) Ran the #1788 and #1785 tests, formatting, and targeted Clippy.
- [ ] Commit, integrate origin/master, push, close #1788, and record the final result.

## Surprises & Discoveries

- Observation: Indexed files already have a synthetic file-scope declaration.
  Evidence: `crates/bifrost-core/src/analyzer/parsed_file.rs` calls `ParsedFile::add_file_scope`, and store hydration restores this declaration.

- Observation: The missing state comes from an explicit editor candidate that has no indexed file state.
  Evidence: `tests/suite_usages/issue_1785_js_file_scope_enclosing.rs` reproduces the same state for JavaScript with a line longer than `DEFAULT_MAX_LINE_LENGTH`.

- Observation: C# applies `is_unparseable_source` again in `prepare_file`.
  Evidence: The explicit long-line candidate never reaches `crates/bifrost-csharp/src/graph/hits.rs`. The query returns no C# hit before and after an empty-enclosing fallback.

- Observation: Python needs an indexed import edge before it can bind a target from another file.
  Evidence: The rejected candidate has no import edge. Its parsed `from target import target` never becomes a target seed, so `build_hit` does not run.

## Decision Log

- Decision: Use `CodeUnit::file_scope(file.clone())` when `enclosing_code_unit` returns `None`.
  Rationale: This is the same identity that indexed files already use. It keeps attribution honest without inventing a language-specific owner.
  Date/Author: 2026-08-12, Codex.

- Decision: Test the production `UsageFinder` path with an `ExplicitCandidateProvider`.
  Rationale: A direct unit test of the hit helper would not prove that an editor request retains the result.
  Date/Author: 2026-08-12, Codex.

- Decision: Do not change the C# hit builder.
  Rationale: The measured production path stops at the shared parse limit. A fallback in the hit builder cannot change behavior and has no valid acceptance test.
  Date/Author: 2026-08-12, Codex.

- Decision: Do not change the Python hit builder.
  Rationale: The measured query stops in target binding before hit construction. A file-scope fallback cannot change the result.
  Date/Author: 2026-08-12, Codex.

## Outcomes & Retrospective

The implementation is complete. Go, Ruby, PHP, Java, and Kotlin now retain references from explicit unindexed candidates. Each hit uses the shared file-scope identity. Indexed controls keep their callable owner. C# and Python stop before hit construction, so their builders did not change.

The #1788 test passed for all five reachable languages. Both #1785 JavaScript controls passed. Formatting, five language-crate Clippy checks, and `suite_usages` Clippy passed with warnings denied. Commit, integration, push, and issue closure remain.

## Context and Orientation

`UsageFinder` scans candidate files for references to indexed `CodeUnit` targets. A `UsageHit` must contain an `enclosing` declaration. The editor can give `UsageFinder` a file that the analyzer rejected. In that file, `CodeUnitIndex::enclosing_code_unit` returns `None`.

The reachable affected hit builders are:

- `crates/bifrost-go/src/graph/hits.rs`
- `crates/bifrost-ruby/src/graph/hits.rs`
- `crates/bifrost-php/src/graph/hits.rs`
- `crates/bifrost-jvm/src/java/graph/hits.rs`
- `crates/bifrost-jvm/src/kotlin/graph/hits.rs`

The C# builder has the same empty return. Its query preparation applies the analyzer parse limit again. The long-line editor candidate cannot reach that return.

The Python builder also has the empty return. Its query needs indexed import edges before it can bind a different-file target. A rejected file has no such edge.

Each builder already receives the current `ProjectFile`. `CodeUnit::file_scope` needs only that file. No source-text parsing or new model is necessary.

## Plan of Work

Add `tests/suite_usages/issue_1788_non_js_file_scope_enclosing.rs`. Use `InlineTestProject` for each language. Put a valid target in one indexed file. Put a reference in one normal caller and one caller with a 20,000-character comment line. Confirm the analyzer has no declarations for the long-line file. Query both callers through `ExplicitCandidateProvider`.

The long-line reference must appear in the query result. Its enclosing identity must equal `CodeUnit::file_scope`. The normal caller reference must keep its indexed callable owner.

Replace each reachable early return caused only by a missing enclosing unit with the shared file-scope identity. Apply the same rule to proven and unproven hit channels. Keep all existing self-reference and usage-limit rules. Leave C# and Python unchanged because their earlier gates make this state unreachable.

## Concrete Steps

Run these commands from `/mnt/optane/bifrost-fird`:

    cargo test --test suite_usages issue_1788 -- --nocapture
    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-go -p brokk-bifrost-ruby -p brokk-bifrost-php -p brokk-bifrost-jvm --all-targets -- -D warnings
    cargo clippy --test suite_usages -- -D warnings
    git diff --check

The focused test must fail before the hit-builder changes. It must pass after them.

## Validation and Acceptance

Acceptance needs all five reachable languages. Each unindexed caller must produce a reference hit with the exact file-scope identity. Each indexed caller must produce a hit with a real callable enclosing unit. The audit must record the C# and Python boundaries. The focused test, formatting, and targeted Clippy must pass.

## Idempotence and Recovery

The test uses temporary projects and leaves no files behind. Cargo commands can run again. Stage only this plan, the new test, its suite registration, and the five hit-builder files.

## Artifacts and Notes

The JavaScript precedent is `tests/suite_usages/issue_1785_js_file_scope_enclosing.rs`. The new test uses the same production path and extends it to the languages named in #1788.

## Interfaces and Dependencies

Do not add a new interface. Use the existing constructor:

    CodeUnit::file_scope(file.clone())

Do not add a string or regular-expression fallback. Use only the existing indexed target, parsed source, and shared file-scope model.

Plan revision, 2026-08-12: Created after the seven-language audit confirmed one shared model identity.

Plan revision, 2026-08-12: Removed C# from the implementation after a production-path test proved its scanner stops at the parse gate.

Plan revision, 2026-08-12: Removed Python after the production query proved that missing import edges stop target binding before hit construction.

Plan revision, 2026-08-12: Recorded the completed implementation and focused validation before delivery.
