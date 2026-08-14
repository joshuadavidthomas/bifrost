# Select patch-compatible Bifrost binaries in the shared agent launcher

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

An installed Bifrost agent plugin should keep starting when its preferred patch release is temporarily unavailable, provided another binary from the explicitly supported patch range is already in the launcher cache or is supplied through an approved local path. After this change, every supported MCP and LSP host adapter continues to invoke one shared launcher, and that launcher chooses the preferred cached binary first, otherwise the newest compatible cached patch, otherwise an explicitly configured or opted-in PATH binary, and only then downloads the checksum-pinned preferred release. `doctor --json` and startup diagnostics expose both versions and whether compatibility fallback was used.

## Progress

- [x] (2026-08-13 12:00Z) Read issue #2096, inspected worktree and remote state, and located the shared launcher, its tests, adapter checks, release metadata, and host documentation.
- [x] (2026-08-13 12:22Z) Added preferred/minimum release metadata validation and SemVer compatibility helpers.
- [x] (2026-08-13 12:29Z) Implemented deterministic newest-compatible cache discovery and shared candidate selection.
- [x] (2026-08-13 12:34Z) Extended doctor/startup diagnostics, resolved-launch metadata, and TypeScript declarations.
- [x] (2026-08-13 12:40Z) Added focused launcher, release-tooling, diagnostic, and package-contract tests.
- [x] (2026-08-13 12:43Z) Updated release tooling, common launcher documentation, and Amp's shared-launcher example.
- [x] (2026-08-13 12:49Z) Ran focused and package validation; recorded the unrelated Pi dependency installation failure below.
- [x] (2026-08-13 13:20Z) Projected the common binary range into the VSIX manifest used by both Visual Studio Marketplace and Open VSX.
- [x] (2026-08-13 13:27Z) Reused the exact or newest compatible verified VS Code managed binary before prompting to download the preferred patch.
- [x] (2026-08-13 13:31Z) Preserved compatible cached patches during VS Code managed-binary cleanup.
- [x] (2026-08-13 13:43Z) Added and passed focused VS Code provisioning, release projection, packaging, and Open VSX artifact-contract validation.
- [x] (2026-08-13 14:02Z) Launched compatible agent binaries immediately while detached preparation caches the preferred binary for the next host task.
- [x] (2026-08-13 14:10Z) Proved background preparation is checksum-pinned, non-blocking, auto-install-aware, reusable by Pi/future adapters, and non-fatal.
- [x] (2026-08-13) Added best-effort cleanup for launcher installation artifacts abandoned for more than 24 hours in the OS temp directory or managed binary directory.

## Surprises & Discoveries

- Observation: The worktree is detached at `fcd830452`, two commits behind `origin/master`, and is clean.
  Evidence: `git status --short --branch` printed `## HEAD (no branch)` and `git rev-list --count HEAD..origin/master` printed `2`; the intervening changes are analyzer/RQL work rather than launcher work.
- Observation: MCP and LSP already share `resolveBifrostBinary` through `resolveBifrostLaunch` and `resolveBifrostLspLaunch` in `plugins/bifrost-agent/bin/bifrost-launcher.mjs`.
  Evidence: Both launch builders call the same exported resolver before constructing protocol-specific arguments.
- Observation: The initial Pi failure came from an incomplete dependency installation rather than a malformed lockfile or published package.
  Evidence: The npm registry metadata for `@earendil-works/pi-ai@0.80.10` exports `./compat`, its published tarball contains both `dist/compat.js` and `dist/compat.d.ts`, and a completed `npm ci --ignore-scripts` followed by `npm test` passed all 124 plugin tests.
- Observation: The default npm cache contains root-owned files on this host.
  Evidence: `npm run check:package` initially failed with npm `EPERM` under `/Users/dave/.npm/_cacache`; rerunning with `npm_config_cache=/tmp/bifrost-npm-cache` passed and reported `Validated Pi manifest and 20 packed files.`
- Observation: Open VSX and the Visual Studio Marketplace consume the same validated VSIX rather than independently generated extension packages.
  Evidence: `.github/workflows/release.yml` makes `publish-open-vsx` reuse the `vscode-package` artifact, and `scripts/release-promotion-workflow.test.mjs` pins that contract and checksum equality.

## Decision Log

- Decision: Keep `binaryVersion` as the checksum-pinned preferred version and add `minimumBinaryVersion`; use the preferred version's major and minor as the exclusive upper-series boundary.
  Rationale: Existing release scripts and manifests already treat `binaryVersion` as the downloadable release. A minimum plus an implicit same-series ceiling expresses the issue's contract without duplicating an error-prone maximum on every patch release.
  Date/Author: 2026-08-13 / Codex
