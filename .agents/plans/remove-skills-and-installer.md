# Remove the obsolete skill installer

This ExecPlan follows `.agents/PLANS.md`. It removes Bifrost's generic Agent
Skill distribution path and the `bifrost --install-skills` command. The MCP
server, LSP server, native launcher, and standalone reviewer-agent files stay
available. After this change, Bifrost will no longer claim to install or ship
skills through the CLI or plugin manifests.

## Progress

- [x] (2026-08-10) Inspect the CLI, plugin, generated bundle, test, and documentation references.
- [x] (2026-08-10) Remove the Rust installer, CLI flags, skill assets, generated bundles, and generators.
- [x] (2026-08-10) Update plugin manifests, package tests, release checks, and documentation.
- [x] (2026-08-10) Run focused Rust and Node validation and inspect the final diff.
- [x] (2026-08-10) Commit the scoped changes on the current branch.

## Surprises & Discoveries

- The CLI installer embeds twelve Markdown files from `plugins/bifrost-agent/skills`.
- The plugin has three copies of the skill surface: canonical skills, generated Codex skills, and an Amp bundle.
- The plugin MCP and LSP configurations do not require skill files. They can remain after the removal.

## Decision Log

- Decision: Remove all Bifrost skill assets associated with the installer, including canonical, Codex, and Amp bundles.
  Rationale: The user requested removal of skills and `install-skills` before redesigning the interface.
  Date/Author: 2026-08-10 / Codex.
- Decision: Keep `plugins/bifrost-agent/agents`, MCP configuration, LSP configuration, launcher, and Pi extension.
  Rationale: These are separate host integration components and do not depend on the CLI skill installer.
  Date/Author: 2026-08-10 / Codex.

## Outcomes & Retrospective

The final commit will remove the broken skill installation path, leave direct
CLI and MCP operation intact, and leave the remaining host integration package
with no skill manifest entries. The next skill design can choose a CLI workflow
or an MCP workflow without preserving the current mixed interface.

## Context and Orientation

`src/skill_install.rs` embeds and copies skill Markdown files. `src/bin/bifrost.rs`
parses and dispatches `--install-skills`. The plugin manifests advertise skills
to Claude Code, Codex, Cursor, and Pi. The Codex and Amp bundles are generated
from the canonical skill directory. Tests and release checks verify these
files, while host documentation describes their installation.

## Plan of Work

Delete `src/skill_install.rs`, remove its module and CLI state, and remove the
skill-install integration test. Delete canonical, generated Codex, and Amp
skill files and their generators. Remove skill entries from plugin manifests
and package tests. Keep the MCP and LSP checks.

Remove skill-only documentation sections and replace any remaining claims with
the direct MCP or CLI interfaces. Remove release and package checks that derive
archives from skill files. Keep the normal workspace package and plugin checks.

## Concrete Steps

Run all commands from `/Users/jonathan/Projects/bifrost`.

Use `apply_patch` for source and documentation edits. Use explicit file paths
for deletion. Stage only files changed by this plan.

## Validation and Acceptance

The source must compile without `skill_install` or `--install-skills` symbols.
`bifrost --help` must show `--tool`, `--mcp`, and `--lsp`, but not
`--install-skills`. The plugin package and manifest checks must pass without
skill entries. Documentation and scripts must contain no obsolete skill paths.

Run focused Rust tests for the CLI suite and the relevant Node package checks.
Run formatting checks when practical. Review `git diff --check` and the final
status before committing.

## Idempotence and Recovery

The deletion is intentional and version-controlled. Before staging, inspect
the complete file list. If a file is outside the skill installer or generated
skill bundles, leave it unchanged. Recovery is available through Git history.

## Artifacts and Notes

The plan file records the removal scope. It is part of the agent-owned planning
namespace and remains with the change for future redesign work.

## Interfaces and Dependencies

The retained interfaces are the existing CLI one-shot tool mode
`bifrost --tool`, MCP server mode `bifrost --mcp`, LSP mode `bifrost --lsp`,
and the plugin launcher under `plugins/bifrost-agent/bin`.
