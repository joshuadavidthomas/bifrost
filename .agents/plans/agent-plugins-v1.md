# Adopt Agent Plugins v1 and prove it with headless clients

This ExecPlan is a living document. Maintain `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` as work continues. Follow `.agents/PLANS.md` from the repository root.

## Purpose / Big Picture

Bifrost currently ships a shared plugin directory, but its manifests are specific to Codex, Claude Code, Cursor, Pi, and Amp. After this work, the same directory will also be a valid Agent Plugins v1 package. A compatible client will discover Bifrost skills from `skills/` and start the Bifrost MCP server from one portable `mcp.json` file.

The result must work without a graphical desktop session. The normal checks will validate the package structure and execute a raw MCP conversation. Opt-in headless client checks will install the package into isolated Codex CLI and GitHub Copilot CLI homes. They will prove package discovery and, when credentials are present, invoke a read-only Bifrost tool against a disposable workspace.

## Progress

- [x] (2026-08-06 18:23Z) Read Agent Plugins v1 requirements and current Bifrost plugin packaging.
- [x] (2026-08-06 18:23Z) Confirmed that Codex CLI, Cursor, and VS Code are installed. Confirmed that GitHub Copilot CLI and Kiro are not installed.
- [x] (2026-08-06 18:23Z) Created GitHub issue #1737 with the implementation scope.
- [x] (2026-08-06 18:23Z) Wrote this ExecPlan for a portable package and headless validation.
- [x] (2026-08-07) Add the portable manifests and preserve host adapters.
- [x] (2026-08-12) Replace the legacy Codex manifest and `.mcp.json` with the
  portable root package. Keep only host adapters that require client-specific
  fields.
- [x] (2026-08-07) Add deterministic structural checks and raw-MCP release smoke coverage.
- [x] (2026-08-07) Run the Codex CLI and Cursor Agent CLI discovery checks in isolated temporary directories.
- [x] (2026-08-07) Run final structural, package-content, release-smoke, and policy validation. Record results and update this plan.

## Surprises & Discoveries

- Observation: `plugins/bifrost-agent/mcp.json` is a Cursor adapter, not a portable Agent Plugins file.
  Evidence: It uses `${CURSOR_PLUGIN_ROOT}` plus `startup_timeout_sec` and `tool_timeout_sec`. Agent Plugins v1 accepts neither timeout field in `mcp.json`.

- Observation: Agent Plugins v1 has a deliberately small portable core.
  Evidence: The fixed component locations are `skills/<name>/SKILL.md` and root `mcp.json`. Agents, hooks, LSP servers, UI data, and marketplace data are client-owned extensions or adapters.

- Observation: GitHub Copilot CLI can install a plugin from a local directory without a graphical client.
  Evidence: Its documented command is `copilot plugin install /abs/path`. It supports an isolated configuration through `COPILOT_HOME`.

- Observation: Current Copilot documentation still lists `.mcp.json` as its legacy MCP path.
  Evidence: Its Agent Plugins v1 support must be proven with the portable root `mcp.json`; do not assume the legacy path is enough.

- Observation: Codex CLI can install a portable-only package. It reports a local
  plugin and creates its internal Codex manifest during installation.
  Evidence: Codex CLI 0.146.1 installed a temporary package that contained only
  root `plugin.json`, `mcp.json`, and `skills/`. A live tool call did not run
  because the sandbox did not allow the required external action.

- Observation: Cursor Agent CLI 3.15.6 does not prove plugin MCP loading from
  `--plugin-dir`.
  Evidence: It loaded the existing Cursor package skills but did not list MCP
  tools from either a portable-only package or a package with the Cursor shim.
  Keep Cursor's native discovery and MCP adapter. Use Cursor desktop for the
  product MCP smoke.

## Decision Log

- Decision: Add the Agent Plugins package in `plugins/bifrost-agent`, beside existing host-specific files.
  Rationale: The canonical skills, launcher, release metadata, and package files already live there. A sibling package would copy assets and make release drift more likely.
  Date/Author: 2026-08-06 / Codex

- Decision: Use root `plugin.json` as the source of truth for portable identity metadata.
  Rationale: The v1 manifest is closed and small. Existing host manifests need different display names and host-only fields, so they remain adapters but must match the portable version, author, repository, license, and keywords where those fields have the same meaning.
  Date/Author: 2026-08-06 / Codex