- Decision: Reject all prerelease candidates for the initial metadata format and record `allowPrerelease: false` explicitly.
  Rationale: The issue requires prereleases to be rejected unless metadata permits them. An explicit false policy is auditable now; support for true can validate candidates consistently without making prereleases eligible by default.
  Date/Author: 2026-08-13 / Codex
- Decision: Preserve the explicit `BIFROST_BINARY_PATH` override as an intentional operator choice, but apply the same range validation to it; among automatically discovered candidates, exact managed then newest compatible managed then opted-in PATH wins.
  Rationale: Existing users rely on the explicit override for local testing, while the acceptance criteria specifically require managed exact precedence over automatic fallbacks and consistent compatibility checks for explicit/PATH binaries.
  Date/Author: 2026-08-13 / Codex
- Decision: Treat `plugins/bifrost-agent/bifrost-release.json` as the release-time range source and project the same three fields into the VSIX manifest.
  Rationale: The Visual Studio Marketplace and Open VSX jobs publish the same validated VSIX, so one projected manifest guarantees identical runtime behavior without registry-specific compatibility logic.
  Date/Author: 2026-08-13 / Codex
- Decision: Keep the first-download consent prompt, but do not prompt when a verified compatible managed patch is already cached.
  Rationale: Reusing previously installed code does not expand the trust boundary, while a new network download and executable installation remains visible to the user.
  Date/Author: 2026-08-13 / Codex
- Decision: When the shared launcher selects compatibility mode and automatic installation is enabled, spawn a detached copy of the launcher to prepare only the preferred release.
  Rationale: A detached helper lets MCP/LSP registration proceed immediately and survive the current server process ending, while reusing the existing checksum-pinned installer and cache layout. The next host task then selects the preferred managed binary. Explicit `BIFROST_LAUNCHER_AUTO_INSTALL=0` continues to prohibit the download.
  Date/Author: 2026-08-13 / Codex
- Decision: Remove only strictly named installation artifacts older than 24 hours, and never fail a launch because cleanup failed.
  Rationale: Normal failures already clean up in `finally`; the remaining case is a hard client or machine crash. A generous age threshold avoids interfering with concurrent or suspended installation helpers while bounding disk residue from abandoned extraction directories and atomic-install staging files.
  Date/Author: 2026-08-13 / Codex

## Outcomes & Retrospective

The shared launcher now accepts stable compatible patches from the declared minor series, deterministically prefers the exact managed binary and then the newest compatible managed binary, applies the same range to explicit and opted-in PATH candidates, and only downloads the exact checksum-pinned preferred version. Doctor and launch results expose preferred/selected versions and exact/compatible mode, while compatibility startup logging remains on stderr. A compatible launch now schedules a detached preferred-version preparation through the reusable launch API, so Codex, Pi, protocol adapters, and future callers start immediately but can use the preferred release after a fresh task. Release sync preserves a minimum across patch releases and resets it for a new minor series. The VS Code provisioner consumes the same fields, reuses a verified compatible cached patch without prompting, and retains compatible patches during cleanup. Because Open VSX and Visual Studio Marketplace publish the same VSIX, both receive identical behavior. Validation passed all 129 agent plugin tests, all 95 VS Code tests, release/projection/workflow tests, TypeScript, ESLint, formatting, manifest checks, `git diff --check`, and an artifact-level inspection of the packaged VSIX metadata.

## Context and Orientation

`plugins/bifrost-agent/bin/bifrost-launcher.mjs` is the single JavaScript executable used by Portable Agent Plugins v1/Codex, Claude Code, Cursor, Pi, and Amp. Its protocol-specific functions build MCP or LSP arguments, but both call `resolveBifrostBinary`, so compatibility selection belongs there rather than in any adapter. `plugins/bifrost-agent/bifrost-release.json` records the exact preferred binary and per-platform archive checksums. Managed binaries live under `<cache>/binaries/<version>/<platform>-<arch>/bifrost[.exe]`.

`plugins/bifrost-agent/test/launcher.test.mjs` exercises candidate selection, downloads, diagnostics, and real adapter invocations. `scripts/check-codex-plugin-manifest.mjs` and `scripts/check-agent-plugins-v1.mjs` validate the packaged host adapters. `scripts/release-version.mjs` updates release metadata during version preparation. `editors/vscode/src/provisioning.ts` separately installs and selects the VS Code LSP binary. `scripts/prepare-vscode-extension-manifest.mjs` projects release metadata into `editors/vscode/package.json`; the tag-driven workflow publishes that same VSIX to the Visual Studio Marketplace and Open VSX. Human-facing common behavior is documented in `docs/src/content/docs/data-boundaries.md`, with host pages linking to that shared explanation.

