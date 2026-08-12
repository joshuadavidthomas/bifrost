# Register Bifrost with native MCP hosts

This ExecPlan is a living document. Keep `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` current.

Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

After this change, a user can run `bifrost --install`. Bifrost then registers a user-scoped MCP server named `brokk` with each supported coding host that is installed. The registration starts the current Bifrost executable with `--mcp core|nlp`. It does not install skills, instructions, plugins, packages, or host applications.

The supported native MCP hosts are Codex, Claude Code, OpenCode, Kimi Code, Hermes, and Oh My Pi. Original Pi has no native MCP server registration interface. Its Bifrost integration remains a separately managed Pi extension.

## Progress

- [x] (2026-08-10) Reviewed the CLI, current host documentation, old compatibility behavior, and clean-system test options.
- [x] (2026-08-10) Confirmed AWS access for isolated end-to-end tests. Podman is not installed on the development host.
- [x] (2026-08-10) Added the installer module and standalone `--install` dispatch.
- [x] (2026-08-10) Added behavior tests with isolated fake host commands and homes.
- [x] (2026-08-10) Updated CLI and installation documentation.
- [x] (2026-08-10) Tested clean and repeat installations on a temporary AWS system, then removed all temporary AWS and local access resources.
- [x] (2026-08-10) Prepared and validated the completed change for its final task commit.

## Surprises & Discoveries

- Observation: Original Pi does not have a native MCP registration command or configuration contract, but Oh My Pi does.
  Evidence: Original Pi loads integrations as extension packages. Oh My Pi documents native user MCP configuration at `~/.omp/agent/mcp.json` and profile-specific equivalents.

- Observation: Current OpenCode V2 documentation uses the `opencode2` executable and a user-scoped `mcp add --global` command.
  Evidence: The current OpenCode MCP server documentation gives this command and stores local server definitions under `mcp.servers`.

- Observation: Stable OpenCode provides only an interactive `opencode mcp add` wizard.
  Evidence: Its current CLI documentation gives no arguments for `mcp add`. Its documented user config stores servers directly under `mcp`.

- Observation: Explicit `bifrost --mcp` starts without a workspace root.
  Evidence: `src/bin/bifrost.rs` sets `initial_root` to `None` when MCP mode is explicit and `--root` is absent. The server then accepts a host-approved MCP root.

- Observation: Claude Code rejects a duplicate user-scoped server instead of updating it.
  Evidence: The second clean-system run returned `MCP server brokk already exists in user config`. The installer now removes that exact user entry and adds the current registration again.

- Observation: Hermes can exit successfully without saving a server when its connection probe fails and stdin supplies no answer.
  Evidence: The first clean-system attempt logged a Bifrost start but `hermes mcp list` remained empty. The installer now answers the save prompt and verifies that `hermes mcp list` contains `brokk`.

## Decision Log

- Decision: Use each host's native registration command.
  Rationale: The host owns its configuration format, migration rules, and user scope. Calling that interface avoids duplicate format implementations in Bifrost. Stable OpenCode and Oh My Pi are exceptions because they have no noninteractive registration command.
  Date/Author: 2026-08-10 / Codex

- Decision: Register the current executable by absolute path.
  Rationale: A coding host can start outside the shell that installed Bifrost. An absolute path does not depend on that host's `PATH`.
  Date/Author: 2026-08-10 / Codex

- Decision: Register a rootless server with the `core|nlp` toolset expression.
  Rationale: A global registration must not bind to the directory where installation ran. The host supplies the active project through MCP roots.
  Date/Author: 2026-08-10 / Codex

- Decision: Register native MCP for Oh My Pi, but do not manage the original Pi extension from `--install`.
  Rationale: Oh My Pi has a generic MCP client and a documented native configuration. Installing the original Pi extension is a different operation.
  Date/Author: 2026-08-10 / Codex

## Outcomes & Retrospective

The implementation registers an absolute Bifrost executable as `brokk` with Codex, Claude Code, OpenCode, Kimi Code, Hermes, and Oh My Pi. It uses `--mcp core|nlp`, starts without a fixed workspace, skips missing hosts, reports each failure, and changes no skills or instructions. Stable OpenCode and Oh My Pi receive atomic JSON merges because their native registration interfaces are interactive. OpenCode V2 and the other four hosts use their current noninteractive management commands.

