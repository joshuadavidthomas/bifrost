//! Fixture corpus for flow-sensitive state and bounded rewrite termination
//! (issue #1480, Milestone 0).
//!
//! The plan is `.agents/plans/issue-1480-flow-sensitive-state-bounded-rewrite.md`.
//! Milestone 0 writes the fixtures before any engine code, because the sibling
//! plans repeatedly found that the fixture shapes drive the row vocabulary
//! rather than the other way around.
//!
//! Two kinds of test live here:
//!
//! - Feasibility probes that run today. They assert that the fixture procedure
//!   lowers to a production CFG, because the flow relations of Milestone 2 have
//!   no other evidence source. A probe that stopped lowering would silently
//!   remove the evidence the whole family stands on.
//! - Acceptance fixtures, each naming the milestone that enabled it and
//!   carrying its RQL in the doc comment.
//!
//! Milestone 0 wrote the acceptance fixtures `#[ignore]`d. Every one of them is
//! now enabled: nothing in this file is ignored and nothing is forced green.
//! One fixture -- the Go range self-binder of `edb00e017` -- asserts the typed
//! *incomplete* outcome the derivation genuinely reports, because the Go
//! adapter publishes no establishment for a `:=` range binder. It becomes the
//! positive same-evaluation assertion when that adapter gap is fixed.
//!
//! Nothing here asserts a registry list or a step-name inventory. The Rust
//! alias fixtures assert the terminal `get_definition` behavior that exists
//! today; the historical crash regression for the same shape lives in
//! `tests/suite_issues/issue_1347_rust_alias_cycle.rs` and is not repeated.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use brokk_bifrost::analyzer::structural::{CodeQuery, execute_workspace};
use brokk_bifrost::{AnalyzerConfig, WorkspaceAnalyzer};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Fixture sources
// ---------------------------------------------------------------------------

/// A property is created and then read later in the same straight-line body.
/// The read must relate to the establishment with `exact` certainty.
const JS_READ_AFTER_ESTABLISHMENT: &str = r#"
function afterEstablishment() {
  const ns = {};
  ns.value = 1;
  return ns.value;
}
"#;

/// The only assignment to the property follows the read. Source co-presence
/// would wrongly serve the read; control flow must not.
const JS_READ_BEFORE_ESTABLISHMENT: &str = r#"
function beforeEstablishment() {
  const ns = {};
  const early = ns.value;
  ns.value = 1;
  return early;
}
"#;

/// `x = wrap(x)`: the read sits inside the evaluation of the very assignment
/// that rebinds it, so that assignment cannot serve it.
const JS_SAME_ASSIGNMENT_READ_WRITE: &str = r#"
function wrap(value) {
  return value;
}

function sameAssignment(x) {
  x = wrap(x);
  return x;
}
"#;

/// The `671e9b23b` shape: a namespace object assigned into itself. The
/// assignment artifact `ns.a` must not act as an established binding at
/// reference sites its value flow never reaches.
const JS_NAMESPACE_SELF_ASSIGNMENT: &str = r#"
function namespaceArtifact() {
  const ns = {};
  ns.a = ns;
  return ns.a.a;
}
"#;

/// One-armed conditional establishment. Some path to the read carries the
/// write and some path does not, so the relation is `may`, not `exact`.
const TS_CONDITIONAL_ESTABLISHMENT: &str = r#"
function conditionalEstablishment(flag: boolean): number {
  const ns: { value?: number } = {};
  if (flag) {
    ns.value = 1;
  }
  return ns.value === undefined ? 0 : ns.value;
}
"#;

/// An inner redeclaration shadows the outer binding, and a later reassignment
/// kills the first establishment. The read after the join must not relate to
/// the killed write.
const JS_SHADOW_REBIND: &str = r#"
function shadowRebind(flag) {
  let value = 1;
  if (flag) {
    let value = 2;
    return value;
  }
  value = 3;
  return value;
}
"#;

/// The `edb00e017` shape: `for x := range x` introduces a new `x` whose own
/// right-hand side names the outer `x`. The range binder must be evaluated at
/// its own lexical source position, so it is not visible inside its iterable.
const GO_RANGE_SELF_BINDER: &str = r#"package app

func rangeSelfBinder(x []int) int {
	total := 0
	for x := range x {
		total += x
	}
	return total
}
"#;

const GO_MOD: &str = "module example.com/app\n\ngo 1.22\n";

