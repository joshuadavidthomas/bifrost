# Use Nested Python Project Roots for Module Identity

This ExecPlan is a living document. Keep the `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` sections current.

Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

A workspace can contain a Python project below the workspace root. Python imports start at that nested project's import root. Bifrost currently includes the parent directory in indexed names when that parent also has `__init__.py`.

After this change, Bifrost will read supported `pyproject.toml` package discovery metadata. It will use that import root for declaration, import, navigation, and inverse identities. The GlobaLeaks call to `globaleaks.handlers.user.operation.disable_2fa` will resolve to the indexed workspace function.

## Progress

- [x] (2026-08-13 05:31Z) Reproduced the exact GlobaLeaks failure at commit `e712be841713eaf756397074d91d535eed017ae8`.
- [x] (2026-08-13 05:35Z) Located the shared path-derived identity in `python_module_components`.
- [x] (2026-08-13 05:48Z) Added a failing inline project-root regression with a same-name member near miss.
- [x] (2026-08-13 06:02Z) Read supported nested `pyproject.toml` import-root metadata in the shared module identity function.
- [x] (2026-08-13 06:02Z) Applied one computed structured module identity during each declaration walk.
- [x] (2026-08-13 06:02Z) Bumped the Python store epoch because persisted names change.
- [x] (2026-08-13 06:14Z) Ran focused tests, formatting, targeted Clippy, and the exact GlobaLeaks replay.
- [ ] Commit, pull, push, and close issue 1971.

## Surprises & Discoveries

- Observation: GlobaLeaks has `backend/__init__.py` and `backend/globaleaks/__init__.py`.
  Evidence: The current `__init__.py` heuristic selects `backend` as the highest package. It therefore indexes `backend.globaleaks...` even though `backend/pyproject.toml` declares `where = ["."]` and imports use `globaleaks...`.

- Observation: The exact current lookup still returns `no_definition`.
  Evidence: At `backend/globaleaks/handlers/admin/operation.py:340:16`, the diagnostic says `disable_2fa` is bound locally but has no indexed Python definition.

- Observation: Correcting the module root first exposed a second ownership defect. Export lookup returned the top-level function and a same-name class method from the imported module.
  Evidence: The intermediate exact result was `ambiguous`. It returned `globaleaks.handlers.user.operation.disable_2fa` and `globaleaks.handlers.user.operation.UserOperationHandler.disable_2fa`.

- Observation: `top_level_declarations` can contain physically nested declarations. Export lookup filtered only by identifier.
  Evidence: Requiring each local export candidate's indexed parent to be the source module removes the class method. Existing reexport tests still pass.

## Decision Log

- Decision: Support the structured `[tool.setuptools.packages.find].where` array in the nearest ancestor `pyproject.toml`.
  Rationale: It directly defines the import roots for the exact project. It also supports common `where = ["src"]` layouts. An unrelated `pyproject.toml` without this package metadata must not change module names.
  Date/Author: 2026-08-13 / Codex

- Decision: Parse TOML with the workspace `toml` crate. Do not scan manifest text.
  Rationale: The manifest already has structured data. Source-text matching would accept comments and invalid shapes.
  Date/Author: 2026-08-13 / Codex

- Decision: Compute the structured module identity once for each declaration walk.
  Rationale: Declaration collection creates many symbols per file. It must not reread and reparse project metadata once per symbol.
  Date/Author: 2026-08-13 / Codex

- Decision: Enforce direct module ownership when selecting a local Python export.
  Rationale: A class member is not a module export only because its identifier matches. The indexed parent is the structured ownership fact.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The shared module identity now starts at a supported nested setuptools import root. Both `where = ["."]` and `where = ["src"]` work. An unrelated manifest keeps the prior `__init__.py` result.

The exact GlobaLeaks call resolves one target: `globaleaks.handlers.user.operation.disable_2fa` at `backend/globaleaks/handlers/user/operation.py:141`. The imported module's same-name class method no longer creates false ambiguity.

The two new issue tests pass. Existing forward reexport and analyzer import tests pass. Formatting and targeted Clippy pass for the Python crate, analysis crate, and issue suite.

## Context and Orientation

`crates/bifrost-python/src/declarations.rs` creates Python module and declaration identities. `python_module_components` currently uses `__init__.py` files to select a package root. Every import and usage path later reads those indexed module units, so the wrong root affects all navigation surfaces.

`ProjectFile` stores the workspace root and a workspace-relative path. A nested import root is therefore another workspace-relative path. For GlobaLeaks, the workspace-relative project directory is `backend`, and `where = ["."]` makes `backend` itself the import root. The module file `backend/globaleaks/handlers/user/operation.py` must render as `globaleaks.handlers.user.operation`.