Compatibility mode means that the selected binary is not the preferred patch but is inside the declared range. Exact mode means the selected version equals the preferred version. A compatible stable version must be at least `minimumBinaryVersion`, have the same major and minor numbers as `binaryVersion`, and contain no prerelease identifier unless metadata explicitly enables prereleases.

## Plan of Work

First, extend metadata parsing so malformed preferred/minimum versions, cross-series minima, inverted ranges, and invalid prerelease policy fail before candidate discovery. Add small SemVer parsing and comparison helpers in the shared launcher and use them for every candidate source.

Next, preserve the explicit development override, then preserve the exact managed-path check and discover sibling version directories under the cache's `binaries` directory. Parse directory names as versions, discard incompatible or prerelease entries, sort semantically newest-first, and probe candidates in that order. A directory name is only an index hint: the executable's reported version remains authoritative. Apply the same range to `BIFROST_BINARY_PATH` and the opted-in PATH result. Installation remains limited to `binaryVersion` and still requires its platform checksum.

Then, carry `preferredVersion`, `selectedVersion`, `source`, and `compatibilityMode` through resolved launches and stable doctor output. Emit a stderr startup line only for a compatibility fallback so MCP/LSP stdout remains protocol-clean. Update the declaration file used by Pi.

Finally, expand behavior tests for exact precedence, unordered cached patches, lower and upper boundaries, prereleases, explicit and PATH candidates, pinned-download behavior, doctor JSON, and startup logging. Strengthen package checks so every adapter continues to name the shared launcher. Update the common launcher documentation and host-facing references, then run focused Node tests and repository package checks.

## Concrete Steps

Work from `/Users/dave/.codex/worktrees/50a5/bifrost`.

Run launcher tests during implementation:

    node --test plugins/bifrost-agent/test/launcher.test.mjs

Run agent package and adapter checks:

    npm --prefix plugins/bifrost-agent test
    npm --prefix plugins/bifrost-agent run check
    node scripts/check-agent-plugins-v1.test.mjs
    node scripts/check-codex-plugin-manifest.mjs

Run formatting or syntax checks required by the touched JavaScript/TypeScript files through the package checks. This task does not change Rust code and therefore does not require an NLP build.

## Validation and Acceptance

Tests must demonstrate that an exact explicit override remains usable, an exact managed binary beats automatic fallbacks, cache directory order cannot change selection, the newest compatible stable patch is selected, the minimum is inclusive, a different minor and a prerelease are rejected by default, and PATH is considered only with `BIFROST_LAUNCHER_ALLOW_PATH=1`. A missing preferred binary may trigger only the preferred version download and only when its target checksum exists.

`doctor --json` must always include `preferredVersion`, `selectedVersion`, `source`, and `compatibilityMode`. A compatible fallback launch must state preferred and selected versions on stderr without writing to stdout. Both MCP and LSP tests must observe the same resolved binary metadata. Package checks must prove Portable Agent Plugins v1/Codex, Claude MCP, Claude LSP, Cursor, Pi, and Amp enter through `plugins/bifrost-agent/bin/bifrost-launcher.mjs` or its imported resolver rather than defining compatibility themselves.

## Idempotence and Recovery

Candidate inspection is read-only and can be rerun. Installation already uses a unique temporary directory and atomic rename; keep that behavior. Cache discovery must not create directories in doctor mode. If a cached candidate is malformed or fails probing, continue to lower-priority automatic candidates while retaining a useful diagnostic for the final failure. Never download a compatible fallback because only the preferred version has pinned archive metadata.

## Artifacts and Notes

Issue: `https://github.com/BrokkAi/bifrost/issues/2096`.

The worktree began detached and clean. Repository instructions prohibit creating or changing branches without explicit user instruction, so implementation remains on the detached checkout unless the user directs otherwise.

## Interfaces and Dependencies

Keep the implementation dependency-free. `readReleaseMetadata` returns a normalized object containing `binaryVersion`, `minimumBinaryVersion`, `allowPrerelease`, and `archiveSha256`. `resolveBifrostBinary`, `resolveBifrostLaunch`, and `resolveBifrostLspLaunch` return the selected path/source plus preferred/selected version and compatibility mode. `isVersionCompatible` accepts a candidate version and normalized release metadata (while retaining a narrow compatibility overload only if existing external callers require it after tests are updated).