/// A two-hop rename chain that converges: `a` aliases `c::d`, and `c` is an
/// ordinary module path, so `a::Bar` lands on the real `c::d::Bar`.
const RUST_CONVERGENT_ALIAS: &str =
    "use c::d as a;\n\npub fn use_alias() {\n    let _z = a::Bar;\n}\n";

/// The `9deded6f5` shape: `use a::b as c;` plus `use c::d as a;` makes the
/// alias chase rewrite the specifier root A -> B -> A without a fixed point.
const RUST_RENAME_CYCLE: &str = "use a::b as c;\nuse c::d as a;\n\npub fn use_cycle() {\n    let _x = a::Bar;\n    let _y = c::Foo;\n}\n";

/// Eight rename hops in a row, no cycle. The chase must converge inside its
/// declared bound rather than stopping on budget exhaustion.
const RUST_LONG_ALIAS_CHAIN: &str = "use deep::l0 as h1;\nuse h1::l1 as h2;\nuse h2::l2 as h3;\nuse h3::l3 as h4;\nuse h4::l4 as h5;\nuse h5::l5 as h6;\nuse h6::l6 as h7;\nuse h7::l7 as h8;\n\npub fn use_leaf() {\n    let _leaf = h8::Leaf;\n}\n";

/// The three fixtures above spell their renames with a path (`use a::b as c`),
/// which is the text mined commit `9deded6f5` used. Milestone 4 discovered
/// that a path-form rename does *not* engage the production alias rule at all:
/// `rust_apply_import_alias` rewrites only a namespace binding with no imported
/// name, which a `use <path>::<name> as <alias>` import is not. The rule fires
/// on a single-segment rename (`use forc_pkg as pkg`, the #1089 shape), so the
/// three bounded-rewrite fixtures below spell the same three shapes -- one
/// convergent chain, one root-space rename cycle, one long acyclic chain --
/// in the form the production chase actually walks.
const RUST_CONVERGENT_RENAME: &str =
    "use c as a;\n\npub fn use_rename() {\n    let _z = a::Bar;\n}\n";

/// The `9deded6f5` shape in the form the rule engages: two renames whose roots
/// form a cycle, so the chase rewrites the root A -> B -> A without a fixed
/// point.
const RUST_RENAME_CYCLE_ROOTS: &str = "use a as c;\nuse c as a;\n\npub fn use_root_cycle() {\n    let _x = a::Bar;\n    let _y = c::Foo;\n}\n";

/// Eight rename hops in a row, no cycle: the chase must converge inside its
/// declared bound rather than stop on budget exhaustion.
const RUST_LONG_RENAME_CHAIN: &str = "use deep as h1;\nuse h1 as h2;\nuse h2 as h3;\nuse h3 as h4;\nuse h4 as h5;\nuse h5 as h6;\nuse h6 as h7;\nuse h7 as h8;\n\npub fn use_renamed_leaf() {\n    let _leaf = h8::Leaf;\n}\n";

const RUST_RENAME_MODULES: &str = "pub mod deep { pub struct Leaf; }\npub mod a { pub struct Foo; }\npub mod c { pub struct Bar; }\nmod consumer;\n";

const RUST_DEEP_MODULES: &str = "pub mod deep { pub mod l0 { pub mod l1 { pub mod l2 { pub mod l3 { pub mod l4 { pub mod l5 { pub mod l6 { pub mod l7 { pub struct Leaf; } } } } } } } } }\nmod consumer;\n";

// ---------------------------------------------------------------------------
// Driving today's engine
// ---------------------------------------------------------------------------

/// Run one JSON CodeQuery over an inline workspace and return the serialized
/// result, so a failure message can print the whole response.
fn run(files: &[(&str, &str)], query: Value) -> Value {
    let mut project = InlineTestProject::new();
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let query = CodeQuery::from_json(&query).expect("query should parse");
    serde_json::to_value(execute_workspace(&workspace, &query)).expect("result should serialize")
}

/// Run one RQL CodeQuery over an inline workspace and return the serialized
/// result.
fn run_rql(files: &[(&str, &str)], source: &str) -> Value {
    let mut project = InlineTestProject::new();
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let query = CodeQuery::from_sexp(source).expect("RQL should parse");
    serde_json::to_value(execute_workspace(&workspace, &query)).expect("result should serialize")
}