`crates/bifrost-analysis/src/analyzer/store/epoch.rs` defines cache epochs. A Python identity change needs a new salt. This prevents old persisted rows from keeping the obsolete `backend.` prefix.

The end-to-end test belongs in `tests/suite_issues/`. It must use `InlineTestProject` and write a nested `pyproject.toml`, package files, an imported function, and an unrelated same-name class method.

## Plan of Work

First, add `tests/suite_issues/issue_1971_python_project_root.rs` and register it in the suite harness. The fixture will place `pyproject.toml` below the workspace root. It will also place `__init__.py` at that project directory to reproduce the misleading higher package. The call imported from `pkg.user` must resolve to `pkg.user.helper`. A class method named `helper` in the imported module must not replace the imported function.

Second, update `crates/bifrost-python/src/declarations.rs`. Walk from the source file's directory toward the workspace root. For each ancestor `pyproject.toml`, parse TOML and read `[tool.setuptools.packages.find].where`. Convert each valid relative entry into a workspace-relative import root. Select the nearest root that contains the source file. If no supported root exists, keep the current `__init__.py` behavior.

Third, keep one computed `FqName` in the declaration visitor. Reuse it for every top-level class, function, and field instead of calling the file-system module resolver for each declaration.

Fourth, add the `toml` workspace dependency to `crates/bifrost-python/Cargo.toml`. Add a Python epoch salt in `crates/bifrost-analysis/src/analyzer/store/epoch.rs`.

Finally, run focused tests and the exact GlobaLeaks command. Commit only the issue files, pull `origin/master`, push the current branch to `origin/master`, and close issue 1971 with exact evidence.

## Concrete Steps

Run all commands from `/mnt/optane/bifrost-fird`.

Run the new regression:

    cargo test --test suite_issues issue_1971_python_project_root -- --nocapture

Run related Python package and import tests selected after implementation. Run local gates:

    cargo fmt --all -- --check
    cargo clippy -p brokk-bifrost-python --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost-analysis --all-targets -- -D warnings
    cargo clippy -p brokk-bifrost --test suite_issues -- -D warnings

Build the CLI and replay the exact witness:

    cargo build --bin bifrost
    BIFROST_SEMANTIC_INDEX=off target/debug/bifrost --root /tmp/fird-issue1971-globaleaks --sources backend/globaleaks --tool get_definitions_by_location --args '{"references":[{"path":"backend/globaleaks/handlers/admin/operation.py","line":340,"column":16}]}'

The final result must have status `resolved`. Its definition must be in `backend/globaleaks/handlers/user/operation.py` with FQN `globaleaks.handlers.user.operation.disable_2fa`.

## Validation and Acceptance

The new inline test must fail before the implementation and pass after it. The same fixture must prove that an unrelated class method does not win.

Existing root-level packages and packages without supported `pyproject.toml` metadata must keep their current names. A `where = ["src"]` fixture must drop the `src` directory. An invalid or unrelated manifest must not change module identity.

The exact GlobaLeaks lookup must resolve the imported function. It must not return `no_definition`, an outside-workspace boundary, or a same-name class member.

## Idempotence and Recovery

The tests and CLI replay are safe to repeat. The CLI disables semantic indexing and does not download models. If the persisted cache affects a replay, remove only the GlobaLeaks `.bifrost` cache or use a new temporary clone. Do not remove repository data.

If a manifest is invalid, module identity must use the existing `__init__.py` fallback. The analyzer must not fail workspace construction because project metadata is malformed.

## Artifacts and Notes

Exact before result:

    GlobaLeaks commit: e712be841713eaf756397074d91d535eed017ae8
    Reference: backend/globaleaks/handlers/admin/operation.py:340:16
    Status: no_definition
    Diagnostic: disable_2fa is bound locally but has no indexed Python definition

The target source is `backend/globaleaks/handlers/user/operation.py:141`. Current symbol search labels it `backend.globaleaks.handlers.user.operation.disable_2fa`. The required identity removes only the configured import-root prefix.

Exact after result:

    Status: resolved
    FQN: globaleaks.handlers.user.operation.disable_2fa
    Path: backend/globaleaks/handlers/user/operation.py
    Line: 141

## Interfaces and Dependencies

Keep `python_module_fq(file: &ProjectFile) -> FqName` and `python_module_name(file: &ProjectFile) -> String` as the shared public identity functions.

Add only the existing workspace `toml` dependency to `brokk-bifrost-python`. Do not create a new crate. Do not add a source-text parser.

Plan revision note, 2026-08-13: Created the plan after the exact GlobaLeaks reproduction. It records the shared identity change, performance constraint, cache migration, tests, and acceptance evidence.

Plan revision note, 2026-08-13: Recorded the same-name export ownership defect, the module-parent fix, the completed local checks, and the exact resolved result.
