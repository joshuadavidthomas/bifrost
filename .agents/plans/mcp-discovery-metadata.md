# Improve MCP discovery metadata

This ExecPlan is a living document. Keep the `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` sections current. Maintain this document in accordance with `.agents/PLANS.md`.

## Purpose / Big Picture

Bifrost sends a short MCP server instruction that does not tell coding agents when to find its tools. Codex and Claude Code use this instruction during deferred tool discovery. Oh My Pi adds it to the model prompt. The original Pi client has no general MCP integration, so the Bifrost Pi extension must forward it.

After this change, Bifrost sends a concise instruction that matches the tools selected for the process. The general routing text fits in the first 512 characters. The full instruction and each tool description fit in 2,000 characters. A Pi user receives the same instruction through the existing native extension.

## Progress

- [x] (2026-08-11 00:20Z) Confirmed current server, registry, Pi extension, and host behavior.
- [x] (2026-08-11 00:26Z) Generated accurate server instructions from effective toolsets.
- [x] (2026-08-11 00:28Z) Enforced metadata limits and shortened the oversized `query_code` description.
- [x] (2026-08-11 00:30Z) Forwarded server instructions through the native Pi extension.
- [x] (2026-08-11 00:32Z) Updated host documentation and behavior tests.
- [x] (2026-08-11 00:36Z) Ran focused tests, full MCP tests, documentation checks, Pi tests, and workspace clippy.

## Surprises & Discoveries

- Observation: Bifrost already returns `InitializeResult.instructions`, but the text is only one generic sentence.
  Evidence: `crates/bifrost-mcp/src/mcp_common.rs` defines `SEARCHTOOLS_INSTRUCTIONS` as "Analyzer-backed search tools for source code workspaces."
- Observation: The tool catalog is fixed after process start.
  Evidence: `BifrostMcpHandler::list_tools` documents this property and returns the complete unpaginated list.
- Observation: Oh My Pi reads server instructions without a Bifrost adapter change. Original Pi needs the Bifrost extension.
  Evidence: The local extension uses the MCP SDK but exposes only `connect`, `listTools`, and `callTool` from its client wrapper.
- Observation: The existing `query_code` description exceeded the host limit.
  Evidence: The first metadata test reported 2,226 characters. The revised text keeps retrieval terms and moves field detail to the existing schema.
- Observation: Named workspace mode appended every absolute workspace path after specification validation.
  Evidence: `NamedWorkspaceRouter::instructions` could exceed 2,000 characters. Tool schemas already carry the permitted workspace names.
- Observation: Documentation dependencies were not installed in the checkout.
  Evidence: The first `astro check` failed with `astro: command not found`. `npm --prefix docs ci` installed the locked dependencies.

## Decision Log

- Decision: Use standard MCP instructions and tool metadata. Do not add a discovery RPC.
  Rationale: Current Codex and Claude Code build their discovery indexes from standard MCP initialization and tool-list metadata.
  Date/Author: 2026-08-11 / Codex
- Decision: Generate capability sentences only for effective toolsets that advertise at least one tool.
  Rationale: An NLP build can omit `semantic_search` for a non-git workspace or unavailable runtime. The instruction must not claim an absent capability.
  Date/Author: 2026-08-11 / Codex
- Decision: Keep the catalog static and do not advertise `listChanged` or Anthropic `alwaysLoad` metadata.
  Rationale: Bifrost does not change its tool list after startup, and no tool must occupy every prompt.
  Date/Author: 2026-08-11 / Codex
- Decision: Enforce 2,000 Unicode characters for server instructions and each tool description.
  Rationale: Claude Code truncates this metadata at that boundary. A startup error prevents silent retrieval loss.
  Date/Author: 2026-08-11 / Codex
- Decision: Do not put absolute named workspace paths in server instructions.
  Rationale: Each tool schema already gives the allowed names. Removing paths keeps instructions bounded and reduces unnecessary path disclosure.
  Date/Author: 2026-08-11 / Codex
- Decision: Group registry expansion collections in `ServerSpecResolution`.
  Rationale: Toolset discovery state belongs to the same expansion operation. This keeps function interfaces small and clear.
  Date/Author: 2026-08-11 / Codex

## Outcomes & Retrospective

The server now returns routing instructions that match its advertised tools. The common routing paragraph fits within 512 characters. The final instruction and every tool description fit within 2,000 characters.

The Pi extension now reads, sanitizes, bounds, and forwards the standard MCP instruction. Codex, Claude Code, and Oh My Pi use the same server metadata without host-specific server behavior.

All focused and package-level validation passed. No custom discovery method, list-change capability, or forced-load metadata was added.

## Context and Orientation

