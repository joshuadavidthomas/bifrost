# Cut v0.9.1 from a verified RC snapshot

This ExecPlan is a living document. Maintain it under `.agents/PLANS.md`.

## Purpose / Big Picture

Bifrost v0.9.1 replaces the unpublished v0.9.0 release candidate. It includes the corrected semantic pack compatibility range. The release uses one frozen RC branch and one immutable tag. A successful result publishes all Rust crates, command archives, Python packages, editor files, and agent plugins from the same commit.

## Progress

- [x] (2026-08-11 09:18Z) Fetch remote state and prove that all v0.9.0 RC fixes have equivalent commits on `master`.
- [x] (2026-08-11 09:20Z) Create and push `dave/v0.9.1-rc` from master commit `528655957`.
- [x] (2026-08-11 09:22Z) Set version 0.9.1 and synchronize release metadata.
- [x] (2026-08-11 09:24Z) Validate release metadata and all 118 agent plugin tests.
- [ ] Commit the version projection and copy it to `master`.
- [ ] Run the full release gate and the pinned semantic pack build on the frozen RC.
- [ ] Tag the validated RC commit as `v0.9.1` and push the tag.
- [ ] Monitor publication and confirm all public version 0.9.1 artifacts.

## Surprises & Discoveries

- Observation: Every commit from `dave/v0.9.0-rc` has an equivalent patch on current `master`.
  Evidence: `git cherry origin/master origin/dave/v0.9.0-rc` marked all four commits with `-`.

- Observation: The v0.9.0 workflow published no artifacts.
  Evidence: Release run 31470463663 failed before the promotion gate, and all publication jobs were skipped.

## Decision Log

- Decision: Use v0.9.1 and keep the v0.9.0 tag unchanged.
  Rationale: A release tag is immutable. The corrected source needs a new version and tag.
  Date/Author: 2026-08-11 / Codex

- Decision: Freeze current `master` at `528655957` before version projection.
  Rationale: This includes every release fix and preserves the RC branch as a stable snapshot while `master` moves.
  Date/Author: 2026-08-11 / Codex

## Outcomes & Retrospective

The RC snapshot and version projection exist. Publication remains pending until all local gates pass and the tag workflow succeeds.

## Context and Orientation

`Cargo.toml` contains `[workspace.package].version`, which is the release version source. `scripts/release-version.mjs sync` copies that version into all Cargo manifests and plugin metadata. `CONTRIBUTING.md` requires a dedicated RC branch. It also requires each RC fix and version projection on `master`. The `v0.9.1` tag starts `.github/workflows/release.yml`.

The prior `v0.9.0` tag points to a build that failed before publication. Its pinned JVM semantic pack specifications excluded version 0.9.0. Commit `adbd4a379` on the old RC corrected the ranges and made release version synchronization maintain them. Master contains the equivalent commit `528655957`.

## Plan of Work

Commit the synchronized 0.9.1 metadata on the RC branch. Copy that focused commit to current `master`, push both branches, and return to the RC branch. Run the exact pinned JVM semantic pack build that failed for v0.9.0. Run the complete repository release gate with all features. Stop if any check fails.

After all checks pass, record the evidence in this plan and make a final checkpoint commit. Tag that exact commit as `v0.9.1` and push only that tag. Monitor the Release workflow. Confirm that each publication target reports version 0.9.1 before completion.

## Concrete Steps

Run all commands from `/Users/dave/.codex/worktrees/94ac/bifrost`.

    node scripts/release-version.mjs check
    node scripts/check-codex-plugin-manifest.mjs
    node --test plugins/bifrost-agent/test/*.test.mjs
    bash scripts/build-pinned-jvm-semantic-packs.sh <output-dir> <work-dir>
    scripts/pre-push-gate.sh
    git tag -a v0.9.1 -m "Release v0.9.1"
    git push origin refs/tags/v0.9.1

Update this section with exact results as work continues.

## Validation and Acceptance

The version check must report 0.9.1. Plugin checks must pass. The pinned JVM build must resolve all representative records. The full pre-push gate must pass with all workspace tests, doctests, formatting, and all-feature clippy. The tag commit must equal the validated RC commit. GitHub Actions must publish every target from that same commit.

## Idempotence and Recovery

Fetch, version checks, tests, and pack generation are safe to repeat. Remove only temporary directories created by this plan. If validation fails, fix the cause on both the RC branch and `master`, then repeat the full gate. After the tag exists, do not move it. Diagnose a failed workflow before any rerun.

## Artifacts and Notes

Current evidence:

    master and RC branch point before projection: 52865595705a8422ed56f77b55e0657a8087084c
    old failed release run: https://github.com/BrokkAi/bifrost/actions/runs/31470463663
    release version check: passed at 0.9.1
    agent plugin tests: 118 passed; 0 failed

## Interfaces and Dependencies

Do not add a crate or dependency. Use the existing version projection script, semantic pack build script, pre-push gate, and release workflow. Keep the v0.9.0 tag unchanged.

Revision note: Created this plan for the v0.9.1 recovery release after confirming that every v0.9.0 RC fix was present on master.