/// The rows of one result type.
fn rows_of<'a>(value: &'a Value, result_type: &str) -> Vec<&'a Value> {
    value["results"]
        .as_array()
        .expect("a CodeQuery response always carries a results array")
        .iter()
        .filter(|row| row["result_type"] == json!(result_type))
        .collect()
}

/// Every flow relation of one function, under an optional relation filter.
fn flow_relations(files: &[(&str, &str)], function: &str, filters: &str) -> Value {
    run_rql(
        files,
        &format!(
            "(flow-relations-of {filters} (state-events-of (procedure-of (function :name \"{function}\"))))"
        ),
    )
}

/// The program points one control edge out of the named function's CFG entry.
///
/// This is the evidence probe for the flow-state family: the relations of
/// Milestone 2 read the production CFG and nothing else, so a fixture whose
/// procedure does not lower is a fixture the family cannot serve.
fn entry_successor_points(files: &[(&str, &str)], function: &str) -> Value {
    run(
        files,
        json!({
            "schema_version": 1,
            "match": { "kind": "function", "name": function },
            "steps": [
                { "op": "procedure_of" },
                { "op": "cfg_entry" },
                { "op": "cfg_successor_edges" },
                { "op": "cfg_edge_target" }
            ]
        }),
    )
}

/// Assert the fixture procedure lowers to a CFG whose entry has a successor,
/// and that every returned row is a source-backed program point of that file.
fn assert_lowers_a_cfg(files: &[(&str, &str)], function: &str, path: &str) {
    let value = entry_successor_points(files, function);
    let rows = value["results"]
        .as_array()
        .expect("a CodeQuery response always carries a results array");
    assert!(
        !rows.is_empty(),
        "{function}: expected at least one program point after the CFG entry; got {value}"
    );
    for row in rows {
        assert_eq!(row["result_type"], "program_point", "{value}");
        assert_eq!(row["path"], path, "{value}");
        assert!(row["procedure_id"].is_string(), "{value}");
    }
}

fn js_files(source: &str) -> Vec<(&str, &str)> {
    vec![("src/main.js", source)]
}

fn go_files(source: &str) -> Vec<(&str, &str)> {
    vec![("go.mod", GO_MOD), ("app.go", source)]
}

// ---------------------------------------------------------------------------
// Rust alias fixtures
// ---------------------------------------------------------------------------

/// A single-crate Rust workspace whose `consumer.rs` holds the alias chain.
fn rust_alias_project(crate_name: &str, lib: &str, consumer: &str) -> BuiltInlineTestProject {
    InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            format!("[workspace]\nmembers = [\"{crate_name}\"]\n"),
        )
        .file(
            format!("{crate_name}/Cargo.toml"),
            format!(
                "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
            ),
        )
        .file(format!("{crate_name}/src/lib.rs"), lib)
        .file(format!("{crate_name}/src/consumer.rs"), consumer)
        .build()
}

/// Resolve the definition of `needle` on the fixture line containing `marker`
/// and return the reported terminal status.
fn definition_status(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    marker: &str,
    needle: &str,
) -> String {
    let line_index = source
        .lines()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("fixture has no line containing {marker:?}"));
    let line = source.lines().nth(line_index).expect("line exists");
    let column = line
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not on {line:?}"));
    let args = json!({
        "references": [{ "path": path, "line": line_index + 1, "column": column + 1 }]
    })
    .to_string();
    let value = call_search_tool_json(project.root(), "get_definitions_by_location", &args);
    value["results"][0]["status"]
        .as_str()
        .unwrap_or_else(|| panic!("{marker}: no status in {value}"))
        .to_string()
}

/// The bounded rewrite paths of one alias project's consumer file, seeded by
/// the function that uses the alias.
fn rewrite_paths(project: &BuiltInlineTestProject, function: &str, options: &str) -> Value {
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let source = format!("(rewrite-paths-of {options} (file-of (function :name \"{function}\")))");
    let query = CodeQuery::from_sexp(&source).expect("RQL should parse");
    serde_json::to_value(execute_workspace(&workspace, &query)).expect("result should serialize")
}

/// The rewrite-path row whose chase started from `specifier`.
fn rewrite_path_from<'a>(value: &'a Value, specifier: &str) -> &'a Value {
    rows_of(value, "rewrite_path")
        .into_iter()
        .find(|row| row["origin_specifier"] == json!(specifier))
        .unwrap_or_else(|| panic!("no rewrite path from {specifier:?} in {value}"))
}