- Decision: Keep `mcp.json` exclusively for Agent Plugins v1 and rename the present Cursor file to `cursor-mcp.json`.
  Rationale: A file cannot be both the closed v1 MCP document and a Cursor document with Cursor-only fields. The Cursor manifest can select an adapter with its existing timeout values.
  Date/Author: 2026-08-06 / Codex

- Decision: Make external-client tests opt-in, but make structural and raw-MCP tests mandatory in CI.
  Rationale: Codex and Copilot command-line clients can need user credentials, a released binary, or a network connection. CI must remain repeatable without them. The opt-in checks still give a true client proof without unlocking the laptop.
  Date/Author: 2026-08-06 / Codex

- Decision: Do not add a new live-client test script in this change.
  Rationale: The real Codex and Cursor tests give the required discovery
  evidence. Cursor's Agent CLI cannot prove its MCP behavior. The release smoke
  gives deterministic protocol coverage without credentials or a desktop.
  Date/Author: 2026-08-07 / Codex

## Outcomes & Retrospective

The implementation now has a portable root manifest and MCP file. It keeps
Codex, Claude Code, Cursor, Pi, and Amp files as adapters. The release smoke
now resolves and tests both the Codex adapter and the portable package.

The portable checker, Node tests, package-content check, host manifest checker,
and raw-MCP release smoke passed. The policy pack returned existing findings in
fixtures and reviewed repository code. It did not identify a changed file.

The full Pi TypeScript and extension tests could not run in this worktree. npm
created empty dependency directories, so Node could not load Pi packages. This
is an environment problem. It is outside this package change.

## Context and Orientation

`plugins/bifrost-agent` is the common Bifrost plugin package. Its `skills/` directory contains the editable Agent Skills. Its `bin/bifrost-launcher.mjs` resolves an approved Bifrost binary and starts it as a standard-input/standard-output MCP server. The launcher deliberately starts without `--root` unless the host explicitly supplies an approved workspace root. This protects the installed plugin directory from becoming the analyzer workspace.

The existing host manifests are:

- `plugins/bifrost-agent/.codex-plugin/plugin.json` for Codex. It selects `.mcp.json` and generated `codex-skills/`.
- `plugins/bifrost-agent/.claude-plugin/plugin.json` for Claude Code. It selects `claude-mcp.json` and `.lsp.json`.
- `plugins/bifrost-agent/.cursor-plugin/plugin.json` for Cursor. It selects
  `cursor-mcp.json`.
- `plugins/bifrost-agent/package.json` for Pi. It includes a native TypeScript extension.
- `plugins/bifrost-agent/amp-skills/` for Amp. It is generated from the canonical skills.

Agent Plugins v1 is a portable package format. Its root `plugin.json` must contain `$schema` set to `https://agent-plugins.org/schemas/1.0.0/plugin.schema.json` and a valid lowercase `name`. The manifest may contain only `$schema`, `name`, `version`, `description`, `author`, `homepage`, `repository`, `license`, `keywords`, and `extensions`.

Agent Plugins v1 discovers a skill only from an immediate `skills/<name>/SKILL.md` directory. It discovers MCP servers only from root `mcp.json`. The MCP file must declare the matching `mcp.schema.json` URL. Each stdio entry needs `type: "stdio"` and one executable `command`. The portable `command` can be `./bin/bifrost-launcher.mjs`, which the client resolves inside the plugin root. Keep the command rootless by passing only `--mcp symbol|extended`.

`scripts/check-codex-plugin-manifest.mjs` is the current manifest integrity check. `scripts/release-version.mjs` synchronizes release versions. `scripts/smoke-agent-plugin-release.mjs` starts the packaged launcher and replays standard MCP messages. `.github/workflows/ci.yml` runs these checks in its `agent-plugin` job. `.github/workflows/release.yml` copies this package to a tarball and runs the same release smoke.

## Plan of Work

### Milestone 1: Add a portable package without deleting adapters

Create `plugins/bifrost-agent/plugin.json`. Give it the Agent Plugins v1 schema URL, portable name `bifrost`, the Cargo workspace version, and the Bifrost identity metadata. Do not put `agents`, `mcpServers`, `lspServers`, `interface`, `logo`, or host path fields in this file.

Replace `plugins/bifrost-agent/mcp.json` with the Agent Plugins v1 document. It will have the MCP schema URL and one `bifrost` stdio server. Set `command` to `./bin/bifrost-launcher.mjs` and `args` to `--mcp`, `symbol|extended`. Do not set a workspace root. Do not add timeout fields, client placeholders, or shell command lines.

