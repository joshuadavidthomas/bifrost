# Bifrost Agent Plugin Publication

This runbook describes the Bifrost host integrations under
`plugins/bifrost-agent`. Codex uses `.agents/plugins/marketplace.json`, Claude
Code uses `.claude-plugin/marketplace.json`, and Cursor uses
`.cursor-plugin/marketplace.json`. The plugin's stable install name is
`brokk`; Cursor uses `bifrost` as its display name.

## Plugin shape

Keep these manifest versions aligned with `Cargo.toml`:

- `plugins/bifrost-agent/.codex-plugin/plugin.json`
- `plugins/bifrost-agent/.claude-plugin/plugin.json`
- `plugins/bifrost-agent/.cursor-plugin/plugin.json`
- `plugins/bifrost-agent/package.json`

The manifests provide host-specific MCP configuration. Claude also provides
the packaged LSP configuration. Codex, Claude, and Cursor use the shared
launcher and the pinned metadata in `plugins/bifrost-agent/bifrost-release.json`.
None of these packages contains the Bifrost binary.

The default analyzer MCP command is:

```bash
bifrost --root /absolute/path/to/workspace --mcp "symbol|extended"
```

The launcher resolves a binary from `BIFROST_BINARY_PATH`, its managed cache,
or a checksum-verified GitHub release. It uses `PATH` only when
`BIFROST_LAUNCHER_ALLOW_PATH=1` is set. Use `doctor [--json]` to inspect a
candidate without downloading. Use `prepare [--json]` to install the pinned
release without starting MCP.

The launcher uses `BIFROST_WORKSPACE_ROOT` or a host-provided root. Without
either value, it starts unbound so the MCP client can provide an approved root.
It must not use the installed plugin directory as the analyzer workspace.

## Pi package

`plugins/bifrost-agent/package.json` publishes the native TypeScript extension
under `extensions/`. The extension starts one MCP child for the Pi session and
maps canonical tools to the `bifrost_` namespace. Its `/bifrost` settings UI
selects Bifrost toolsets; it does not manage workspace lifecycle tools.

Keep `package.json`, `package-lock.json`, `bifrost-release.json`, and
`Cargo.toml` version-aligned. Test the package with:

```bash
cd plugins/bifrost-agent
npm ci
npm test
npm run check:package
npm run test:packed
```

## Release validation

Before publication, prepare release metadata from the built archive sidecars.
Then run:

```bash
node scripts/release-version.mjs check
node scripts/check-codex-plugin-manifest.mjs
node --test plugins/bifrost-agent/test/*.test.mjs
claude plugin validate plugins/bifrost-agent
claude plugin validate .
```

The manifest check validates host manifests, MCP and LSP configuration,
launcher metadata, marketplace versions, and release hashes.

The release workflow packages `plugins/bifrost-agent` for the host plugin
markets. It also publishes the Pi package and the VS Code extension. It does
not generate or publish instruction-file bundles.

## MCP smoke

Validate one real tool call from each host. Use a source file, not a README.
Confirm that the result contains a path inside the selected workspace.

```text
Call the Bifrost get_summaries tool on a source file and report the returned symbols.
```

Keep the MCP process separate from the VS Code LSP process. They can use the
same binary and release metadata, but each host starts its own stdio process.