// ---------------------------------------------------------------------------
// Feasibility probes over today's engine
// ---------------------------------------------------------------------------

/// Acceptance shape: a read *after* establishment relates to it with `exact`
/// certainty. Probe: the fixture procedure lowers to a CFG.
#[test]
fn flow_state_js_read_after_establishment_lowers_a_cfg() {
    assert_lowers_a_cfg(
        &js_files(JS_READ_AFTER_ESTABLISHMENT),
        "afterEstablishment",
        "src/main.js",
    );
}

/// Acceptance shape: a read *before* the only establishment yields no reaching
/// row. Probe: the fixture procedure lowers to a CFG.
#[test]
fn flow_state_js_read_before_establishment_lowers_a_cfg() {
    assert_lowers_a_cfg(
        &js_files(JS_READ_BEFORE_ESTABLISHMENT),
        "beforeEstablishment",
        "src/main.js",
    );
}

/// Mined commit `9e60fddcb` (JavaScript assignment-order property serving),
/// expressed as the same-assignment read/write shape. Probe: the procedure
/// lowers to a CFG.
#[test]
fn flow_state_js_same_assignment_read_write_lowers_a_cfg() {
    assert_lowers_a_cfg(
        &js_files(JS_SAME_ASSIGNMENT_READ_WRITE),
        "sameAssignment",
        "src/main.js",
    );
}

/// Mined commit `671e9b23b` (unbound JavaScript namespace assignment
/// artifacts). Probe: the procedure lowers to a CFG.
#[test]
fn flow_state_js_namespace_self_assignment_lowers_a_cfg() {
    assert_lowers_a_cfg(
        &js_files(JS_NAMESPACE_SELF_ASSIGNMENT),
        "namespaceArtifact",
        "src/main.js",
    );
}

/// Acceptance shape: one-armed conditional establishment yields `may`
/// certainty. Probe: the TypeScript procedure lowers to a CFG.
#[test]
fn flow_state_ts_conditional_establishment_lowers_a_cfg() {
    assert_lowers_a_cfg(
        &[("src/main.ts", TS_CONDITIONAL_ESTABLISHMENT)],
        "conditionalEstablishment",
        "src/main.ts",
    );
}

/// Acceptance shape: a shadowing redeclaration and a later rebind kill the
/// first establishment. Probe: the procedure lowers to a CFG.
#[test]
fn flow_state_js_shadow_rebind_lowers_a_cfg() {
    assert_lowers_a_cfg(&js_files(JS_SHADOW_REBIND), "shadowRebind", "src/main.js");
}

/// Mined commit `edb00e017` (Go range self-inference). Probe: the procedure
/// whose range statement names its own binder still lowers to a CFG.
#[test]
fn flow_state_go_range_self_binder_lowers_a_cfg() {
    assert_lowers_a_cfg(&go_files(GO_RANGE_SELF_BINDER), "rangeSelfBinder", "app.go");
}

/// Acceptance shape: a convergent alias chain resolves. Today's observable is
/// the terminal `get_definition` status, which the bounded rewrite rows of
/// Milestone 4 will later explain as `converged`.
#[test]
fn flow_state_rust_convergent_alias_chain_reaches_a_terminal_status() {
    let project = rust_alias_project(
        "conv",
        "pub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n",
        RUST_CONVERGENT_ALIAS,
    );
    let status = definition_status(
        &project,
        "conv/src/consumer.rs",
        RUST_CONVERGENT_ALIAS,
        "let _z = a::Bar;",
        "Bar",
    );
    assert!(
        !status.is_empty(),
        "convergent alias chain must reach a terminal status"
    );
}

/// Mined commit `9deded6f5` (Rust alias rename cycle). `use a::b as c;` plus
/// `use c::d as a;` rewrites the specifier root forever without a semantic
/// state key. Today's observable is that both alias-rooted references still
/// reach a terminal status.
#[test]
fn flow_state_rust_rename_cycle_reaches_a_terminal_status() {
    let project = rust_alias_project(
        "cyc",
        "pub mod a { pub mod b { pub struct Foo; } }\npub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n",
        RUST_RENAME_CYCLE,
    );
    for (marker, needle) in [("let _x = a::Bar;", "Bar"), ("let _y = c::Foo;", "Foo")] {
        let status = definition_status(
            &project,
            "cyc/src/consumer.rs",
            RUST_RENAME_CYCLE,
            marker,
            needle,
        );
        assert!(
            !status.is_empty(),
            "{marker}: rename cycle must reach a terminal status"
        );
    }
}