The clean AWS test used Ubuntu 24.04 and these host releases: Codex 0.147.0, Claude Code 2.1.226, OpenCode V2 `0.0.0-next-17114`, Kimi 1.49.0, Hermes 0.20.0, and Oh My Pi 17.2.12. The first and second installation runs both succeeded. Codex and Claude reported a connected server. OpenCode reported `connected`. Kimi showed the exact command. Hermes showed an enabled server with all discovered tools. Oh My Pi contained the exact native JSON entry. The stable OpenCode JSON path has a separate isolated behavior test.

The AWS instance `i-0288c3c2643f458e4` was terminated. Its security group, key pair, network interface, local private-key file, and local known-hosts file were removed. A final tag query found no active test instance.

## Context and Orientation

`src/bin/bifrost.rs` parses all user-facing arguments and starts MCP mode. A new small module under `src/` will describe supported hosts, find installed host commands, invoke their registration commands without a shell, and return one result per host. `src/bin/bifrost.rs` will call that module for the standalone `--install` action.

The integration tests belong in `tests/suite_mcp_cli/`. They will run the built Bifrost binary with an isolated home directory and a `PATH` that contains fake host executables. These fakes will record exact argument vectors. No test will read or write real host configuration.

The clean-system test will use a temporary AWS instance. It will install the five host CLIs, build or copy Bifrost, run `bifrost --install`, and inspect each host through its own list command. The test procedure must remove the instance at the end.

## Plan of Work

Add `--install` as a standalone action. Reject combinations with server, tool, policy, root, rendering, and workspace options. Resolve the running executable through `std::env::current_exe`, then call each installed host in a stable order. Skip a missing host. Continue after one host fails so the result reports all hosts. Return failure when a found host rejects registration or when no supported host is installed.

Use direct argument vectors for Codex, Claude Code, OpenCode, Kimi Code, and Hermes. Use the server name `brokk`. Preserve each host's user or global scope. Oh My Pi has no noninteractive registration command, so merge the server into its documented user MCP JSON file. Preserve unrelated JSON fields. Honor the active profile environment when present.

Add unit or integration behavior checks for exact command construction, missing hosts, partial failure, no-host failure, help text, and incompatible CLI options. Update the CLI and installation documentation with the supported-host list, root negotiation requirement, and Pi exclusion.

Provision a minimal temporary AWS Linux system. Install current releases of all six host CLIs in an isolated user account. Test registration on a clean home directory. Run each host's MCP list command and inspect its user configuration when the list output is not sufficient. Terminate the system and confirm termination.

## Concrete Steps

From the repository root:

    cargo fmt
    cargo test --test suite_mcp_cli bifrost_install

Use AWS CLI commands outside the restricted sandbox for system creation, status checks, and termination. Do not change local Codex, Claude, OpenCode, Kimi, Hermes, or Pi configuration.

## Validation and Acceptance

`bifrost --help` must describe `--install`. On a clean system with all six supported CLIs, one `bifrost --install` command must create a user-scoped server named `brokk` in all six hosts. Each registration must start the same absolute Bifrost executable with `--mcp core|nlp`.

The command must not write skills, instruction files, plugin files, project files, or Pi state. It must not bind the server to the installation directory. Focused Rust tests must pass. The temporary AWS system must be terminated after validation.

## Idempotence and Recovery

The host registration commands should support repeated registration. Clean-system tests will run the installer twice. If a host command does not replace an equal existing entry, the implementation will detect that host's documented duplicate behavior and use its safe update interface.

The installer attempts all found hosts even when one fails. Its final error lists failed hosts. A user can correct that host and run `bifrost --install` again.

AWS test resources use task-specific tags. Terminate all tagged instances after success or failure. Confirm that no tagged instance remains in a running or pending state.

## Artifacts and Notes

The server registration is logically equivalent to this process command:

    /absolute/path/to/bifrost --mcp core|nlp

The validated host registration forms are:

    codex mcp add brokk -- /absolute/path/to/bifrost --mcp core|nlp
    claude mcp add --transport stdio --scope user brokk -- /absolute/path/to/bifrost --mcp core|nlp
    opencode2 mcp add brokk --global -- /absolute/path/to/bifrost --mcp core|nlp
    kimi mcp add brokk -- /absolute/path/to/bifrost --mcp core|nlp
    hermes mcp add brokk --command /absolute/path/to/bifrost --args --mcp core|nlp

Stable OpenCode stores the equivalent local server under `mcp.brokk`. Oh My Pi stores it under `mcpServers.brokk` in the active agent directory reported by `omp config path`.

## Interfaces and Dependencies

The installer uses the Rust standard library and the existing `serde_json` dependency. It adds no network dependency. The main interface accepts the current executable path and invokes supported host commands or updates Oh My Pi's native JSON. Tests inject command lookup and execution boundaries so they do not use local host state.
