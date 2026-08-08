---
title: CI Gating with GitHub Actions
description: Run Bifrost policies in CI and show findings as pull-request annotations through GitHub code scanning.
---

Bifrost ships a reusable GitHub Action that runs static-analysis policies and uploads the SARIF report to GitHub code scanning. GitHub then shows each finding as a pull-request annotation and tracks alert lifecycle across runs.

## Quick start

Add one workflow file:

```yaml
name: bifrost-policies

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read
  security-events: write

jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: BrokkAi/bifrost/.github/actions/policy-scan@v0.8.24
```

The default configuration installs the pinned Bifrost release, runs the `bifrost.code-smells` pack on the checkout, writes `bifrost-policy.sarif`, uploads it, and gates on the exit code.

The `security-events: write` permission is required for the SARIF upload. Code scanning must be available on the repository.

## Exit codes

The gate distinguishes three results:

- `0` - clean. The job passes.
- `1` - findings at or above the `fail-on` threshold. The job fails and the findings appear as code-scanning alerts.
- `2` - unreliable. The run could not prove its own completeness, for example after a budget or capability limit. The job fails with a distinct message. Do not treat an unreliable run as clean, and do not lower `fail-on` to hide it. Read the report diagnostics instead.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | pinned release tag | Bifrost release to install. Pin an exact tag for reproducible gates. |
| `policy-packs` | `bifrost.code-smells` | Space-separated built-in pack IDs. |
| `policy-ids` | empty | Space-separated policy IDs. |
| `policy-categories` | empty | Space-separated categories. |
| `policy-files` | empty | Space-separated workspace-relative `.rqlp` files. |
| `fail-on` | `warning` | Severity gate: `never`, `finding`, `note`, `warning`, or `error`. |
| `diff-base` | empty | Git revision to diff against. Only findings absent from that revision gate. |
| `sarif-file` | `bifrost-policy.sarif` | SARIF output path, relative to `working-directory`. |
| `upload` | `true` | Upload the SARIF file to code scanning. |
| `category` | `bifrost-policy` | Code-scanning category. Use one category per scan configuration. |
| `cache` | `true` | Restore and save the analyzer cache between runs. |
| `working-directory` | `.` | Workspace root to analyze, relative to the checkout. |

The action exposes `exit-code` and `sarif-file` as outputs for later steps.

## Analyzer cache

The action caches `.bifrost/cache` at the checkout root. The database keys rows by Git blob object ID, so a cache saved on one branch stays valid for every file that other branches did not change. A pull-request run restores the most recent cache and re-analyzes only the files it changed. Keep the workflow enabled on the default branch so pull requests always find a warm cache to restore.

## New findings on pull requests

Code scanning compares the alerts from the pull-request analysis with the base branch and marks new alerts on the pull request. Upload from both the default branch and pull requests, with the same `category`, to get that comparison.

## Gate only on findings the pull request introduced

By default the gate fails on every finding, including pre-existing debt. To fail a pull request only on the findings it introduced, pass the pull request's base commit as `diff-base`. Bifrost then evaluates the same policies against that commit's content, classifies every finding as `new` or `persisting` by its stable finding identity, and computes the exit code from the new findings only. Each SARIF result carries the standard `baselineState` field (`new` or `unchanged`).

```yaml
jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - uses: BrokkAi/bifrost/.github/actions/policy-scan@v0.8.24
        with:
          diff-base: ${{ github.event.pull_request.base.sha }}
```

The checkout must contain the base revision: `fetch-depth: 0` is the simple form, or fetch the base SHA explicitly. If the base revision does not resolve, the run exits `2` (unreliable) rather than silently gating on everything. If the base revision resolves but its evaluation cannot prove its own completeness, the run degrades to full gating and reports a `diff-base-unreliable` diagnostic, so a broken base can never hide new findings.

Two identity limitations are accepted: a pure file rename re-keys every finding in the file (one `fixed` plus one `new`), and inserting an identical duplicate of an existing source slice above it under the same owner can shift the ordinal that distinguishes the duplicates and misclassify one pair.

## Suppressions and scope

The run honors `.bifrost/suppressions.json` and `.bifrost/policy-scope.json` from the workspace, exactly as the CLI does. Accepted suppressions appear in the SARIF report as suppressed results. See [Static-Analysis Policies](/static-analysis-policies/) for the document formats.
