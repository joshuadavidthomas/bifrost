//! Cross-language conformance for the #1478 Milestone 1 call-shape rows.
//!
//! The invariants under test are the milestone's honesty rules. Every exact
//! call site yields exactly one outcome row; group and argument rows are
//! ordered, stably identified, and foreign-keyed to that site; a call kind is
//! only ever the one the language's own grammar names; and a call shape the
//! analyzer cannot read yields an outcome row that says so, with no argument
//! rows at all, so an exact-cardinality assertion over it can never read as
//! clean.
//!
//! Each language is exercised through the public `query_code` surface, which
//! is what an RQL or RQLP author actually binds.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::structural::{CodeQuery, CodeQueryResult, execute_workspace};
use brokk_bifrost::{AnalyzerConfig, WorkspaceAnalyzer};
use serde_json::{Value, json};

fn run(files: &[(&str, &str)], query: Value) -> CodeQueryResult {
    let mut project = InlineTestProject::new();
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    let query = CodeQuery::from_json(&query).expect("query should parse");
    execute_workspace(&workspace, &query)
}

fn serialized(result: &CodeQueryResult) -> Value {
    serde_json::to_value(result).expect("query result should serialize")
}

fn rows(value: &Value) -> &Vec<Value> {
    value["results"].as_array().expect("results array")
}

/// Every call to `callee` in `path`, expanded by the given call-shape steps.
fn call_query(path: &str, callee: &str, steps: &[&str]) -> Value {
    json!({
        "where": [path],
        "match": { "kind": "call", "callee": { "name": callee } },
        "steps": steps.iter().map(|op| json!({ "op": op })).collect::<Vec<_>>(),
        "result_detail": "full"
    })
}

/// The sole row of a result, with the query echoed on failure.
fn sole<'a>(value: &'a Value, what: &str) -> &'a Value {
    let rows = rows(value);
    assert_eq!(rows.len(), 1, "expected one {what} row: {value}");
    &rows[0]
}

const JAVA: &str = r#"public class App {
  static class Widget {
    Widget(int size) {}
  }
  static int helper(int a, int b) { return a + b; }
  static void caller() {
    new Widget(2);
    helper(1, 2);
  }
}
"#;

/// Java spells object creation as its own grammar node, so `new Widget(2)` is
/// a constructor call while the plain invocation beside it is not. Both are
/// exact: their argument lists are written in the source.
#[test]
fn java_object_creation_is_a_constructor_call_shape() {
    let files = [("App.java", JAVA)];

    let created = serialized(&run(
        &files,
        call_query("App.java", "Widget", &["call_shape"]),
    ));
    let created = sole(&created, "call shape");
    assert_eq!(created["result_type"], "call_shape");
    assert_eq!(created["call_kind"], "constructor");
    assert_eq!(created["coverage"], "exact");
    assert_eq!(created["group_count"], 1);
    assert_eq!(created["id"], created["site_id"]);

    let called = serialized(&run(
        &files,
        call_query("App.java", "helper", &["call_shape"]),
    ));
    let called = sole(&called, "call shape");
    assert_eq!(called["call_kind"], "function");
    assert_ne!(
        called["site_id"], created["site_id"],
        "two sites, two identities"
    );

    let arguments = serialized(&run(
        &files,
        call_query(
            "App.java",
            "helper",
            &["call_shape", "call_argument_groups", "call_arguments"],
        ),
    ));
    let arguments = rows(&arguments);
    assert_eq!(arguments.len(), 2, "{arguments:?}");
    assert_eq!(arguments[0]["argument_index"], 0);
    assert_eq!(arguments[1]["argument_index"], 1);
    for argument in arguments {
        assert_eq!(argument["site_id"], called["site_id"]);
        assert_eq!(argument["spread"], false);
    }
}

/// An occurrence anywhere inside a call reaches the same call-shape row the
/// structural match reaches, and the two agree on the site's AST identity.
/// That identity is the join key an RQLP assertion binds occurrences and call
/// shapes together with.
#[test]
fn occurrence_rows_reach_the_same_call_shape_by_ast_identity() {
    let files = [("App.java", JAVA)];

    let from_match = serialized(&run(
        &files,
        call_query("App.java", "helper", &["call_shape"]),
    ));
    let from_match = sole(&from_match, "call shape");

    let from_occurrences = serialized(&run(
        &files,
        call_query("App.java", "helper", &["occurrences_in", "call_shape"]),
    ));
    let from_occurrences = rows(&from_occurrences);
    assert!(
        !from_occurrences.is_empty(),
        "the call's own tokens must reach its shape: {from_occurrences:?}"
    );
    for row in from_occurrences {
        assert_eq!(row["result_type"], "call_shape");
        assert_eq!(row["site_ast_id"], from_match["site_ast_id"]);
        assert_eq!(row["site_id"], from_match["site_id"]);
    }
}

const SCALA: &str = r#"object App {
  def curried(a: Int)(b: Int): Int = a + b
  def caller: Int = curried(1)(2)
  def infix(a: Int, b: Int): Int = a max b
}
"#;