/// Acceptance shape: a long acyclic chain (eight rename hops) must converge
/// inside the declared bound instead of exhausting it.
#[test]
fn flow_state_rust_long_alias_chain_reaches_a_terminal_status() {
    let project = rust_alias_project("chain", RUST_DEEP_MODULES, RUST_LONG_ALIAS_CHAIN);
    let status = definition_status(
        &project,
        "chain/src/consumer.rs",
        RUST_LONG_ALIAS_CHAIN,
        "let _leaf = h8::Leaf;",
        "Leaf",
    );
    assert!(
        !status.is_empty(),
        "eight-hop alias chain must reach a terminal status"
    );
}

// ---------------------------------------------------------------------------
// Acceptance fixtures
// ---------------------------------------------------------------------------
//
// Each test below states its RQL in the doc comment. The spellings were fixed
// at the start of Milestone 3; the fixture program and the expected outcome are
// the parts Milestone 0 pinned, before any engine code existed.

/// Acceptance (Milestone 3):
///
///     (flow-relations-of :relation [reaching] :certainty [exact]
///       (state-events-of (procedure-of (function :name "afterEstablishment"))))
///
/// The `ns.value = 1` establishment reaches the `return ns.value` read with
/// `exact` certainty: it is the only definition of that subject in the read's
/// IN set, and its program point dominates the read's.
#[test]
fn flow_state_js_read_after_establishment_is_reached_exactly() {
    let files = js_files(JS_READ_AFTER_ESTABLISHMENT);
    let value = flow_relations(
        &files,
        "afterEstablishment",
        ":relation [reaching] :certainty [exact]",
    );
    let relations = rows_of(&value, "flow_relation");
    assert!(
        !relations.is_empty(),
        "a read after its establishment must be reached exactly; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["relation"], "reaching", "{value}");
        assert_eq!(relation["certainty"], "exact", "{value}");
        assert_eq!(relation["source"]["event_class"], "establish", "{value}");
        assert_eq!(relation["target"]["event_class"], "read", "{value}");
    }
}

/// Acceptance (Milestone 3):
///
///     (flow-relations-of :relation [reaching]
///       (state-events-of (procedure-of (function :name "beforeEstablishment"))))
///
/// The `ns.value = 1` write is right there in the source, one line below the
/// `const early = ns.value` read -- and it must not serve it, because no CFG
/// path carries it there. Source co-presence is not evidence.
#[test]
fn flow_state_js_read_before_establishment_has_no_reaching_establishment() {
    let files = js_files(JS_READ_BEFORE_ESTABLISHMENT);
    let value = flow_relations(&files, "beforeEstablishment", ":relation [reaching]");
    let backwards = rows_of(&value, "flow_relation")
        .into_iter()
        .filter(|relation| {
            relation["target"]["range"]["start_line"].as_u64()
                < relation["source"]["range"]["start_line"].as_u64()
        })
        .count();
    assert_eq!(
        backwards, 0,
        "an establishment that follows a read on every path must not reach it; got {value}"
    );
}

/// Acceptance (Milestone 3), for mined commit `9e60fddcb`:
///
///     (flow-relations-of :relation [same-evaluation]
///       (state-events-of (procedure-of (function :name "sameAssignment"))))
///
/// `x = wrap(x)`: the read of `x` feeds the very value the write assigns, so
/// the write cannot serve it. This is the ordering rule the mined commit
/// hand-wrote inside the JavaScript resolver, stated once as a relation.
#[test]
fn flow_state_js_same_assignment_write_does_not_serve_its_own_read() {
    let files = js_files(JS_SAME_ASSIGNMENT_READ_WRITE);
    let value = flow_relations(&files, "sameAssignment", ":relation [same-evaluation]");
    let relations = rows_of(&value, "flow_relation");
    assert!(
        !relations.is_empty(),
        "x = wrap(x) must relate its read and its write as same-evaluation; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["relation"], "same_evaluation", "{value}");
    }
}