Move the present Cursor MCP JSON content to `plugins/bifrost-agent/cursor-mcp.json`. Update `plugins/bifrost-agent/.cursor-plugin/plugin.json` to select `./cursor-mcp.json`. Keep the existing Cursor root placeholder and timeout values there. Leave `.mcp.json` and `claude-mcp.json` as their host adapters.

Update `scripts/release-version.mjs` so it changes the portable manifest version with the Cargo version. Update `CONTRIBUTING.md`, `plugins/bifrost-agent/README.md`, and the relevant docs pages. Explain that portable clients read root `plugin.json`, `skills/`, and `mcp.json`; host-specific adapters preserve features outside the standard.

### Milestone 2: Make drift and schema errors fail locally and in CI

Add `scripts/check-agent-plugins-v1.mjs`. Use Node built-ins only. The script must parse `plugins/bifrost-agent/plugin.json` and `plugins/bifrost-agent/mcp.json`, enforce the exact v1 field sets used by Bifrost, and check required types and values. It must check these facts:

- Both schema URLs use version `1.0.0`.
- The portable package name is valid and is `bifrost`.
- The portable version equals the Cargo workspace version.
- The portable identity fields match the canonical Bifrost values used by adapters where they have the same meaning.
- Every portable skill is an immediate child of `skills/` and has one `SKILL.md`.
- Root `mcp.json` contains only `$schema` and `mcpServers`.
- The `bifrost` server is `stdio`, starts `./bin/bifrost-launcher.mjs`, uses exactly `--mcp symbol|extended`, and contains no client-only timeout or path fields.
- `cursor-mcp.json`, `.mcp.json`, and `claude-mcp.json` remain explicit adapters. Their path substitutions and timeout policy must not leak into the portable file.

Add focused Node tests for valid package data and for invalid copies. The invalid cases must include an unknown portable manifest field, a v1 MCP timeout field, a client root placeholder in portable `mcp.json`, a missing `type`, a version mismatch, and a nested skill directory. Do not download schemas at test time. The test must remain deterministic when offline.

Call the new checker from `scripts/check-codex-plugin-manifest.mjs`. Keep the
current command name. Add the new direct script to the normal and macOS CI
plugin jobs. The release manifest checker imports the portable checker, so the
release validation also checks it.

### Milestone 3: Prove portable MCP behavior without a client GUI

Extend `scripts/smoke-agent-plugin-release.mjs` with `resolvePortablePluginLaunch(pluginRoot)`. It must read root `plugin.json` first, require the v1 schema, read root `mcp.json`, and resolve its `bifrost` server command relative to the plugin root. Reuse the present MCP roots and Codex sandbox-state replay helpers after resolving the portable launch. Do not duplicate protocol or workspace-binding code.

The release smoke must execute both paths independently: the current Codex adapter (`.codex-plugin/plugin.json` and `.mcp.json`) and the portable v1 package (`plugin.json` and `mcp.json`). Each path must list `search_symbols`, invoke a read-only symbol lookup in a disposable workspace, and prove that the bound workspace is not the plugin directory. This is a protocol smoke, not a claim that a simulated client is a full product client.

Update the package archive steps so release tarballs include root `plugin.json`, root `mcp.json`, and `cursor-mcp.json`. Update `plugins/bifrost-agent/package.json` and its packed-content test if the npm Pi package should include the portable package. Decide this explicitly: include portable files if Pi package users can install it as an Agent Plugins directory; otherwise keep the npm artifact Pi-only and document the separate release archive. The default choice is to include the portable files because they are small, public, and reuse existing skills and launcher.

### Milestone 4: Test actual command-line clients in isolated homes

This milestone was superseded by the completed manual isolated-client checks.
Codex CLI 0.146.1 installed the portable-only package. Cursor Agent CLI 3.15.6
did not load plugin MCP tools from `--plugin-dir`, even with the Cursor adapter.
No new client test script is added because it would not give a reliable Cursor
MCP result. GitHub Copilot CLI was not installed.

Add `scripts/smoke-agent-plugins-v1-clients.mjs`. This test is never part of the default CI job. It must require `BIFROST_AGENT_PLUGIN_LIVE_CLIENT_TESTS=1` before executing a real client. It must also require explicit absolute paths from `BIFROST_CODEX_BIN` and `BIFROST_COPILOT_BIN`; a missing executable reports `skipped`, not success.

