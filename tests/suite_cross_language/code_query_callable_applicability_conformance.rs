//! Cross-language conformance for the #1478 fixture families (Milestone 5).
//!
//! `code_query_callable_applicability.rs` beside this file pins the Milestone 3
//! acceptance rules on one overload set. This file is the wider sweep: one
//! family per call shape the 31 motivating bug-fix commits clustered into, each
//! with a realistic near miss in the same fixture, and each written only as far
//! as the analyzer's recorded capability actually reaches.
//!
//! Two boundaries decide where each family is asserted, and both are properties
//! of the workspace rather than of this file.
//!
//! - The `callable-applicability` and `overload-selection` steps start from an
//!   occurrence, so they reach only a language that classifies occurrence roles.
//!   Today that is Java (#1473/#1724 owns the rest). A family whose language
//!   cannot be reached that way is asserted on the row domain that *is*
//!   reachable -- the Milestone 1 call-shape rows, which come from a structural
//!   match rather than an occurrence -- and its applicability counterpart lives
//!   with its language's production trace.
//! - A language whose resolver performs no argument-shape check at all is
//!   unsupported on the axis by construction, and says so. That is an answer,
//!   not a gap, and the Rust family below asserts it rather than skipping.

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

fn serialized(files: &[(&str, &str)], query: Value) -> Value {
    serde_json::to_value(run(files, query)).expect("query result should serialize")
}

fn rows(value: &Value) -> &Vec<Value> {
    value["results"].as_array().expect("results array")
}

/// Every occurrence of `role` in `path`, expanded by one step.
fn occurrence_query(path: &str, role: &str, op: &str) -> Value {
    json!({
        "where": [path],
        "occurrences": { "role": [role] },
        "steps": [{ "op": op }],
        "result_detail": "full"
    })
}

/// Every occurrence in `path` whatever its role, expanded by one step. Used
/// where the family's site is reached by a role the fixture should not have to
/// name -- a static call and an object creation are classified differently, and
/// the row set is what the family is about.
fn any_occurrence_query(path: &str, op: &str) -> Value {
    json!({
        "where": [path],
        "occurrences": {},
        "steps": [{ "op": op }],
        "result_detail": "full"
    })
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

/// The rows of one call site, identified by the line its reference sits on.
/// Every family below has more than one call in its fixture on purpose -- the
/// near miss is in the same file -- so a family assertion has to name its site.
fn rows_on_line(value: &Value, line: u64) -> Vec<&Value> {
    rows(value)
        .iter()
        .filter(|row| row["range"]["start_line"] == json!(line))
        .collect()
}

/// The rows of the one site that starts at `line`:`column`. A fixture holds
/// declarations as well as calls, and a declaration name sits on the same line
/// as the call in its body, so a family that names a site by line alone would
/// silently include the wrong rows.
fn rows_at(value: &Value, line: u64, column: u64) -> Vec<&Value> {
    rows_on_line(value, line)
        .into_iter()
        .filter(|row| row["range"]["start_column"] == json!(column))
        .collect()
}

fn sole<'a>(value: &'a Value, what: &str) -> &'a Value {
    let rows = rows(value);
    assert_eq!(rows.len(), 1, "expected one {what} row: {value}");
    &rows[0]
}

const JAVA_FACTORIES: &str = r#"package app;

public class Widget {
    static Widget of(int width, String label) { return null; }
    static Widget of(String label, int width) { return null; }
    static Widget of(int width, String label, int height) { return null; }

    static Widget make() { return of(1, "a"); }
}
"#;