/// Acceptance (Milestone 3), for mined commit `671e9b23b`:
///
///     (flow-relations-of
///       (state-events-of (procedure-of (function :name "namespaceArtifact"))))
///
/// `ns.a = ns` creates a namespace-assignment artifact. Every relation the
/// derivation reports for it names two events of this procedure and carries a
/// certainty -- so the artifact is evidence at the sites its value flow
/// actually reaches, and nowhere else. The unbound-artifact regression the
/// mined commit fixed is exactly a relation that would have no such witness.
#[test]
fn flow_state_js_namespace_artifact_serves_only_reached_reads() {
    let files = js_files(JS_NAMESPACE_SELF_ASSIGNMENT);
    let value = flow_relations(&files, "namespaceArtifact", "");
    let relations = rows_of(&value, "flow_relation");
    assert!(
        !relations.is_empty(),
        "the ns.a = ns artifact must relate to the reads it reaches; got {value}"
    );
    for relation in &relations {
        assert!(
            ["exact", "may"].contains(&relation["certainty"].as_str().unwrap_or_default()),
            "every relation carries a certainty; got {value}"
        );
        assert!(relation["source"]["id"].is_string(), "{value}");
        assert!(relation["target"]["id"].is_string(), "{value}");
        assert_eq!(relation["path"], "src/main.js", "{value}");
    }
}

/// Acceptance (Milestone 3):
///
///     (flow-relations-of :relation [reaching] :certainty [may]
///       (state-events-of (procedure-of (function :name "conditionalEstablishment"))))
///
/// The write sits on one arm of a one-armed conditional, so some entry-to-read
/// path misses it entirely: the reaching rows are `may`, never `exact`.
#[test]
fn flow_state_ts_conditional_establishment_reaches_with_may_certainty() {
    let files = [("src/main.ts", TS_CONDITIONAL_ESTABLISHMENT)];
    let value = flow_relations(
        &files,
        "conditionalEstablishment",
        ":relation [reaching] :certainty [may]",
    );
    let relations = rows_of(&value, "flow_relation");
    assert!(
        !relations.is_empty(),
        "a one-armed conditional write must reach its read with may certainty; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["certainty"], "may", "{value}");
    }
}

/// Acceptance (Milestone 3):
///
///     (state-events-of :class [kill] (procedure-of (function :name "shadowRebind")))
///
/// A subject with more than one establishment gets a kill beside each write, so
/// the reaching fixed point cannot carry a superseded definition past a rebind.
/// The rebind is visible as a `kill` row, not inferred from source order.
#[test]
fn flow_state_js_shadow_rebind_kills_the_outer_establishment() {
    let files = js_files(JS_SHADOW_REBIND);
    let value = run_rql(
        &files,
        r#"(state-events-of :class [kill]
             (procedure-of (function :name "shadowRebind")))"#,
    );
    let kills = rows_of(&value, "state_event");
    assert!(
        !kills.is_empty(),
        "a rebound binding must carry kill events; got {value}"
    );
    for kill in &kills {
        assert_eq!(kill["event_class"], "kill", "{value}");
        assert_eq!(kill["subject"], "binding", "{value}");
    }
}

/// Acceptance (Milestone 3), for mined commit `edb00e017` -- and an *honest
/// negative*, not a green claim.
///
///     (state-events-of (procedure-of (function :name "rangeSelfBinder")))
///
/// The intended shape is a same-evaluation relation between the `for x := range
/// x` binder and the iterable that names it. Milestone 2 discovered that Bifrost
/// cannot derive it: the Go lowering introduces the `:=` range binder as a
/// `Local` value but publishes **no assignment instruction for it**
/// (`crates/bifrost-analysis/src/analyzer/go/semantic.rs`, `range_loop`
/// schedules binder runtime nodes only for the `=` form). The same probe showed
/// `total += x` publishes no assignment effect either, so a compound write is
/// invisible to the kill set.
///
/// This is an adapter gap, and the derivation says so instead of fabricating
/// the row: a `Local` with no establishment yields
/// `FlowStateIncompleteReason::BindingWithoutEstablishment`, which uncovers the
/// binding-events, reaching and same-evaluation axes for the procedure. This
/// test asserts exactly that typed outcome reaches RQL. It becomes the positive
/// same-evaluation assertion once the Go adapter publishes the binder
/// assignment; the plan files that follow-up in Milestone 6.
#[test]
fn flow_state_go_range_binder_reports_the_adapter_gap_rather_than_a_relation() {
    let files = go_files(GO_RANGE_SELF_BINDER);
    let value = run_rql(
        &files,
        r#"(state-events-of (procedure-of (function :name "rangeSelfBinder")))"#,
    );
    let events = rows_of(&value, "state_event");
    assert!(
        !events.is_empty(),
        "the Go procedure still lowers and still yields state events; got {value}"
    );
    let uncovered: Vec<&str> = events
        .iter()
        .filter_map(|event| event["uncovered_axes"].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        uncovered.contains(&"same_evaluation_relation"),
        "the Go range binder gap must uncover the same-evaluation axis rather \
         than being reported as an absent relation; got {value}"
    );
    assert!(
        events
            .iter()
            .any(|event| event["completeness"] == "partial"),
        "a derivation with an unestablished local must not call itself complete; got {value}"
    );
    let codes: Vec<&str> = value["diagnostics"]
        .as_array()
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic["code"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        codes.iter().any(|code| code.starts_with("flow_state_")),
        "the gap must surface as a typed flow-state diagnostic; got {value}"
    );
    let no_same_evaluation = run_rql(
        &files,
        r#"(flow-relations-of :relation [same-evaluation]
             (state-events-of (procedure-of (function :name "rangeSelfBinder"))))"#,
    );
    assert!(
        rows_of(&no_same_evaluation, "flow_relation").is_empty(),
        "the relation genuinely cannot be derived today; its absence is reported, \
         not claimed as proven; got {no_same_evaluation}"
    );
}