The script creates one temporary directory per client. It sets `HOME`, `CODEX_HOME`, `COPILOT_HOME`, `COPILOT_CACHE_HOME`, the launcher's cache directory, and `BIFROST_BINARY_PATH` to disposable locations. It never reads, changes, or removes the user's normal Codex or Copilot configuration. It copies only the portable package files into a test package root. Do not include `.codex-plugin`, `.claude-plugin`, `.cursor-plugin`, `.mcp.json`, `claude-mcp.json`, `cursor-mcp.json`, `agents`, or Amp files in that copy. This prevents a legacy manifest from making the test pass.

For Codex, create a temporary marketplace that points to the portable copy. Run `codex plugin marketplace add <temporary-marketplace>`, list the available portable plugin as JSON, install it, and list installed plugins as JSON. Capture the Codex version and the selected manifest path in a JSON report. If the Codex CLI does not yet load an Agent Plugins v1 root manifest, fail with its complete output and record the smallest reproduction in this plan and issue #1737.

For Copilot, run `copilot plugin install <portable-package-path>` with the isolated `COPILOT_HOME`, then run `copilot plugin list`. Capture the Copilot version and installed package path in the same JSON report. The portable root `mcp.json` must be present in the installed package. If Copilot reports a legacy `.mcp.json` requirement, retain a generated Copilot adapter only after recording the result and linking the upstream documentation discrepancy.

When a client has valid non-interactive credentials, the script may run its documented non-interactive prompt command against a disposable Java file. The prompt must request only a Bifrost `search_symbols` call. It must not allow file writes, shell commands, repository changes, or network actions. Treat this final tool-call proof as a manual release or nightly gate. Treat package discovery as the required headless smoke.

Add a documented command for this test. Do not add account tokens to repository files or GitHub Actions. Do not run a graphical application. The locked laptop does not block this work because Codex CLI and Copilot CLI run from a terminal.

### Milestone 5: Finish documentation and release checks

Update `plugins/bifrost-agent/README.md` with a small layout section. State that `plugin.json`, `skills/`, and `mcp.json` form the portable package. State that `cursor-mcp.json`, `.mcp.json`, `claude-mcp.json`, Pi extensions, generated Codex skills, and the Amp bundle are adapters.

Update `CONTRIBUTING.md` with the new version projection and test commands. Explain that a schema update is a deliberate compatibility decision. Do not silently change the `1.0.0` schema URLs. Update the release archive check so it fails when either portable manifest is absent.

Run the policy check required by `AGENTS.md` after code changes. Use `bifrost.code-smells` and every executable repository policy root. Correct findings or report an unreliable result as failed validation.

## Concrete Steps

Run all commands from the repository root unless a command says otherwise.

