# Cut the v0.9.0 release from a stabilized RC branch

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`.

## Purpose / Big Picture

Bifrost v0.9.0 must publish one consistent version of the Rust crates, command-line archives, Python packages, editor extension, and agent plugins. The release must use a dedicated release-candidate branch. A successful result has a signed `v0.9.0` tag on the validated RC commit, a successful Release workflow, and published artifacts from that same commit.

## Progress

- [x] (2026-08-11 04:40Z) Read the release process and fetch current branches and tags.
- [x] (2026-08-11 04:45Z) Find the failed Linux installer test and failed policy scan on `master`.
- [x] (2026-08-11 05:08Z) Correct and validate the confirmed `master` blockers.
- [x] (2026-08-11 05:35Z) Commit and push the first validated `master` repair.
- [x] (2026-08-11 06:10Z) Correct the remaining Hermes fixture race found by the full Linux CI job.
- [x] (2026-08-11 07:00Z) Create and push `dave/v0.9.0-rc` from master commit `35868e505`.
- [x] (2026-08-11 07:08Z) Set the workspace version to 0.9.0 and synchronize release metadata.
- [x] (2026-08-11 07:10Z) Copy the version projection and both RC fixes to `master`.
- [x] (2026-08-11 07:45Z) Run the release validation gates on the frozen RC.
- [x] (2026-08-11 07:48Z) Tag RC commit `b9ac4c806` as `v0.9.0` and push the tag.
- [ ] Monitor the Release and follow-on publication workflows. Stop on a failed gate.
- [ ] Confirm the GitHub Release and public package versions.

## Surprises & Discoveries

- Observation: `master` at `b898da7f` failed the Linux test matrix and policy scan.
  Evidence: CI run 31450971490 failed `install_registers_all_native_mcp_hosts_without_using_real_home`; policy run 31450971428 exited 1.

- Observation: The installer fixture uses `rm` and `touch` while the test replaces `PATH` with a directory that contains only fake host programs.
  Evidence: `tests/suite_mcp_cli/bifrost_install_cli.rs` sets `PATH` to its temporary `bin` directory.

- Observation: The policy failure contained six active findings, not broad baseline drift.
  Evidence: Two file reads, two parses, and two sorts operate on different per-loop inputs in the new summary-foundry code.

- Observation: The debug policy scan took approximately 140 seconds, including a warm second run.
  Evidence: Both local `target/debug/bifrost` runs exceeded two minutes; the release CI build completed its scan in 60 seconds.

- Observation: The full Linux job found a second fixture race after the first focused test passed.
  Evidence: CI run 31462640467 reported a broken pipe while the installer wrote the Hermes confirmation response. The fake Hermes host exited before it read standard input.

- Observation: `brokk-bifrost-rql` was the only release crate that did not exist on crates.io.
  Evidence: The 0.8.24 package dry run passed. The bootstrap publication then succeeded, and its trusted publisher now names `BrokkAi/bifrost`, `release.yml`, and the `release` environment.

- Observation: The release sync script omitted `crates/bifrost-rql/Cargo.toml`.
  Evidence: `cargo check --workspace --locked` rejected the remaining `brokk-bifrost-core = "=0.8.24"` dependency after the first 0.9.0 sync. A behavior test now verifies this projection.

- Observation: The active-session read-only cache opener failed for macOS temporary paths.
  Evidence: SQLite `SQLITE_OPEN_NOFOLLOW` refused `/var/folders/...`, while the other read-only opener already canonicalized the parent to `/private/var/folders/...`.

- Observation: Direct RC branch pushes do not start the repository CI workflows.
  Evidence: `ci.yml` and `policy-sarif.yml` accept direct pushes only on `master`. The RC therefore used the local pre-push gate, while master received the identical tree.

- Observation: The first tagged Release run failed before promotion because every pinned JVM pack excluded Bifrost 0.9.0.
  Evidence: Run 31470463663 reported `representative lookup did not resolve any records: Type { name: "java.lang.Object" }`. The JDK spec declared `>=0.8.18, <0.9.0`.

- Observation: The release version script did not project compatibility ranges into pinned release-bundle specifications.
  Evidence: The script synchronized code and plugin versions but did not inspect the three JVM specifications or the Python specification. It now updates their exclusive upper bound and has a behavior test.

- Observation: Promotion did not start after the JVM failure.
  Evidence: Every crate, wheel, agent plugin, Pi package, and editor publication job in run 31470463663 was skipped.

## Decision Log

- Decision: Repair `master` before selecting the RC commit.
  Rationale: The user explicitly permitted a master repair, and the current tip is not green.
  Date/Author: 2026-08-11 / Codex

- Decision: Stop at any failed validation or publication gate.
  Rationale: A release tag must identify evidence that all common package forms passed.
  Date/Author: 2026-08-11 / Codex

- Decision: Freeze the current master tip before its full push workflow completes.
  Rationale: The RC branch is the stable snapshot. New master pushes twice cancelled long master jobs. The RC branch supplies the exact validation target.
  Date/Author: 2026-08-11 / Codex

## Outcomes & Retrospective

The stabilization work corrected two Linux fixture defects, one macOS cache-path defect, and the release projections. The first immutable v0.9.0 tag failed before promotion because its pinned packs excluded 0.9.0. No artifact was published from that run.

## Context and Orientation

`Cargo.toml` contains `[workspace.package].version`, the release version source. `scripts/release-version.mjs sync` copies that value into committed plugin and editor files. `CONTRIBUTING.md` requires an RC branch, requires every RC fix on `master`, and requires the final tag on the validated RC commit. `.github/workflows/release.yml` runs after the tag is pushed. `.bifrost/suppressions.json` records reviewed code-smell findings for the repository policy scan.

The RC branch is a stable branch cut from one selected `master` commit. Only release fixes can move it. The release tag starts publication from one immutable commit.

## Plan of Work

First, make `master` green. Correct the installer test fixture without weakening its behavior check. Inspect the policy report and correct its baseline or policy defect only after the report proves the cause. Run focused tests, formatting, and the required local gate. Commit and push only the changed files.

Next, create `dave/v0.9.0-rc` from the green `master` commit and push it. Change only the workspace version in `Cargo.toml`, run the synchronization script, inspect all generated changes, run manifest checks, and commit the release projection. Copy that focused commit to `master` and push it so both source lines contain the same release metadata.

Then, validate the frozen RC branch. Run the repository pre-push gate when practical and all release metadata checks. Confirm remote branch checks. Create the annotated `v0.9.0` tag only after these gates pass, then push it.

Finally, monitor the Release workflow and its publication workflows. Do not rerun or replace the tag without a precise failure analysis. Confirm the GitHub Release and the package registries after all workflows succeed.

## Concrete Steps

Run all commands from `/Users/dave/.codex/worktrees/94ac/bifrost`.

    cargo test --test suite_mcp_cli bifrost_install_cli::unix::install_registers_all_native_mcp_hosts_without_using_real_home -- --exact
    cargo fmt
    git commit <only changed master repair files>
    git push origin master
    git switch -c dave/v0.9.0-rc <green-master-sha>
    git push -u origin dave/v0.9.0-rc
    node scripts/release-version.mjs sync
    node scripts/release-version.mjs check
    node scripts/check-codex-plugin-manifest.mjs
    node --test plugins/bifrost-agent/test/*.test.mjs
    git tag -a v0.9.0 -m "Release v0.9.0"
    git push origin refs/tags/v0.9.0

Update this section with exact commands and short results as work continues.

## Validation and Acceptance

The focused installer test must pass twice without access to the real home directory. Formatting must produce no diff. The full selected local gate and all remote RC checks must pass. `node scripts/release-version.mjs check` must report synchronized 0.9.0 metadata. `git rev-list -n 1 v0.9.0` must equal the validated RC commit. The GitHub Release and each public package must report version 0.9.0 from that commit.

## Idempotence and Recovery

Fetch, tests, metadata checks, and workflow inspection are safe to repeat. Version synchronization is deterministic. If an RC check fails, fix the cause on both `master` and the RC branch, then repeat validation. If publication fails after tag creation, keep the tag and diagnose the failed publication job. Do not create a replacement tag for the same version.

## Artifacts and Notes

Current evidence:

    master: b898da7fd0b16a27aac0d672ccaa0d531680b5ee
    CI: https://github.com/BrokkAi/bifrost/actions/runs/31450971490
    policy: https://github.com/BrokkAi/bifrost/actions/runs/31450971428
    first repair CI: https://github.com/BrokkAi/bifrost/actions/runs/31462640467
    frozen RC source: 35868e505e26cf56c49c8e93cc293eaa66f857ad
    focused installer test: 1 passed; 0 failed
    local policy scan after review: exit 0
    release metadata checks: passed at 0.9.0
    agent plugin tests: 116 passed; 0 failed
    cargo check --workspace --locked: passed
    cache database tests: 49 passed; 0 failed
    cargo nextest run --workspace: 9989 passed; 0 failed
    cargo test --workspace --doc: passed
    cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
    exact pinned JVM bundle script after compatibility fix: passed
    semantic-pack release-tooling tests: 116 passed; 0 failed
    framework pack shipping tests: 6 passed; 0 failed
    golden and sanitizer pack shipping tests: 10 passed; 0 failed

## Interfaces and Dependencies

Do not add a crate or dependency. Keep `install_mcp_hosts` behavior unchanged unless the focused diagnosis proves a product defect. Use the existing release scripts and GitHub workflows. The tag remains the only publication trigger.

Revision note: Created this plan after current `master` failed two release gates. The plan includes master repair before RC selection.

Revision note: Updated the first milestone after the focused installer test passed and the reviewed policy scan exited cleanly.

Revision note: Recorded the Hermes confirmation race and the RQL crate bootstrap before the second master repair checkpoint.

Revision note: Corrected the gate sequence after two fast master pushes cancelled long jobs. The RC branch now freezes the validation target first.

Revision note: Recorded and corrected the missing RQL dependency projection found during the 0.9.0 locked Cargo check.

Revision note: Recorded the macOS active-session cache path failure found by the RC pre-push gate.