/// A Scala curried application is one call site whose two argument lists are
/// two ordered `ordinary` groups. Reporting it as two sites, or as one site
/// with one list, would both misstate the call the compiler sees.
#[test]
fn scala_curried_application_is_one_site_with_ordered_groups() {
    let files = [("App.scala", SCALA)];

    let shape = serialized(&run(
        &files,
        call_query("App.scala", "curried", &["call_shape"]),
    ));
    let shape = sole(&shape, "call shape");
    assert_eq!(shape["call_kind"], "function");
    assert_eq!(shape["coverage"], "exact");
    assert_eq!(shape["group_count"], 2, "{shape}");

    let groups = serialized(&run(
        &files,
        call_query(
            "App.scala",
            "curried",
            &["call_shape", "call_argument_groups"],
        ),
    ));
    let groups = rows(&groups);
    assert_eq!(groups.len(), 2, "{groups:?}");
    for (index, group) in groups.iter().enumerate() {
        assert_eq!(group["group_index"], index, "groups stay in source order");
        assert_eq!(group["kind"], "ordinary");
        assert_eq!(group["argument_count"], 1);
        assert_eq!(group["site_id"], shape["site_id"]);
    }
    assert_ne!(groups[0]["id"], groups[1]["id"]);

    let arguments = serialized(&run(
        &files,
        call_query(
            "App.scala",
            "curried",
            &["call_shape", "call_argument_groups", "call_arguments"],
        ),
    ));
    let arguments = rows(&arguments);
    assert_eq!(arguments.len(), 2, "{arguments:?}");
    assert_eq!(arguments[0]["group_id"], groups[0]["id"]);
    assert_eq!(arguments[1]["group_id"], groups[1]["id"]);
}

/// A named infix application is an `infix` call site: the receiver and the
/// operand are still the call's own structure, and the kind is the one the
/// Scala grammar names.
#[test]
fn scala_infix_application_is_classified_as_infix() {
    let shape = serialized(&run(
        &[("App.scala", SCALA)],
        call_query("App.scala", "max", &["call_shape"]),
    ));
    let shape = sole(&shape, "call shape");
    assert_eq!(shape["call_kind"], "infix");
    assert_eq!(shape["coverage"], "exact");
}

const CSHARP: &str = r#"namespace Demo {
  class Widget {
    public Widget(int size) {}
  }
  class App {
    static int Helper(int a, int b) { return a + b; }
    static void Caller() {
      new Widget(2);
      Helper(1, 2);
    }
  }
}
"#;

/// C# object creation is a constructor call shape, and the ordinary
/// invocation beside it keeps its own identity and its own ordered arguments.
#[test]
fn csharp_object_creation_is_a_constructor_call_shape() {
    let files = [("App.cs", CSHARP)];

    let created = serialized(&run(
        &files,
        call_query("App.cs", "Widget", &["call_shape"]),
    ));
    let created = sole(&created, "call shape");
    assert_eq!(created["call_kind"], "constructor");
    assert_eq!(created["coverage"], "exact");
    assert_eq!(created["group_count"], 1);

    let groups = serialized(&run(
        &files,
        call_query("App.cs", "Helper", &["call_shape", "call_argument_groups"]),
    ));
    let group = sole(&groups, "argument group");
    assert_eq!(group["group_index"], 0);
    assert_eq!(group["kind"], "ordinary");
    assert_eq!(group["argument_count"], 2);
}

const CPP: &str = r#"#define CALL_TWICE(a) helper(a, 1)

int helper(int a, int b) { return a + b; }

int caller() {
  CALL_TWICE(2);
  return helper(1, 2);
}
"#;

/// A C++ call whose callee names a function-like macro of the same
/// translation unit still yields its mandatory outcome row, and that row says
/// the shape is macro-derived and carries no argument rows at all. The
/// ordinary call in the same file stays exact, so the unknown answer is a
/// statement about this site rather than about the language.
#[test]
fn cpp_macro_derived_call_reports_unknown_coverage_and_no_arguments() {
    let files = [("main.cpp", CPP)];

    let macro_shape = serialized(&run(
        &files,
        call_query("main.cpp", "CALL_TWICE", &["call_shape"]),
    ));
    let macro_shape = sole(&macro_shape, "call shape");
    assert_eq!(macro_shape["coverage"], "unknown_macro_derived");
    assert_eq!(
        macro_shape["group_count"], 0,
        "an unreadable shape fabricates no group"
    );

    let macro_groups = serialized(&run(
        &files,
        call_query(
            "main.cpp",
            "CALL_TWICE",
            &["call_shape", "call_argument_groups"],
        ),
    ));
    assert!(
        rows(&macro_groups).is_empty(),
        "an unreadable shape fabricates no argument rows: {macro_groups}"
    );

    let ordinary = serialized(&run(
        &files,
        call_query("main.cpp", "helper", &["call_shape"]),
    ));
    let ordinary = rows(&ordinary);
    assert_eq!(ordinary.len(), 1, "{ordinary:?}");
    assert_eq!(ordinary[0]["coverage"], "exact");
    assert_eq!(ordinary[0]["group_count"], 1);
}

/// Re-running the same query returns byte-identical rows: row identities are
/// content-scoped, not run-scoped, so an assertion written against them holds
/// across runs.
#[test]
fn call_shape_rows_are_deterministic_across_runs() {
    let files = [("App.scala", SCALA)];
    let query = call_query(
        "App.scala",
        "curried",
        &["call_shape", "call_argument_groups", "call_arguments"],
    );
    let first = serialized(&run(&files, query.clone()));
    let second = serialized(&run(&files, query));
    assert_eq!(rows(&first), rows(&second));
}
