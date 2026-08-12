# Add MCP Tasks: durable async execution for run_policy

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `.agents/PLANS.md` at the repository root.

Tracking issue: https://github.com/BrokkAi/bifrost/issues/2006

## Purpose / Big Picture

Today every Bifrost MCP tool call must finish inside one interactive request: the server executes the tool, the client blocks on the JSON-RPC response, and a call that outlives the request budget is cancelled. That is right for navigation lookups, but wrong for explicitly batch-shaped work. The issue names large policy runs as the motivating case: `run_policy` over a big repository can legitimately exceed a connection or proxy timeout.

The MCP Tasks extension (`io.modelcontextprotocol/tasks`, SEP-2663) fixes this. A server may answer an eligible `tools/call` with a durable task handle (`resultType: "task"` plus a `task` object carrying `taskId`, `ttlMs`, `pollInterval`) instead of a result. The client polls `tasks/get` until the task reaches a terminal status (`completed`, `failed`, `cancelled`), and the terminal `tasks/get` response embeds exactly the `CallToolResult` the synchronous call would have produced. `tasks/cancel` requests cooperative cancellation. `tasks/update` delivers client answers to mid-task input requests (Bifrost does not issue any in this initial version, so `tasks/update` simply reports there is nothing to answer).

After this change, a `2026-07-28` client that declares the tasks extension in its capabilities and calls `run_policy` receives a task handle immediately, keeps its connection responsive, and collects the finished policy report by polling -- even after the initiating request has long since been answered. Clients that do not declare the extension see no behavioral change at all.

## Progress