`crates/bifrost-mcp/src/mcp_registry.rs` expands command-line expressions such as `core|nlp` into tool descriptors. A descriptor contains a tool name, description, and JSON input schema. `crates/bifrost-mcp/src/mcp_common.rs` stores those descriptors in `McpServerSpec`. `crates/bifrost-mcp/src/rmcp_host.rs` converts the specification into the MCP initialize result and `tools/list` result.

An MCP server instruction is short model guidance returned during initialization. It tells a coding agent when the server can help. Deferred discovery means the host initially hides full tool schemas and searches the catalog only when needed.

`plugins/bifrost-agent/extensions/bifrost-session.ts` adapts Bifrost MCP tools into native Pi tools. `plugins/bifrost-agent/extensions/bifrost.ts` adds a fixed Pi namespace note to the model prompt. The MCP SDK already stores initialize instructions and exposes them through `Client.getInstructions()`.

## Plan of Work

First, change the MCP specification to own its instruction string. Track which expanded toolsets produced advertised descriptors. Build one fixed routing paragraph, then append short sentences for those effective toolsets. Keep all important general terms in the first 512 characters. Validate instruction and tool-description lengths while building the specification. Shorten a description only when the new test identifies an overflow.

Second, extend the Pi client interface with `getInstructions`. Read the text after the MCP connection starts. Store sanitized and bounded text in connected session state and expose it through status. Add it to the system prompt before the existing Pi namespace and workspace note.

Third, update the MCP, Codex, Claude Code, Oh My Pi, and Pi documentation. Explain that standard MCP instructions route the server, while names and descriptions retrieve individual tools. State that Bifrost has a static catalog and sends no list-change notification.

Finally, add behavior tests. Rust tests must prove accurate capability text, both size boundaries, and unchanged static capabilities. TypeScript tests must prove that Pi receives server instructions and still works when they are absent.

## Concrete Steps

Run all commands from `/Users/jonathan/Projects/bifrost`.

Edit the Rust MCP registry and common specification. Then run:

    cargo fmt --check
    cargo test -p brokk-bifrost-mcp mcp_registry
    cargo test -p brokk-bifrost-mcp rmcp_host

Edit the Pi extension. Then run:

    npm --prefix plugins/bifrost-agent run check
    npm --prefix plugins/bifrost-agent test

Run the combined focused validation after documentation updates. The commands must complete successfully without failures.

Completed validation:

    cargo test -p brokk-bifrost-mcp
    test result: ok. 130 passed
    test result: ok. 32 passed

    npm --prefix plugins/bifrost-agent run check
    npm --prefix plugins/bifrost-agent test
    tests 118; pass 118; fail 0

    npm --prefix docs run check
    0 errors, 0 warnings, 0 hints

    npm --prefix docs run build
    Checked 6427 internal docs links across 62 HTML files

    cargo clippy --workspace --all-targets -- -D warnings
    Finished successfully

## Validation and Acceptance

Resolving `symbol` must return the general routing paragraph and symbol guidance. It must not claim policy or semantic-search support. Resolving an effective `nlp` toolset must mention natural-language semantic search. Resolving NLP without an advertised tool must not mention it.

Every server instruction and every advertised tool description must contain at most 2,000 Unicode characters. Specification construction must report the named offending field if this condition fails. The initialize response must return the generated instruction and keep the tools capability free of `listChanged`.

The Pi extension test must connect to a fake client with server instructions. Its next `before_agent_start` result must contain the instructions followed by the fixed Pi note. A fake client without instructions must still produce the fixed note.

## Idempotence and Recovery

All edits and tests are safe to repeat. Cargo must use the repository target directory. Do not direct build output to `/tmp`. If a test reveals an existing description above the limit, shorten that description without changing its tool name or behavior.

## Artifacts and Notes

The intended instruction starts with a self-contained paragraph similar to this text:

    Semantic source-code analysis and repository navigation. Search this server for language-aware answers about code structure, symbols, definitions, references, callers, types, related files, and available analysis features. Use these tools when text search cannot reliably answer cross-file or structural questions. Check result completeness before you claim all results or no results.

The exact final text can change for clarity, but these routing concepts and limits are acceptance requirements.

## Interfaces and Dependencies

`McpServerSpec.instructions` becomes `String`. The specification builders accept an owned or borrowed instruction and validate it. The registry adds an internal instruction builder that receives the effective toolset names.

`BifrostSessionClient` adds `getInstructions(): string | undefined`. `BifrostSessionStatus` adds optional `instructions`. The SDK-backed client implements the method with `Client.getInstructions()`.

No new dependency is required. Tool names and MCP request or response shapes remain unchanged.

Plan revision note, 2026-08-11: Marked implementation complete. Added the actual metadata overflow, named-workspace decision, and validation evidence.