1. Inspect current package state before editing:

       git status --short --branch
       node scripts/check-codex-plugin-manifest.mjs
       node --test plugins/bifrost-agent/test/*.test.mjs

   Expect the existing host package checks to pass before the migration starts.

2. Create the portable manifests and Cursor adapter described in Milestone 1. Update version synchronization and release package inventories in the same change.

3. Run the deterministic checks after each edit:

       node scripts/check-agent-plugins-v1.mjs
       node scripts/check-codex-plugin-manifest.mjs
       node --test plugins/bifrost-agent/test/*.test.mjs
       cd plugins/bifrost-agent && npm run check && npm test && npm run test:packed

   Expect the portable checker to print the plugin name, version, skill count, and portable MCP command. Expect package tests to prove the released npm contents contain each selected portable file.

4. Build a local Bifrost binary for the raw-MCP smoke:

       cargo build --bin bifrost
       node scripts/smoke-agent-plugin-release.mjs \\
         --plugin-dir "$PWD/plugins/bifrost-agent" \\
         --cache-dir "$(mktemp -d)" \\
         --binary-path "$PWD/target/debug/bifrost"

   The expected output must name both the Codex adapter and portable package paths, then report successful workspace binding.

5. Run real headless clients only when their command paths and credentials are available:

       BIFROST_AGENT_PLUGIN_LIVE_CLIENT_TESTS=1 \\
       BIFROST_CODEX_BIN="$(command -v codex)" \\
       BIFROST_COPILOT_BIN="$(command -v copilot)" \\
       BIFROST_BINARY_PATH="$PWD/target/debug/bifrost" \\
       node scripts/smoke-agent-plugins-v1-clients.mjs

   The JSON report must identify the client versions, installed temporary paths, selected portable manifest, and result for skills and MCP discovery. A missing client is `skipped`. A client that selects a legacy manifest is `failed`.

6. Run the focused policy request and record its output in the pull request. Then run:

       git diff --check
       git status --short

## Validation and Acceptance

The change is complete when all of these observable behaviors hold:

- `node scripts/check-agent-plugins-v1.mjs` accepts the Bifrost portable package and rejects each invalid fixture.
- `node scripts/release-version.mjs check` fails after changing only the portable manifest version. It succeeds after the synchronization command updates every required projection.
- The existing Codex, Claude Code, Cursor, Pi, Amp, and generic skill install tests still pass.
- `scripts/smoke-agent-plugin-release.mjs` starts Bifrost from both the legacy Codex adapter and portable `mcp.json`. Both runs resolve a symbol from a temporary workspace. Neither run creates a `.bifrost` cache under the installed plugin directory.
- The isolated Codex CLI check installed the portable-only package from a
  temporary marketplace. Cursor Agent CLI did not prove MCP loading, so the
  Cursor desktop path remains the product MCP smoke. GitHub Copilot CLI was not
  installed.
- Documentation identifies which fields are portable and which files are adapters.

## Idempotence and Recovery

The manifest checker and raw-MCP smoke only read the package and create unique temporary directories. They are safe to rerun. The live client smoke sets fresh isolated homes and removes only the temporary directories that it created.

Do not use a real home directory, a global client install location, or `--force` marketplace removal during tests. If a client test fails, retain its temporary report when `BIFROST_KEEP_AGENT_PLUGIN_SMOKE=1`; otherwise cleanup is automatic. Use the report to reproduce the failure with the exact client version. Do not change a portable manifest to add host-only fields just to satisfy one client; add or keep an explicit host adapter instead.

## Artifacts and Notes

The expected portable package shape is:

    plugins/bifrost-agent/
      plugin.json
      mcp.json
      bin/bifrost-launcher.mjs
      skills/
        bifrost-code-navigation/SKILL.md
        bifrost-code-reading/SKILL.md
        bifrost-codebase-search/SKILL.md
        bifrost-policy-checking/SKILL.md

The expected portable MCP entry is conceptually:

    {
      "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
      "mcpServers": {
        "bifrost": {
          "type": "stdio",
          "command": "./bin/bifrost-launcher.mjs",
          "args": ["--mcp", "symbol|extended"]
        }
      }
    }

The command is an executable token. It is not a shell command. It does not contain a client placeholder or a workspace path.

## Interfaces and Dependencies

`scripts/check-agent-plugins-v1.mjs` must expose no production API. It is a Node executable that exits nonzero on malformed package data. It may export small pure validation functions for Node tests, but it must not fetch a remote schema during validation.

`scripts/smoke-agent-plugin-release.mjs` must add this function:

    async function resolvePortablePluginLaunch(pluginRoot) {
      // Return the absolute launcher command, working directory, and arguments
      // after validating root plugin.json and root mcp.json.
    }

Its return shape must match the existing launch resolver enough to reuse `prepare`, `assertCodexSandboxWorkspaceBinding`, and `assertMcpRootsWorkspaceBinding`.

`scripts/smoke-agent-plugins-v1-clients.mjs` must accept only environment configuration. Its report must be JSON and contain one entry per attempted client:

    {
      "client": "codex" | "copilot",
      "version": "string",
      "status": "passed" | "failed" | "skipped",
      "portableManifest": "absolute path",
      "skillsDiscovered": true | false,
      "mcpDiscovered": true | false,
      "details": "string"
    }

Update note, 2026-08-06: Created the plan after inspecting issue #1737, the v1 specification, the current Bifrost package, and the available headless client commands. The plan makes graphical clients unnecessary for initial proof.

Update note, 2026-08-07: Implemented the portable manifests, Cursor adapter,
version projection, package inventory, structural checker, and release-smoke
resolver. Headless Codex installation passed. Cursor Agent CLI could not prove
MCP loading, so the retained Cursor adapter remains required.

Validation note, 2026-08-07: `node scripts/check-agent-plugins-v1.mjs`,
`node scripts/check-codex-plugin-manifest.mjs`, release-version checks, 59
focused Node tests, and the two-path raw-MCP release smoke passed. The Pi
package-content check passed. Full Pi checks remain blocked by empty npm
dependency directories in this worktree. The code-smell pack reported existing
repository findings outside the changed paths.
