//! End-to-end coverage of the bounded rewrite-path query surface (#1480,
//! Milestone 4).
//!
//! Every assertion is about behavior visible on the wire: the rows a query
//! returns, the ordered steps and outcome payloads they carry, and what an
//! inapplicable file answers.
//!
//! The load-bearing property is that the rows come from the *production* alias
//! chase in `resolve_module_package`, instrumented in place. Nothing here walks
//! the import binder a second time, so a row that says `cycle` says the
//! production resolver met that cycle.

use crate::common::InlineTestProject;
use brokk_bifrost::Language;
use brokk_bifrost::analyzer::structural::{CodeQuery, CodeQueryResult, execute_workspace};
use brokk_bifrost::{AnalyzerConfig, WorkspaceAnalyzer};
use serde_json::{Value, json};

/// A two-hop rename chain that converges: `a` aliases `c::d`, and `c` is an
/// ordinary module path.
const CONVERGENT: &str = "use c as a;\n\npub fn use_alias() {\n    let _z = a::Bar;\n}\n";

/// The `9deded6f5` shape: `use a::b as c;` plus `use c::d as a;` rewrites the
/// specifier root A -> B -> A forever without a semantic state key.
const RENAME_CYCLE: &str = "use a as c;\nuse c as a;\n\npub fn use_cycle() {\n    let _x = a::Bar;\n    let _y = c::Foo;\n}\n";

/// Eight rename hops in a row, no cycle.
const LONG_CHAIN: &str = "use deep as h1;\nuse h1 as h2;\nuse h2 as h3;\nuse h3 as h4;\nuse h4 as h5;\nuse h5 as h6;\nuse h6 as h7;\nuse h7 as h8;\n\npub fn use_leaf() {\n    let _leaf = h8::Leaf;\n}\n";

const DEEP_MODULES: &str = "pub mod deep { pub struct Leaf; }\nmod consumer;\n";

/// A Rust file with no rename at all: an ordinary `use` binds its own last
/// segment, which the alias rule must not rewrite.
const NO_RENAME: &str = "use c::d;\n\npub fn plain() {\n    let _z = d::Bar;\n}\n";

/// A single-crate Rust workspace whose `consumer.rs` holds the alias chain.
fn rust_workspace(
    crate_name: &str,
    lib: &str,
    consumer: &str,
) -> (WorkspaceAnalyzer, crate::common::BuiltInlineTestProject) {
    let project = InlineTestProject::with_language(Language::Rust)
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
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    (workspace, project)
}

fn serialize(result: &CodeQueryResult) -> Value {
    serde_json::to_value(result).expect("query result should serialize")
}

fn rows_of<'a>(value: &'a Value, result_type: &str) -> Vec<&'a Value> {
    value["results"]
        .as_array()
        .expect("results array")
        .iter()
        .filter(|row| row["result_type"] == json!(result_type))
        .collect()
}

/// Every rewrite path of the consumer file of one alias workspace, under an
/// optional RQL option string.
fn consumer_paths(
    crate_name: &str,
    lib: &str,
    consumer: &str,
    function: &str,
    options: &str,
) -> Value {
    let (workspace, _project) = rust_workspace(crate_name, lib, consumer);
    let source = format!("(rewrite-paths-of {options} (file-of (function :name \"{function}\")))");
    let query = CodeQuery::from_sexp(&source).expect("RQL should parse");
    serialize(&execute_workspace(&workspace, &query))
}