/// Two factory overloads accept the same argument count and differ only in the
/// order of their declared types. Java's own applicability check measures the
/// argument *count*, so it accepts both -- and the honest report of that is an
/// ambiguous site with both winners retained, never one of them broken by
/// candidate order. The three-parameter sibling in the same file is the near
/// miss: it shares the name and is refused for the exact end of the range the
/// call missed.
///
/// This is also the ambiguous-factory family: two equal winners stay equal.
#[test]
fn java_same_arity_factories_stay_ambiguous_beside_a_refused_sibling() {
    let files = [("app/Widget.java", JAVA_FACTORIES)];
    let applicability = serialized(
        &files,
        any_occurrence_query("app/Widget.java", "callable_applicability"),
    );
    let site = rows_at(&applicability, 8, 35);
    assert_eq!(
        site.len(),
        3,
        "every overload the resolver considered keeps a row: {applicability}"
    );

    let applicable = site
        .iter()
        .filter(|row| row["verdict"] == "applicable")
        .collect::<Vec<_>>();
    assert_eq!(
        applicable.len(),
        2,
        "both same-arity factories stay winners: {applicability}"
    );
    let signatures = applicable
        .iter()
        .map(|row| row["candidate"]["unit"]["signature"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    assert!(
        signatures.contains(&"(int, String)") && signatures.contains(&"(String, int)"),
        "the two winners are the two orderings, not one of them twice: {applicability}"
    );

    let refused = site
        .iter()
        .filter(|row| row["verdict"] == "inapplicable")
        .collect::<Vec<_>>();
    assert_eq!(refused.len(), 1, "{applicability}");
    assert_eq!(
        refused[0]["reason"], "arity_below_required",
        "{applicability}"
    );

    let selection = serialized(
        &files,
        any_occurrence_query("app/Widget.java", "overload_selection"),
    );
    let summary = rows_at(&selection, 8, 35);
    assert_eq!(
        summary.len(),
        1,
        "one mandatory summary per site: {selection}"
    );
    assert_eq!(summary[0]["resolution"], "ambiguous", "{selection}");
    assert_eq!(summary[0]["applicable_count"], 2, "{selection}");
}

/// The discrimination Java's arity check cannot make is still queryable one row
/// domain over: the two same-arity factories separate into two signature rows
/// whose declared parameter types are written in opposite orders. An RQLP
/// author who needs to tell them apart joins there, which is the whole reason
/// the signature domain exists beside the applicability domain.
#[test]
fn the_signature_rows_separate_what_the_arity_check_could_not() {
    let files = [("app/Widget.java", JAVA_FACTORIES)];
    let parameters = serialized(
        &files,
        json!({
            "where": ["app/Widget.java"],
            "match": { "kind": "method", "name": "of" },
            "steps": [
                { "op": "enclosing_decl" },
                { "op": "callable_signature" },
                { "op": "signature_parameters" }
            ],
            "result_detail": "full"
        }),
    );
    let two_parameter_shapes = rows(&parameters)
        .iter()
        .filter(|row| row["parameter_index"] == json!(0))
        .map(|row| row["declared_type"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    assert!(
        two_parameter_shapes.contains(&"int".to_owned())
            && two_parameter_shapes.contains(&"String".to_owned()),
        "the overloads' first parameters differ in declared type: {parameters}"
    );
}

const JAVA_VARARGS: &str = r#"package app;

public class Widget {
    int pad(int width) { return width; }
    int pad(int width, int... extra) { return width; }

    int caller(Widget widget) {
        return widget.pad(1, 2, 3);
    }
}
"#;

/// A variadic declaration accepts more arguments than it names, and its
/// fixed-arity sibling in the same file does not. The repeated tail is why the
/// verdict is a *range* answer rather than a count comparison: the winner's
/// declared list is shorter than the call and still applicable, while the loser
/// is refused at the exact end of its range the call overshot.
#[test]
fn java_a_variadic_declaration_accepts_a_call_its_fixed_sibling_cannot() {
    let files = [("app/Widget.java", JAVA_VARARGS)];
    let applicability = serialized(
        &files,
        occurrence_query(
            "app/Widget.java",
            "member_position",
            "callable_applicability",
        ),
    );
    let considered = rows(&applicability);
    assert_eq!(considered.len(), 2, "{applicability}");

    let winner = considered
        .iter()
        .find(|row| row["verdict"] == "applicable")
        .unwrap_or_else(|| panic!("the variadic overload is applicable: {applicability}"));
    assert_eq!(winner["selected"], json!(true), "{applicability}");
    assert_eq!(
        winner["candidate"]["unit"]["signature"], "(int, int[])",
        "the winner is the variadic declaration: {applicability}"
    );

    let loser = considered
        .iter()
        .find(|row| row["verdict"] == "inapplicable")
        .unwrap_or_else(|| panic!("the fixed-arity sibling is refused: {applicability}"));
    assert_eq!(loser["reason"], "arity_above_total", "{applicability}");
    assert_eq!(
        loser["candidate"]["unit"]["signature"], "(int)",
        "{applicability}"
    );

    // The declaration side states the same range independently, which is what
    // makes the verdict checkable rather than merely reported.
    let signatures = serialized(
        &files,
        json!({
            "where": ["app/Widget.java"],
            "match": { "kind": "method", "name": "pad" },
            "steps": [{ "op": "enclosing_decl" }, { "op": "callable_signature" }],
            "result_detail": "full"
        }),
    );
    let repeated = rows(&signatures)
        .iter()
        .find(|row| row["repeated"] == json!(true))
        .unwrap_or_else(|| {
            panic!("the variadic declaration reports a repeated tail: {signatures}")
        });
    assert_eq!(
        repeated["required_arity"], 1,
        "a variadic tail is not required: {signatures}"
    );
}

const JAVA_CONSTRUCTORS: &str = r#"package app;

public class Widget {
    Widget() {}
    Widget(int width) {}

    static Widget make() { return new Widget(1); }
}
"#;

/// A constructor invocation is a call site with its own overload set, and it
/// reaches the applicability rows through the type name it writes. The near
/// miss is in the same fixture and is the reason a name alone is not evidence:
/// the `Widget` written as `make`'s return type is the same token spelled the
/// same way, and it carries no callable verdict because no argument list was
/// measured there.
#[test]
fn java_constructor_overloads_are_judged_at_the_object_creation_only() {
    let files = [("app/Widget.java", JAVA_CONSTRUCTORS)];
    let applicability = serialized(
        &files,
        any_occurrence_query("app/Widget.java", "callable_applicability"),
    );

    let creation = rows_at(&applicability, 7, 39);
    assert_eq!(
        creation.len(),
        2,
        "both constructors were considered at the `new` expression: {applicability}"
    );
    let winner = creation
        .iter()
        .find(|row| row["verdict"] == "applicable")
        .unwrap_or_else(|| panic!("the one-argument constructor is applicable: {applicability}"));
    assert_eq!(
        winner["candidate"]["unit"]["signature"], "(int)",
        "{applicability}"
    );
    let loser = creation
        .iter()
        .find(|row| row["verdict"] == "inapplicable")
        .unwrap_or_else(|| panic!("the no-argument constructor is refused: {applicability}"));
    assert_eq!(loser["reason"], "arity_above_total", "{applicability}");

    let return_type = rows_at(&applicability, 7, 12);
    assert_eq!(return_type.len(), 1, "{applicability}");
    assert_eq!(
        return_type[0]["verdict"], "unknown",
        "a type mention is not a call, so nothing about a call shape was decided: {applicability}"
    );

    // The call-shape row agrees about what the `new` expression is, which is the
    // join an RQLP author uses to scope an assertion to constructors.
    let shape = serialized(
        &files,
        call_query("app/Widget.java", "Widget", &["call_shape"]),
    );
    assert_eq!(sole(&shape, "call shape")["call_kind"], "constructor");
}

const JAVA_METHOD_VALUE: &str = r#"package app;

import java.util.function.Supplier;

public class Widget {
    int render() { return 0; }
    int render(int width) { return width; }

    Supplier<Integer> deferred(Widget widget) { return widget::render; }

    int immediate(Widget widget) { return widget.render(1); }
}
"#;

/// A method value names a callable without writing an argument list. The call
/// shape says so -- `method_value`, with no argument group at all -- and the
/// ordinary invocation of the same overload set in the same file is the near
/// miss that shows the difference is the site's, not the language's.
///
/// Stated boundary: a method value has no applicability verdict, because there
/// is no argument list for an applicability check to measure. Its overload set
/// is decided by the target functional interface, which no resolver models
/// today, so an eta-expanded reference is honestly outside this axis rather
/// than being given a count comparison against an argument list nobody wrote.
#[test]
fn java_a_method_value_states_its_kind_and_claims_no_applicability() {
    let files = [("app/Widget.java", JAVA_METHOD_VALUE)];
    let shapes = serialized(
        &files,
        call_query("app/Widget.java", "render", &["call_shape"]),
    );
    let deferred = rows_on_line(&shapes, 9);
    assert_eq!(deferred.len(), 1, "{shapes}");
    assert_eq!(deferred[0]["call_kind"], "method_value", "{shapes}");
    assert_eq!(deferred[0]["coverage"], "exact", "{shapes}");
    assert_eq!(
        deferred[0]["group_count"], 1,
        "the arena still models one list position for the reference: {shapes}"
    );

    let immediate = rows_on_line(&shapes, 11);
    assert_eq!(immediate.len(), 1, "{shapes}");
    assert_eq!(immediate[0]["call_kind"], "method", "{shapes}");
    assert_eq!(immediate[0]["group_count"], 1, "{shapes}");

    // Only the invocation carries applicability rows.
    let applicability = serialized(
        &files,
        occurrence_query(
            "app/Widget.java",
            "member_position",
            "callable_applicability",
        ),
    );
    assert!(
        !rows_on_line(&applicability, 11).is_empty(),
        "the invocation is judged: {applicability}"
    );
    assert!(
        rows_on_line(&applicability, 9).is_empty(),
        "the method value claims no verdict: {applicability}"
    );
}

const SCALA_LIST_SHAPES: &str = r#"object App {
  def curried(a: Int)(b: Int): Int = a + b
  def flat(a: Int, b: Int): Int = a + b
  def viaCurried: Int = curried(1)(2)
  def viaFlat: Int = flat(1, 2)
}
"#;

/// The same total argument count written in two different list shapes. This is
/// the family a flat arity integer cannot express at all: both calls pass two
/// arguments, and the difference that decides which declaration each one can
/// reach is how those arguments are grouped. The curried call is one site with
/// two ordered groups; its uncurried near miss is one site with one group of
/// two.
///
/// Scala classifies no occurrence roles yet (#1473/#1724), so the applicability
/// counterpart of this family is asserted on the production trace in
/// `code_query_member_dispatch_scala.rs`, where Scala's own call-shape relation
/// refuses a list that its declaration cannot consume.
#[test]
fn scala_curried_and_flat_calls_differ_in_list_shape_at_equal_argument_count() {
    let files = [("App.scala", SCALA_LIST_SHAPES)];

    let curried = serialized(
        &files,
        call_query(
            "App.scala",
            "curried",
            &["call_shape", "call_argument_groups"],
        ),
    );
    let curried = rows(&curried);
    assert_eq!(
        curried.len(),
        2,
        "a curried application is one site with two ordered groups: {curried:?}"
    );
    assert_eq!(curried[0]["group_index"], 0, "{curried:?}");
    assert_eq!(curried[0]["argument_count"], 1, "{curried:?}");
    assert_eq!(curried[1]["group_index"], 1, "{curried:?}");
    assert_eq!(curried[1]["argument_count"], 1, "{curried:?}");
    assert_eq!(
        curried[0]["site_id"], curried[1]["site_id"],
        "both lists belong to one call site: {curried:?}"
    );

    let flat = serialized(
        &files,
        call_query("App.scala", "flat", &["call_shape", "call_argument_groups"]),
    );
    let flat = rows(&flat);
    assert_eq!(flat.len(), 1, "{flat:?}");
    assert_eq!(flat[0]["group_index"], 0, "{flat:?}");
    assert_eq!(
        flat[0]["argument_count"], 2,
        "the near miss passes the same two arguments in one list: {flat:?}"
    );
}

const PYTHON_NAMED: &str = r#"def greet(name, greeting):
    return greeting + name


class Service:
    def run(self, width):
        return width


def caller(service):
    greet(name="ada", greeting="hi")
    greet("ada", "hi")
    return service.run(1)
"#;

/// Named arguments are a shape fact the analyzer records: the named list is its
/// own group, each argument carries the label it was written with, and the
/// positional call in the same file carries none. That is what an ordered
/// name-to-parameter parity assertion binds.
///
/// Stated boundary, asserted here rather than assumed: no resolver refuses a
/// candidate because of an argument *name* today. Python performs no
/// argument-shape filtering at all, so its sites report the axis as
/// unsupported, and `unknown_named_argument` is a vocabulary entry no language
/// produces yet.
#[test]
fn python_named_arguments_are_shape_facts_and_never_an_applicability_verdict() {
    let files = [("app.py", PYTHON_NAMED)];

    let groups = serialized(
        &files,
        call_query("app.py", "greet", &["call_shape", "call_argument_groups"]),
    );
    let named = rows_on_line(&groups, 11);
    assert_eq!(named.len(), 2, "{groups}");
    assert_eq!(named[0]["kind"], "ordinary", "{groups}");
    assert_eq!(named[0]["argument_count"], 0, "{groups}");
    assert_eq!(named[1]["kind"], "named", "{groups}");
    assert_eq!(named[1]["argument_count"], 2, "{groups}");

    let positional = rows_on_line(&groups, 12);
    assert_eq!(positional.len(), 1, "{groups}");
    assert_eq!(positional[0]["kind"], "ordinary", "{groups}");
    assert_eq!(positional[0]["argument_count"], 2, "{groups}");

    let arguments = serialized(
        &files,
        call_query(
            "app.py",
            "greet",
            &["call_shape", "call_argument_groups", "call_arguments"],
        ),
    );
    let names = rows_on_line(&arguments, 11)
        .iter()
        .map(|row| row["name"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["name".to_owned(), "greeting".to_owned()],
        "{arguments}"
    );
    assert!(
        rows_on_line(&arguments, 12)
            .iter()
            .all(|row| row["name"].is_null()),
        "a positional argument invents no label: {arguments}"
    );

    let selection = serialized(
        &files,
        occurrence_query("app.py", "member_position", "overload_selection"),
    );
    let summary = sole(&selection, "overload selection");
    assert_eq!(summary["supported"], json!(false), "{selection}");
    assert_eq!(summary["resolution"], "unknown_shape", "{selection}");
}

const RUST_INHERENT_AND_TRAIT: &str = r#"struct Widget;

trait Render {
    fn render(&self, width: i32) -> i32;
}

impl Widget {
    fn render(&self) -> i32 { 0 }
}

impl Render for Widget {
    fn render(&self, width: i32) -> i32 { width }
}

fn caller(widget: &Widget) -> i32 { widget.render() }
"#;

/// Inherent-versus-trait precedence with applicability, asserted as far as it
/// is real. Rust's resolver performs no argument-shape check anywhere, so the
/// two `render` declarations -- one inherent and zero-argument, one from the
/// trait and one-argument -- are both retained with an undecided verdict, and
/// the site says the axis is unsupported rather than reporting a winner it did
/// not compute.
///
/// The near miss is inside the fixture: a call that only the inherent
/// declaration can accept does not make the trait declaration inapplicable
/// here, because nothing measured the argument list. Reading this row set as
/// "the inherent method won on applicability" would be reading a conclusion out
/// of an absence, and the `supported` flag exists to stop exactly that.
#[test]
fn rust_reports_no_applicability_rather_than_a_verdict_it_never_computed() {
    let files = [("app.rs", RUST_INHERENT_AND_TRAIT)];
    let selection = serialized(
        &files,
        occurrence_query("app.rs", "member_position", "overload_selection"),
    );
    let summary = sole(&selection, "overload selection");
    assert_eq!(summary["supported"], json!(false), "{selection}");
    assert_eq!(summary["resolution"], "unknown_shape", "{selection}");
    assert_eq!(summary["considered_count"], 2, "{selection}");
    assert_eq!(
        summary["unknown_count"], 2,
        "an unmeasured candidate is undecided, never inapplicable: {selection}"
    );
    assert_eq!(summary["applicable_count"], 0, "{selection}");
    assert_eq!(summary["inapplicable_count"], 0, "{selection}");

    let applicability = serialized(
        &files,
        occurrence_query("app.rs", "member_position", "callable_applicability"),
    );
    assert_eq!(rows(&applicability).len(), 2, "{applicability}");
    for row in rows(&applicability) {
        assert_eq!(row["verdict"], "unknown", "{applicability}");
        assert!(
            row["reason"].is_null(),
            "an undecided candidate states no rejection reason: {applicability}"
        );
    }
}