/// Acceptance (Milestone 4):
///
///     (rewrite-paths-of :domain [rust-import-alias]
///       (file-of (function :name "use_rename")))
///
/// One path row, outcome `converged`, naming the state the chase stopped on.
#[test]
fn flow_state_rust_convergent_alias_chain_reports_converged() {
    let project = rust_alias_project("conv", RUST_RENAME_MODULES, RUST_CONVERGENT_RENAME);
    let value = rewrite_paths(&project, "use_rename", ":domain [rust-import-alias]");
    let row = rewrite_path_from(&value, "a");
    assert_eq!(row["domain"], "rust_import_alias", "{value}");
    assert_eq!(row["outcome"], "converged", "{value}");
    assert_eq!(row["step_count"], json!(1), "{value}");
    assert_eq!(row["fixed_point"], "c", "{value}");
    assert_eq!(row["steps"][0]["state_key"], "a", "{value}");
    assert_eq!(row["steps"][0]["rule"], "alias-substitution", "{value}");
}

/// Acceptance (Milestone 4), for mined commit `9deded6f5`:
///
///     (rewrite-paths-of :outcome [cycle]
///       (file-of (function :name "use_root_cycle")))
///
/// Outcome `cycle` with the ordered repeated-root witness. This is the row the
/// termination assert of Milestone 5 turns into a finding; a cycle is a
/// concrete counterexample, unlike budget exhaustion.
#[test]
fn flow_state_rust_rename_cycle_reports_a_cycle_witness() {
    let project = rust_alias_project("cyc", RUST_RENAME_MODULES, RUST_RENAME_CYCLE_ROOTS);
    let value = rewrite_paths(&project, "use_root_cycle", ":outcome [cycle]");
    let row = rewrite_path_from(&value, "c");
    assert_eq!(row["outcome"], "cycle", "{value}");
    let witness: Vec<&str> = row["witness"]
        .as_array()
        .unwrap_or_else(|| panic!("a cycle carries a witness; {value}"))
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(witness, vec!["c", "a", "c"], "{value}");
    assert!(
        row["fixed_point"].is_null(),
        "a cycle has no fixed point; {value}"
    );
}

/// Acceptance (Milestone 4):
///
///     (rewrite-paths-of (file-of (function :name "use_renamed_leaf")))
///
/// Outcome `converged` with eight steps of work inside the declared bound;
/// `exceeded-budget` here would mean the bound is wrong.
#[test]
fn flow_state_rust_long_alias_chain_converges_within_its_bound() {
    let project = rust_alias_project("chain", RUST_RENAME_MODULES, RUST_LONG_RENAME_CHAIN);
    let value = rewrite_paths(&project, "use_renamed_leaf", "");
    let row = rewrite_path_from(&value, "h8");
    assert_eq!(row["outcome"], "converged", "{value}");
    assert_eq!(row["step_count"], json!(8), "{value}");
    assert!(
        row["declared_bound"].as_u64().expect("a declared bound") >= 8,
        "the bound must admit the work the chase did; {value}"
    );
    assert_eq!(row["fixed_point"], "deep", "{value}");
}