- [x] (2026-08-12 16:20Z) rmcp 3.0.1 tasks surface surveyed; all findings recorded in Context and Orientation.
- [x] (2026-08-12 16:30Z) ExecPlan drafted.
- [x] (2026-08-12 16:45Z) Milestone 1: server plumbing (Arc'd analyzer pool, TaskManager field, TaskScopes, capability advertisement in get_info).
- [x] (2026-08-12 16:50Z) Milestone 2: task creation branch in call_tool plus run_tool_as_task; compiled clean on first check.
- [x] (2026-08-12 16:50Z) Milestone 3: tasks/get, tasks/update, tasks/cancel handlers delegating to the manager behind check_task_authorization.
- [x] (2026-08-12 16:56Z) Milestone 4: six behavior tests pass (the planned seven scenarios; unsupported-2026 and legacy-2025 clients share one test).
- [x] (2026-08-12 17:00Z) Milestone 5: Tasks section added to docs/src/content/docs/mcp.md.
- [x] (2026-08-12 17:05Z) Full bifrost-mcp suite green locally (126 unit + 43 integration); fmt clean.
- [x] (2026-08-12 17:15Z) Workspace clippy (--all-targets --all-features) clean: zero warnings, exit 0.

## Surprises & Discoveries

- Observation: rmcp 3.0.1 already ships a complete server-side task runtime (`rmcp::task_manager::TaskManager`), including TTL sweeping, terminal-result retention for one extra TTL window, input routing, and cooperative cancel signals. Bifrost only supplies the operation future and the authorization policy.
  Evidence: `~/.cargo/registry/src/*/rmcp-3.0.1/src/task_manager.rs` (998 lines, documented behavior).
- Observation: rmcp's dispatch already refuses `CallToolResponse::Task` to a client that did not declare the tasks capability, and refuses `tasks/get`/`update`/`cancel` from such clients, so Bifrost's own gating is defence in depth rather than the only wall.
  Evidence: `handler/server.rs` lines 204-245 (`client_declared_tasks` guard, `validate_tasks_capability`).
- Observation: `ServerResult::task_ack` wraps `tasks/update` and `tasks/cancel` acknowledgements; the update handler returns `GetTaskResult` internally but the ack on the wire is an empty result.
- Observation: `CreateTaskResult` serializes the task fields flattened at the top level of `result` (`result.taskId`, `result.ttlMs`), not nested under a `task` object as the plan's Validation section first assumed. Four tests failed on the nesting and passed once the asserts read the flattened fields.
  Evidence: wire response `{"result":{"resultType":"task","status":"working","taskId":"307a9276-...","ttlMs":600000,...}}`.
- Observation: the `run_policy` tool lives in the `extended` toolset, so the rootless rebinding test must spawn `--mcp "workspace|symbol|extended"`; there is no toolset named `policy`.
- Observation: an MRTR roots-activation retry from a tasks-capable client flows naturally into task creation -- the retry carries the roots, binds the workspace during activation, and the same request then task-ifies. No special interplay code was needed; the rebinding test pins this end to end.

## Decision Log

- Decision: The initial eligible tool set is exactly `{run_policy}`.
  Rationale: the issue directs "start narrow" from measured long-running use cases and names "large policy/corpus analysis" as the example. `run_policy` is the one registry tool whose runtime scales with the whole corpus times the policy count. Navigation tools stay synchronous by design ("do not turn normal symbol/navigation calls into tasks merely because cold initialization is slow").
  Date/Author: 2026-08-12 / Claude (Dave's session)
- Decision: Task creation is server-directed and deterministic: a request task-ifies exactly when (a) the request's client capabilities declare `io.modelcontextprotocol/tasks`, (b) the negotiated or per-request protocol revision is `2026-07-28` or newer, (c) the tool is in the eligible set, and (d) the server is a single-workspace rmcp host (not named-workspace mode). There is no per-request opt-out knob.
  Rationale: rmcp 3.0.1 models no per-request task request field on `CallToolRequestParams`; SEP-2663 makes task-shaped responses a server decision gated on client capability. Determinism (capability implies task for eligible tools) keeps the contract testable and predictable; a client that declares the capability on a request is opting in for that request. Named-workspace mode is excluded to keep scope narrow; it can be added later by recording the workspace id in the task scope, which the scope type already carries.
  Date/Author: 2026-08-12 / Claude
- Decision: Argument validation and workspace authorization stay synchronous; only the execution phase becomes a task.
  Rationale: a malformed call should fail immediately with the same message the synchronous path gives, not park the error inside a task the client must poll to discover. Authorization (workspace binding, MRTR activation) must happen under the workspace lock in the request that carries it.
  Date/Author: 2026-08-12 / Claude
- Decision: Task TTL defaults to 600000 ms (10 minutes) and is overridable through the `BIFROST_MCP_TASK_TTL_MS` environment variable (whole milliseconds, minimum 1); the advertised poll interval is fixed at 1000 ms. The TTL is also the execution deadline: the task's Bifrost cancellation token carries `deadline = created + ttl`.
  Rationale: tasks exist precisely because the 5-second interactive budget is wrong for batch work, so they need their own bound. Ten minutes covers large policy runs while still guaranteeing cleanup. Making TTL the cancellation deadline keeps "rmcp marks the task failed at TTL" and "the analyzer actually stops at TTL" the same moment instead of two drifting timers. The env override exists so the expiry test can use a tiny TTL without waiting minutes.
  Date/Author: 2026-08-12 / Claude
- Decision: Each task is bound at creation to the workspace generation it was authorized under (`WorkspaceRequestScope`), recorded in a handler-side map keyed by task id. Every `tasks/get`, `tasks/update`, and `tasks/cancel` first checks that recorded generation against `service.workspace_generation()`; a mismatch cancels the task (if still running) and answers with an invalid-params error naming revoked workspace authorization. The running task also registers its cancellation token in the existing `InFlightRequests` registry, so a workspace rebind cancels the underlying analyzer work promptly exactly as it does for synchronous calls.
  Rationale: the issue requires that a handle can never be reused across workspace boundaries. Generation numbers already are Bifrost's authorization epoch; both the prompt-cancel path (in-flight registry) and the polling gate (scope map) reuse them, so tasks are exactly as strict as the synchronous path.
  Date/Author: 2026-08-12 / Claude
- Decision: The durability boundary is the process. Bifrost's MCP transport is stdio: one connection is one process, so "durable" means the task survives the initiating request, not a process restart. If a launcher restarts the process every handle is gone and polling it in the new process fails with unknown-task. This is stated in the docs.
  Rationale: the issue demands the boundary be decided explicitly rather than implied. Process-local is what the stdio transport can honestly deliver; persisting task state to disk buys nothing while the analyzer state it would resume is equally process-local.
  Date/Author: 2026-08-12 / Claude
- Decision: At most `MAX_QUEUED_ANALYZER_REQUESTS` (8) non-terminal tasks may exist at once; task creation beyond that fails with the same "too many analyzer requests are queued" protocol error the synchronous path uses for a saturated pool.
  Rationale: bounded storage is required by the issue; tying the bound to the analyzer backlog reuses an existing, already-documented limit instead of inventing a second knob. The task future still queues for a real analyzer permit inside the pool, so this cap only bounds bookkeeping, not analyzer pressure.
  Date/Author: 2026-08-12 / Claude
- Decision: No `notifications/tasks` push notifications in this version; polling only. `tasks/update` is implemented but, since Bifrost issues no mid-task input requests yet, any update necessarily names an unknown or non-outstanding key and is answered by rmcp's own error.
  Rationale: polling is the required minimum of the extension and the only part the acceptance criteria exercise. Push status is additive later.
  Date/Author: 2026-08-12 / Claude

## Outcomes & Retrospective

Implementation completed 2026-08-12, same session as the draft. Against the issue's acceptance criteria:

- `run_policy` completes through a task handle and is polled to `completed` after the initiating request ended; the terminal `tasks/get` embeds the same content/structured-content payload as a synchronous call, pinned by comparing the embedded report against a direct in-process `evaluate_policy_files` run (`mcp_tasks_run_policy_completes_through_a_task_handle`).
- Unsupported clients never receive task results: a `2026-07-28` client without the capability and a legacy `2025-11-25` client with it both stay synchronous, and `tasks/get` without the capability is refused at dispatch (`mcp_tasks_never_reach_incapable_or_legacy_clients`).
- Unknown ids fail safely on all three task methods; an expired task (TTL forced to 300 ms via `BIFROST_MCP_TASK_TTL_MS`) is observed as terminal `failed`; `tasks/cancel` settles a running task as `cancelled`; and a handle polled after a roots revocation is refused with the workspace-revocation error while the revocation also cancels the work through the in-flight registry (`mcp_tasks_unknown_ids_fail_safely`, `mcp_tasks_expire_to_failed`, `mcp_tasks_cancel_settles_cancelled`, `mcp_tasks_handles_die_with_workspace_rebinding`).
- Execution respects analyzer admission (the task future queues for a real permit), the TTL doubles as the cooperative-cancellation deadline, and validation errors stay synchronous.
- `docs/src/content/docs/mcp.md` states the eligible tool set, the TTL/poll/retention numbers, and the process-local durability boundary.

Gaps intentionally left, all recorded in the Decision Log: named-workspace mode stays synchronous, no `notifications/tasks` push, no mid-task input requests. Lesson: rmcp 3.0.1's `TaskManager` carried nearly all lifecycle burden; the Bifrost work was almost entirely authorization policy, which is where it belongs. The one planning error worth remembering is assuming a nested `task` object in `CreateTaskResult` instead of checking the serializer first -- four tests failed on exactly that.

## Context and Orientation

Bifrost is a code-intelligence server. Its MCP host lives in `crates/bifrost-mcp/src/rmcp_host.rs` and is built on `rmcp` 3.0.1 (the official Model Context Protocol Rust SDK, pinned in `crates/bifrost-mcp/Cargo.toml`). One `BifrostMcpHandler` instance serves one stdio connection; the process serves exactly one connection and exits when stdin closes.

Key pieces of the existing host, all in `crates/bifrost-mcp/src/rmcp_host.rs` unless noted:

- `BifrostMcpHandler::call_tool` is the entry point for `tools/call`. It authorizes the workspace (`prepare_tool_call`, which runs under the async `workspace` mutex), then waits for workspace readiness, acquires an analyzer permit from `analyzer_pool` (an `AnalyzerExecutionPool` from `crates/bifrost-mcp/src/analyzer_pool.rs` bounding concurrent analyzer executions to `ANALYZER_POOL_CAPACITY` with a backlog bound of `MAX_QUEUED_ANALYZER_REQUESTS`), and finally runs the tool body on the blocking pool via `execute_tool`.
- `InFlightRequests` (same file) maps running requests to `(WorkspaceRequestScope, CancellationToken)`. `WorkspaceRequestScope` is `{workspace_id, generation}` where `generation` is `service.workspace_generation()`, a counter that increments whenever the bound workspace changes. When the workspace rebinds, `cancel_stale` cancels every token whose generation differs -- this is how Bifrost stops analyzer work the client no longer authorizes.
- `ProgressReporter` (same file) sends `notifications/progress` for bounded synchronous calls; tasks are the heavier mechanism and progress notifications remain the lighter one, per the issue.
- Tool output mapping: `tool_success_result` and `tool_error_result` convert a `crate::ToolOutput` or an error message into `rmcp::model::CallToolResult`. `map_service_error` converts `SearchToolsServiceError` codes into protocol errors.
- `run_policy` executes policy packs; its output is post-processed by `attach_run_policy_correlation` keyed by a hash of the request id.

What rmcp 3.0.1 already provides for tasks (verified by reading `~/.cargo/registry/src/index.crates.io-*/rmcp-3.0.1/src/`):

- `rmcp::task_manager::TaskManager`: cheaply cloneable store + executor. `spawn(TaskOptions, |TaskContext| -> TaskFuture) -> Task` creates a durable entry before returning (the spec requires `tasks/get` to resolve as soon as the client holds the handle). `get_task`, `update_task`, `cancel_task` implement the spec semantics including TTL expiry (non-terminal tasks past TTL are marked `failed` and their tokio task aborted) and terminal retention (entries evicted one TTL window after the terminal transition). `running_task_count()` counts non-terminal entries. There is no background sweeper; sweeps happen opportunistically on every manager call.
- `TaskContext`: `cancelled().await` resolves when `tasks/cancel` arrives; `is_cancel_requested()`; `set_status_message(...)`. The operation future returns `Result<CallToolResult, TaskExit>` where `TaskExit::Cancelled` settles the task `cancelled` and `TaskExit::Error(ErrorData)` settles it `failed`.
- Models in `rmcp::model`: `CreateTaskResult` (wire shape `resultType: "task"` + `task`), `GetTaskParams/GetTaskResult` (`DetailedTask` = `Task` + status payload; terminal payload embeds the `CallToolResult`), `UpdateTaskParams`, `CancelTaskParams`. `CallToolResponse::Task(CreateTaskResult)` is the handler return variant.
- Capability plumbing: `ServerCapabilities::builder().enable_tasks()` puts `io.modelcontextprotocol/tasks: {}` into `capabilities.extensions`; `ClientCapabilities::supports_tasks()` checks the same key on the client side. The wire JSON for a client is `"capabilities": {"extensions": {"io.modelcontextprotocol/tasks": {}}}` (or the same object under the per-request `_meta` key `io.modelcontextprotocol/clientCapabilities` for stateless `2026-07-28` requests). rmcp's dispatch refuses a task-shaped `tools/call` response to a client that did not declare the capability, and refuses `tasks/*` methods from such clients, independent of anything Bifrost does.
- `server/discover` and `initialize` both derive their capability advertisement from `ServerHandler::get_info`, so enabling the extension there advertises it on both entry paths.

The wire-level integration test file is `crates/bifrost-mcp/tests/bifrost_mcp_server.rs`. It spawns the real server binary over stdio (`spawn_server`, `spawn_rootless_server`), writes JSON-RPC lines, and reads responses (`round_trip`, `read_line`). Test workspaces are built with `InlineTestProject` from `tests/common/inline_project.rs`. Policy fixtures for `run_policy` can be provided by writing an `.rqlp` file under `<workspace>/.bifrost/policies/` (see existing test `bifrost_mcp_run_policy_uses_the_active_snapshot_and_durable_suppressions` for the shape) or by running a built-in policy id from `list_policies`.

## Plan of Work

Milestone 1 -- plumbing. In `crates/bifrost-mcp/src/rmcp_host.rs`: change `BifrostMcpHandler.analyzer_pool` from `AnalyzerExecutionPool` to `Arc<AnalyzerExecutionPool>` (task futures are `'static` and must own their admission path); add fields `task_manager: rmcp::task_manager::TaskManager` and `task_scopes: Arc<Mutex<HashMap<String, WorkspaceRequestScope>>>`; add a `tasks_active(&self) -> bool` helper true exactly when `named_workspaces.is_none()`; in `get_info`, when tasks are active, set `capabilities.extensions` to declare `io.modelcontextprotocol/tasks` (mutate the built `InitializeResult` because the type-state builder cannot branch). Add module constants `MCP_TASK_TTL_ENV = "BIFROST_MCP_TASK_TTL_MS"`, `DEFAULT_TASK_TTL: Duration = 600s`, `TASK_POLL_INTERVAL_MS: u64 = 1000`, and a `fn mcp_task_ttl() -> Duration` reading the env var (whole milliseconds, minimum 1, invalid values fall back to the default with a stderr note).

Milestone 2 -- task creation. In `call_tool`, after `prepare_tool_call` has produced `PreparedToolCall::Ready { arguments, workspace_scope }` and the serial guard has been dropped, insert the task branch before the readiness wait: if `tasks_active()`, the tool name is `run_policy`, `speaks_2026_07_28(&context)`, and `context.client_capabilities().is_some_and(|c| c.supports_tasks())`, then create the task instead of executing inline. Creation: refuse with the existing saturated-pool error when `task_manager.running_task_count() >= MAX_QUEUED_ANALYZER_REQUESTS`; compute `ttl = mcp_task_ttl()`; clone the Arcs the future needs (service, analyzer pool, in-flight registry, render options copy, correlation id, tool name, arguments, scope); call `task_manager.spawn(TaskOptions with ttl_ms/poll_interval/status message, |task_context| Box::pin(run_tool_as_task(...)))`; record `task_scopes[task.task_id] = scope`; return `Ok(CallToolResponse::Task(CreateTaskResult::new(task)))`.

`run_tool_as_task` is a new free async function in the same file. It mirrors the synchronous execution path with task-mode bounds: build a Bifrost `CancellationToken` with `deadline = now + ttl`; spawn a bridge that cancels it when `task_context.cancelled()` resolves; register it in the in-flight registry under the recorded scope (guard held for the duration of the blocking work, exactly like `execute_tool`); wait workspace readiness on the blocking pool (bounded by the same token); acquire an analyzer permit from the pool (no separate admission timeout -- the deadline token bounds the whole task); re-check the workspace generation before and after execution; run `service.call_tool_output_with_cancellation` on the blocking pool; apply `attach_run_policy_correlation`; map success through `tool_success_result`, user-actionable failures (unknown tool) through `tool_error_result`, and everything else through `map_service_error` into `TaskExit::Error`. If the token was cancelled because `tasks/cancel` arrived (`task_context.is_cancel_requested()`), return `TaskExit::Cancelled`; a deadline expiry or workspace revocation returns `TaskExit::Error` with a message naming the cause.

Milestone 3 -- task methods. Implement `ServerHandler::get_task`, `update_task`, `cancel_task` on `BifrostMcpHandler`. Each first calls a shared `check_task_scope(&self, task_id) -> Result<(), ErrorData>`: unknown id falls through to the manager (which answers with its own not-found error); a known id whose recorded generation differs from `service.workspace_generation()` gets `ErrorData::invalid_params("the task's workspace authorization was revoked; the handle is no longer valid")` after `task_manager.cancel_task(id)` (ignoring its error if already terminal). Then delegate: `get_task` -> `GetTaskResult::new(manager.get_task(id)?)`; `update_task` -> `manager.update_task(id, responses)` discarding the returned detail (the wire ack is empty); `cancel_task` -> `manager.cancel_task(id)`. Entries in `task_scopes` are removed opportunistically when `get_task` observes a terminal status and on `cancel_task`, bounding the map by the manager's own retention.

Milestone 4 -- tests, in `crates/bifrost-mcp/tests/bifrost_mcp_server.rs` (wire level, following the file's existing helpers). A small local helper declares the tasks capability in `initialize` and polls `tasks/get` until terminal with a bounded loop. Scenarios: (1) completion -- capable `2026-07-28` client calls `run_policy` on a workspace with a repo policy fixture, receives `resultType: "task"`, polls to `completed`, and the embedded result equals the synchronous result of the same call made by a capability-less client (same content and structuredContent shape); (2) unsupported client -- no capability, same call, synchronous `CallToolResult`, and `tasks/get` from that client is refused; (3) legacy client -- `2025-11-25` with the capability still gets synchronous results; (4) cancellation -- start a task, `tasks/cancel` immediately, poll observes terminal `cancelled`; (5) expiry -- server spawned with `BIFROST_MCP_TASK_TTL_MS=300` against a slow-enough call, poll after the TTL observes terminal `failed`; (6) unknown id -- `tasks/get` with a made-up id errors; (7) workspace rebinding -- rootless server bound via client roots, create a task, send `notifications/roots/list_changed` and rebind to a different root with a second call, then `tasks/get` on the old handle fails with the revocation error.

Milestone 5 -- documentation. Add a "Tasks" section to `docs/src/content/docs/mcp.md` after the Progress section: eligible tool set (`run_policy` only), the gating conditions, TTL/poll/retention numbers and the `BIFROST_MCP_TASK_TTL_MS` override, the process-local durability boundary spelled out, the workspace-authorization rule, and the relationship to progress notifications.

## Concrete Steps

Work from the repository root on branch `dave/github-issue-2006-mcp-tasks`.

Build and focused tests while iterating:

    cargo check -p brokk-bifrost-mcp
    cargo test -p brokk-bifrost-mcp --test bifrost_mcp_server mcp_tasks

Full validation before push:

    cargo fmt
    cargo test -p brokk-bifrost-mcp
    cargo clippy --workspace --all-targets --all-features -- -D warnings

Expected: all bifrost-mcp tests pass (126 unit + 37 integration existed before this plan; the plan adds seven task tests), clippy is clean.

## Validation and Acceptance

Acceptance is behavioral, matching the issue:

- A `2026-07-28` client that declares `{"extensions": {"io.modelcontextprotocol/tasks": {}}}` and calls `run_policy` receives a response whose `result.resultType` is `"task"` with a `task.taskId`, then `tasks/get {taskId}` eventually returns `status: "completed"` with the policy report embedded exactly as a synchronous call would return it (same `content` text and `structuredContent`).
- The same call without the capability returns an ordinary synchronous `CallToolResult`; a `tasks/get` from that client is refused before reaching Bifrost's handler.
- `tasks/cancel` on a running task yields terminal `cancelled` and stops the analyzer work (the cancellation token registered in the in-flight registry is cancelled).
- With `BIFROST_MCP_TASK_TTL_MS=300`, a task that cannot finish in 300 ms is observed as terminal `failed` when polled after expiry.
- `tasks/get` with an unknown id, and with a handle created before a workspace rebind, both fail with typed errors and never return results computed for the old scope.

Run: `cargo test -p brokk-bifrost-mcp --test bifrost_mcp_server mcp_tasks` and expect all task tests to pass; each fails before the implementation exists.

## Idempotence and Recovery

All edits are additive to `crates/bifrost-mcp/src/rmcp_host.rs`, the test file, and the docs page; re-running the steps is safe. If a milestone breaks the existing suite, `git checkout -- <file>` restores the last committed state; commits land per milestone so recovery is always one commit away.

## Interfaces and Dependencies

No new crate dependencies: `rmcp` 3.0.1 already carries the tasks extension (models, dispatch gating, and `rmcp::task_manager::TaskManager`).

In `crates/bifrost-mcp/src/rmcp_host.rs`, at the end of the work these exist:

    struct BifrostMcpHandler {
        // ... existing fields ...
        analyzer_pool: Arc<AnalyzerExecutionPool>,
        task_manager: rmcp::task_manager::TaskManager,
        task_scopes: Arc<Mutex<HashMap<String, WorkspaceRequestScope>>>,
    }

    fn mcp_task_ttl() -> Duration;                  // env-overridable task TTL
    async fn run_tool_as_task(/* owned Arcs */) -> Result<CallToolResult, rmcp::task_manager::TaskExit>;

    impl ServerHandler for BifrostMcpHandler {
        // ... existing methods ...
        async fn get_task(&self, GetTaskParams, RequestContext<RoleServer>) -> Result<GetTaskResult, ErrorData>;
        async fn update_task(&self, UpdateTaskParams, RequestContext<RoleServer>) -> Result<GetTaskResult, ErrorData>;
        async fn cancel_task(&self, CancelTaskParams, RequestContext<RoleServer>) -> Result<(), ErrorData>;
    }

Note: the exact `update_task`/`cancel_task` return types follow `rmcp::ServerHandler`'s trait signatures in 3.0.1; the dispatch wraps them via `ServerResult::task_ack`.

---

Revision note (2026-08-12): initial draft. A first version of this file pre-filled Progress and Outcomes as if the work were done; that was wrong and was corrected before any implementation commit -- Progress now reflects only the survey and drafting actually completed.