/// The one row whose origin specifier is `specifier`.
fn row_for<'a>(value: &'a Value, specifier: &str) -> &'a Value {
    rows_of(value, "rewrite_path")
        .into_iter()
        .find(|row| row["origin_specifier"] == json!(specifier))
        .unwrap_or_else(|| panic!("no rewrite path for {specifier:?} in {value}"))
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// A convergent rename chain reports `converged` and names the fixed point the
/// chase actually stopped on, with its one substitution step in order.
#[test]
fn a_convergent_alias_chain_reports_converged_with_its_fixed_point() {
    let value = consumer_paths(
        "conv",
        "pub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n",
        CONVERGENT,
        "use_alias",
        "",
    );
    let row = row_for(&value, "a");
    assert_eq!(row["domain"], "rust_import_alias", "{value}");
    assert_eq!(row["outcome"], "converged", "{value}");
    assert_eq!(row["path"], "conv/src/consumer.rs", "{value}");
    assert_eq!(row["completeness"], "complete", "{value}");
    assert_eq!(row["step_count"], json!(1), "{value}");
    let steps = row["steps"].as_array().expect("steps array");
    assert_eq!(steps[0]["state_key"], "a", "{value}");
    assert_eq!(steps[0]["input"], "a", "{value}");
    assert_eq!(steps[0]["output"], "c", "{value}");
    assert_eq!(steps[0]["rule"], "alias-substitution", "{value}");
    assert_eq!(
        row["fixed_point"], "c",
        "a converged chase names the state it stopped on; {value}"
    );
    assert!(
        row["witness"].is_null(),
        "convergence carries no cycle witness; {value}"
    );
    assert!(
        row["declared_bound"].as_u64().expect("a declared bound") >= 1,
        "{value}"
    );
    assert!(row["generation"].is_number(), "{value}");
}

/// The mined `9deded6f5` shape: both renames form one cycle in root space, and
/// the row carries the ordered witness whose last state repeats its first.
#[test]
fn the_rename_cycle_reports_an_ordered_cycle_witness() {
    let value = consumer_paths(
        "cyc",
        "pub mod a { pub mod b { pub struct Foo; } }\npub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n",
        RENAME_CYCLE,
        "use_cycle",
        "",
    );
    for (origin, expected) in [("c", ["c", "a", "c"]), ("a", ["a", "c", "a"])] {
        let row = row_for(&value, origin);
        assert_eq!(row["outcome"], "cycle", "{origin}: {value}");
        let witness: Vec<&str> = row["witness"]
            .as_array()
            .unwrap_or_else(|| panic!("{origin}: a cycle carries a witness; {value}"))
            .iter()
            .map(|state| state.as_str().expect("witness states are strings"))
            .collect();
        assert_eq!(witness, expected.to_vec(), "{origin}: {value}");
        assert_eq!(
            witness.first(),
            witness.last(),
            "{origin}: the last state closes the cycle it repeats; {value}"
        );
        assert!(
            row["fixed_point"].is_null(),
            "{origin}: a cycle has no fixed point; {value}"
        );
        // The steps are the derivation the witness summarises: each one names
        // the root it rewrote and the specifier it produced.
        let steps = row["steps"].as_array().expect("steps array");
        assert_eq!(steps.len(), 2, "{origin}: {value}");
        assert_eq!(steps[0]["state_key"], witness[0], "{origin}: {value}");
        assert_eq!(steps[1]["state_key"], witness[1], "{origin}: {value}");
    }
}

/// Eight hops converge strictly inside the bound the domain declares for
/// itself. `exceeded_budget` here would mean the bound is wrong.
#[test]
fn a_long_alias_chain_converges_within_its_declared_bound() {
    let value = consumer_paths("chain", DEEP_MODULES, LONG_CHAIN, "use_leaf", "");
    let row = row_for(&value, "h8");
    assert_eq!(row["outcome"], "converged", "{value}");
    let step_count = row["step_count"].as_u64().expect("a step count");
    assert_eq!(step_count, 8, "eight renames means eight hops; {value}");
    let declared_bound = row["declared_bound"].as_u64().expect("a declared bound");
    assert!(
        declared_bound >= step_count,
        "the bound must admit the work the chase did; {value}"
    );
    let steps = row["steps"].as_array().expect("steps array");
    // The steps are in chase order: each step's input is the previous step's
    // output, and each state key is that input's root.
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            assert_eq!(step["input"], steps[index - 1]["output"], "{value}");
        }
        let input = step["input"].as_str().expect("an input specifier");
        let root = input.split("::").next().expect("a root segment");
        assert_eq!(step["state_key"], root, "{value}");
    }
    assert_eq!(steps[0]["state_key"], "h8", "{value}");
    assert_eq!(
        row["fixed_point"],
        steps[steps.len() - 1]["output"],
        "the fixed point is the last rewrite's output; {value}"
    );
}

