# Unify JSX attribute owner identity

This ExecPlan is a living document maintained according to `.agents/PLANS.md`. It fixes JSX attribute navigation and usage analysis so both directions identify a prop through the component declaration and its props type, rather than treating the attribute spelling as a file-wide bare name.

## Purpose / Big Picture

After this change, navigating from `<Local title={...} />` or `<Imported title={...} />` reaches the `title` field on that component's props type. Scanning usages of that field returns only attributes on the matching component. Intrinsic elements, external components, unresolved components, and unrelated components with the same attribute spelling produce no guessed definition or usage.

## Progress

- [x] (2026-07-12) Traced definition, target-centric usage, and inverted usage paths and identified their separate JSX handling.
- [x] (2026-07-13) Added one shared structured JSX attribute resolver to the JS/TS receiver provider, including typed parameters, React function-component wrappers, class components, imports, and cached prop owners.
- [x] (2026-07-13) Routed definition lookup, target-centric usage extraction, and both inverted usage scans through the shared resolver.
- [x] (2026-07-13) Added public local/imported positives plus exact same-name and unresolved-component negatives across definition, graph, and service tests.
- [x] (2026-07-13) Passed focused and full regression suites, formatting, all-feature clippy, release build, and both Kibana exact-site reruns.

## Surprises & Discoveries

- Observation: Existing JSX handlers resolve only the component element name. Attribute names either fall into flat definition lookup or are never emitted by inverse usage analysis.
  Evidence: `handle_jsx_element`, `handle_jsx`, and `handle_scoped_jsx` only call `rightmost_jsx_identifier` on the element's `name` field.

- Observation: Indexed exported const components map to a surrounding `lexical_declaration`, not directly to the `variable_declarator` that owns `React.FC<Props>`.
  Evidence: The new imported `React.FC<ChildProps>` definition regression failed until component declaration discovery iteratively descended the indexed range node.

- Observation: The service groups multiple source ranges in one enclosing callable into one rendered hit.
  Evidence: Splitting the two positive `Child` attributes into `ViewOne` and `ViewTwo` makes the public negative test prove exactly two enclosings while excluding `OtherView` and `ExternalView`.

## Decision Log

- Decision: Represent a JSX attribute lookup as `Option<Vec<CodeUnit>>`, where `None` means the syntax is not an attribute and `Some(empty)` means it is an attribute whose exact prop owner cannot be proven.
  Rationale: Definition lookup must continue for ordinary names but fail closed for recognized JSX attributes, including package components and intrinsic elements.
  Date/Author: 2026-07-12 / Codex

- Decision: Put component-to-props resolution on `JsTsReceiverFactProvider`.
  Rationale: This existing provider already owns the parsed tree, import binder, alias resolver, definition index, and structured TypeScript property-owner resolution used by forward and inverse paths.
  Date/Author: 2026-07-12 / Codex

- Decision: Cache component prop-owner identities per provider and use iterative AST walks for declaration and qualified-type discovery.
  Rationale: Inverted scans reuse a provider across a file, so repeated attributes must not repeatedly read and parse the component source; AST depth is input-shaped and must remain stack-safe.
  Date/Author: 2026-07-13 / Codex

## Outcomes & Retrospective

Forward and inverse JSX attribute identity now share one structured resolver. Local typed functions, imported `React.FC` consts, same-name unrelated components, and unresolved components are covered through public APIs. The final Kibana run made external `EuiSelectable.onChange` fail closed and produced an exact inverse hit for `GrokPatternSuggestionProps.simulationResult`; both sites had zero actionable findings.

## Context and Orientation

`src/analyzer/usages/get_definition/js_ts.rs` resolves a source reference to indexed declarations. `src/analyzer/usages/js_ts_graph/extractor.rs` scans candidate files for usages of one target declaration. `src/analyzer/usages/js_ts_graph/inverted.rs` builds all usage edges in one workspace scan. `src/analyzer/usages/js_ts_graph/receiver_analysis.rs` is shared structured JS/TS type and receiver analysis. A `CodeUnit` is Bifrost's indexed declaration identity; returning the exact prop `CodeUnit` prevents two components with a same-named prop from being conflated.

## Plan of Work

Extend `JsTsReceiverFactProvider` with a method that recognizes a tree-sitter `jsx_attribute`, resolves the opening element's simple local or imported component declaration, inspects the component's typed first parameter or recognized React function-component type wrapper, resolves the props type in the declaration file, and returns fields matching the attribute. Use the method before general definition lookup and from every JSX usage handler. Add public inline-project tests that prove local and imported positives and exact same-name and unresolved-component negatives.

## Concrete Steps

Work from `/home/jonathan/Projects/bifrost`. Run `cargo test --test get_definition_test`, `cargo test --test usages_js_ts_graph_test`, and the relevant service test executable. Then run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build --release`. Finally run the reference differential exact-site command for Kibana byte ranges 9397..9405 and 5111..5127 and retain the JSONL evidence.

## Validation and Acceptance

Acceptance requires public API tests to resolve local and imported JSX props to exact owner fields, usage tests to include only attributes on the matching component, and unresolved/external or unrelated same-name components to have no result. Both specified Kibana sites must report zero actionable differences. Formatting and clippy must complete without errors.

## Idempotence and Recovery

All test and build commands are repeatable. The implementation does not mutate fixture projects. If a focused test fails, rerun that executable with the test name and `--nocapture`; no cleanup is required beyond ordinary build artifacts.

## Artifacts and Notes

Full regression evidence: `get_definition_test` passed 423 tests; `searchtools_service` passed 137 with 1 ignored; `usages_js_ts_graph_test` passed 70 with 2 ignored. Final focused reruns passed 2 definition tests, 1 usage test, and 1 service test after the cache and iterative-walk review changes. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the release build passed.

Final exact evidence is `/tmp/issue678-jsx-final.jsonl`. `sort_renderer.tsx` bytes 9397..9405 reports `forward_status=no_definition`, `classification=inconclusive`, and zero missing/actionable findings. `grok_pattern_suggestion.tsx` bytes 5111..5127 resolves `GrokPatternSuggestionProps.simulationResult` and reports an exact-range `consistent` inverse hit, also with zero actionable findings.

Revision note (2026-07-13): Marked implementation complete and recorded public-suite, release, and exact Kibana evidence after the final stack-safety and cache review.

## Interfaces and Dependencies

`JsTsReceiverFactProvider::resolve_jsx_attribute_targets` will be the sole component/props identity resolver. It depends only on tree-sitter nodes, the existing JS/TS module binding resolver, `ts_resolve_type_text_to_property_owners`, and analyzer `CodeUnit` identities. It will not use regex, substring search, or a new source parser.