/// An ordinary `use c::d;` binds its own last segment, which is not a rename.
/// It engages no rewrite, so it is not a path through the domain -- and the
/// file's answer is complete, not partial.
#[test]
fn a_file_without_renames_yields_no_paths_and_a_complete_answer() {
    let value = consumer_paths(
        "plain",
        "pub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n",
        NO_RENAME,
        "plain",
        "",
    );
    assert!(
        rows_of(&value, "rewrite_path").is_empty(),
        "an ordinary import engages no rewrite; {value}"
    );
    assert!(
        !value["diagnostics"]
            .as_array()
            .map(
                |diagnostics| diagnostics.iter().any(|diagnostic| diagnostic["code"]
                    .as_str()
                    .is_some_and(|code| code.starts_with("rewrite_")))
            )
            .unwrap_or(false),
        "no rewrite diagnostic: nothing failed here; {value}"
    );
}

// ---------------------------------------------------------------------------
// Filters, inapplicable files, and frontend parity
// ---------------------------------------------------------------------------

/// `:outcome` narrows the rows to the named terminal outcome and nothing else.
#[test]
fn the_outcome_filter_keeps_only_the_named_outcome() {
    let lib = "pub mod a { pub mod b { pub struct Foo; } }\npub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n";
    let cycles = consumer_paths("cyc", lib, RENAME_CYCLE, "use_cycle", ":outcome [cycle]");
    let rows = rows_of(&cycles, "rewrite_path");
    assert!(!rows.is_empty(), "{cycles}");
    for row in &rows {
        assert_eq!(row["outcome"], "cycle", "{cycles}");
    }
    let converged = consumer_paths(
        "cyc",
        lib,
        RENAME_CYCLE,
        "use_cycle",
        ":outcome [converged]",
    );
    assert!(
        rows_of(&converged, "rewrite_path").is_empty(),
        "the cyclic file has no converged chase; {converged}"
    );
    // The domain filter is the other axis, and it names the same rows.
    let by_domain = consumer_paths(
        "cyc",
        lib,
        RENAME_CYCLE,
        "use_cycle",
        ":domain [rust-import-alias]",
    );
    assert_eq!(
        rows_of(&by_domain, "rewrite_path").len(),
        rows.len(),
        "{by_domain}"
    );
}

/// A file of a language no declared domain applies to answers empty *and*
/// complete: there is nothing this derivation failed to compute.
#[test]
fn a_non_rust_file_yields_an_empty_complete_answer() {
    let project = InlineTestProject::new()
        .file(
            "src/main.js",
            "function handler(seed) {\n  return seed;\n}\n",
        )
        .build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let query = CodeQuery::from_sexp("(rewrite-paths-of (file-of (function :name \"handler\")))")
        .expect("RQL should parse");
    let value = serialize(&execute_workspace(&workspace, &query));
    assert!(rows_of(&value, "rewrite_path").is_empty(), "{value}");
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
        !codes.iter().any(|code| code.starts_with("rewrite_")),
        "an inapplicable domain is not an incomplete derivation; got {codes:?} in {value}"
    );
}

/// The JSON frontend lowers to the same plan as the RQL frontend, so one query
/// written two ways returns the same rows.
#[test]
fn the_json_frontend_returns_the_same_rows_as_rql() {
    let lib = "pub mod c { pub mod d { pub struct Bar; } }\nmod consumer;\n";
    let (workspace, _project) = rust_workspace("conv", lib, CONVERGENT);
    let rql = CodeQuery::from_sexp(
        "(rewrite-paths-of :outcome [converged] (file-of (function :name \"use_alias\")))",
    )
    .expect("RQL should parse");
    let json = CodeQuery::from_json(&json!({
        "schema_version": 1,
        "match": { "kind": "function", "name": "use_alias" },
        "steps": [
            { "op": "file_of" },
            { "op": "rewrite_paths_of", "rewrite_outcome": ["converged"] }
        ]
    }))
    .expect("JSON query should parse");
    let from_rql = serialize(&execute_workspace(&workspace, &rql));
    let from_json = serialize(&execute_workspace(&workspace, &json));
    let rql_rows = rows_of(&from_rql, "rewrite_path");
    assert!(!rql_rows.is_empty(), "{from_rql}");
    assert_eq!(
        rql_rows,
        rows_of(&from_json, "rewrite_path"),
        "the two frontends must lower to one plan;\nrql: {from_rql}\njson: {from_json}"
    );
}
