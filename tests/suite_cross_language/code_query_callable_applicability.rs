//! Conformance for the #1478 Milestone 3 applicability and overload-selection
//! rows, driven through the public `query_code` surface.
//!
//! The milestone's acceptance rules are what these tests assert, per claimed
//! language: the selected candidate's applicability row is `applicable`; every
//! overload the resolver's own filter refused appears as `inapplicable` with the
//! exact structured reason; a site with two equally applicable winners reports
//! `ambiguous` with both rows retained; and a language that has not been
//! factored reports `unknown_shape` rather than a confident answer nobody
//! computed.
//!
//! Java is the claimed language of this tranche. The near misses are realistic:
//! a same-name overload at a different arity, a call whose argument count no
//! overload accepts, and a language whose adapter does not declare the axis.

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

fn occurrence_query(path: &str, op: &str) -> Value {
    json!({
        "where": [path],
        "occurrences": { "role": ["member_position"] },
        "steps": [{ "op": op }],
        "result_detail": "full"
    })
}

/// The overload set the Java tranche exists for: three same-named methods
/// separated only by arity, and one call that reaches exactly one of them.
const OVERLOADS: &str = r#"package app;

public class Widget {
    int render() { return 0; }
    int render(int width) { return width; }
    int render(int width, int height) { return width * height; }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#;

/// The selected overload is applicable, and every overload the resolver's own
/// arity filter refused stays visible with the exact end of the range it
/// missed. Losing overloads are evidence here, not an absence.
#[test]
fn java_refused_overloads_stay_visible_with_their_structured_reason() {
    let files = [("app/Widget.java", OVERLOADS)];
    let applicability = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    let candidates = rows(&applicability);
    assert!(
        candidates.len() >= 3,
        "every considered overload keeps a row: {applicability}"
    );

    let selected = candidates
        .iter()
        .filter(|row| row["selected"] == json!(true))
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1, "{applicability}");
    assert_eq!(selected[0]["verdict"], "applicable", "{applicability}");
    assert_eq!(
        selected[0]["candidate"]["unit"]["signature"], "(int)",
        "the winner is the one-argument overload: {applicability}"
    );

    let refused = candidates
        .iter()
        .filter(|row| row["verdict"] == "inapplicable")
        .collect::<Vec<_>>();
    assert_eq!(
        refused.len(),
        2,
        "the zero- and two-argument overloads both lost: {applicability}"
    );
    let reasons = refused
        .iter()
        .map(|row| row["reason"].as_str().expect("a refused row states why"))
        .collect::<Vec<_>>();
    assert!(
        reasons.contains(&"arity_above_total"),
        "the zero-argument overload cannot take one argument: {applicability}"
    );
    assert!(
        reasons.contains(&"arity_below_required"),
        "the two-argument overload needs more than one argument: {applicability}"
    );
    for row in &refused {
        assert_eq!(row["selected"], json!(false), "{applicability}");
        assert_eq!(
            row["site_ast_id"], candidates[0]["site_ast_id"],
            "every row of one site shares its AST identity: {applicability}"
        );
    }
}

/// The mandatory summary is exactly one row per occurrence, and it agrees with
/// the applicability rows it was computed from.
#[test]
fn java_overload_selection_is_one_row_that_agrees_with_its_candidates() {
    let files = [("app/Widget.java", OVERLOADS)];
    let selection = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "overload_selection"),
    ));
    assert_eq!(rows(&selection).len(), 1, "{selection}");
    let summary = &rows(&selection)[0];
    assert_eq!(summary["result_type"], "overload_selection", "{selection}");
    assert_eq!(summary["resolution"], "resolved_unique", "{selection}");
    assert_eq!(summary["supported"], json!(true), "{selection}");
    assert_eq!(summary["applicable_count"], 1, "{selection}");
    assert_eq!(summary["inapplicable_count"], 2, "{selection}");
    assert_eq!(summary["unknown_count"], 0, "{selection}");

    let applicability = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    assert_eq!(
        summary["considered_count"].as_u64(),
        Some(rows(&applicability).len() as u64),
        "the summary counts exactly the rows the projection emits: {applicability}"
    );
    for row in rows(&applicability) {
        assert_eq!(
            row["site_ast_id"], summary["site_ast_id"],
            "{applicability}"
        );
    }
}

/// Two overloads that accept the same call are equally applicable winners, so
/// the site stays ambiguous with both rows retained. Candidate order is never
/// a tie breaker.
#[test]
fn java_two_equal_winners_stay_ambiguous_with_both_rows_retained() {
    let files = [(
        "app/Widget.java",
        r#"package app;

public class Widget {
    int render(int width) { return width; }
    int render(String label) { return 1; }

    int caller(Widget widget) {
        return widget.render(1);
    }
}
"#,
    )];
    let selection = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "overload_selection"),
    ));
    let summary = &rows(&selection)[0];
    assert_eq!(summary["resolution"], "ambiguous", "{selection}");
    assert_eq!(summary["applicable_count"], 2, "{selection}");

    let applicability = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    let applicable = rows(&applicability)
        .iter()
        .filter(|row| row["verdict"] == "applicable")
        .collect::<Vec<_>>();
    assert_eq!(
        applicable.len(),
        2,
        "both winners keep their row rather than one being broken by order: {applicability}"
    );
}

/// A call no overload accepts binds nothing. Every considered overload keeps a
/// rejected row with its typed reason, no row is `selected`, and the site's
/// summary says `unresolved` -- which is #1478's rule contract stated at a real
/// site: zero applicable candidates stay unresolved rather than being promoted
/// into a binding the resolver's own check refused.
#[test]
fn java_a_call_no_overload_accepts_binds_nothing_and_stays_unresolved() {
    let files = [(
        "app/Widget.java",
        r#"package app;

public class Widget {
    int render(int width) { return width; }

    int caller(Widget widget) {
        return widget.render(1, 2, 3);
    }
}
"#,
    )];
    let applicability = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    let candidates = rows(&applicability);
    assert!(!candidates.is_empty(), "{applicability}");
    assert!(
        candidates
            .iter()
            .any(|row| row["verdict"] == "inapplicable" && row["reason"] == "arity_above_total"),
        "the overload the call cannot reach keeps its typed reason: {applicability}"
    );
    assert!(
        candidates
            .iter()
            .all(|row| row["selected"] == json!(false) && row["verdict"] != "applicable"),
        "nothing the check refused is bound: {applicability}"
    );

    let selection = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "overload_selection"),
    ));
    let summary = &rows(&selection)[0];
    assert_eq!(summary["resolution"], "unresolved", "{selection}");
    assert_eq!(summary["applicable_count"], 0, "{selection}");
    assert_eq!(summary["inapplicable_count"], 1, "{selection}");
}

/// The same rule at the constructor seam, which is where the whole-set fallback
/// used to live (`e9033e203` removed it so a semantic model can supply the
/// constructor instead). A `new` expression whose argument count no declared
/// constructor accepts binds no constructor: the refused constructor keeps its
/// rejected row and the site reports `unresolved`.
#[test]
fn java_a_constructor_call_no_declaration_accepts_binds_nothing() {
    let files = [(
        "app/Widget.java",
        r#"package app;

public class Widget {
    Widget(int width) { }

    static Widget build() {
        return new Widget(1, 2, 3);
    }
}
"#,
    )];
    // A constructor call is reached through the type name it writes, not through
    // a `member_position` occurrence, so this query keeps every role.
    let applicability = serialized(&run(
        &files,
        json!({
            "where": ["app/Widget.java"],
            "occurrences": {},
            "steps": [{ "op": "callable_applicability" }],
            "result_detail": "full"
        }),
    ));
    let candidates = rows(&applicability);
    assert!(
        candidates.iter().any(|row| row["verdict"] == "inapplicable"
            && row["reason"] == "arity_above_total"
            && row["selected"] == json!(false)),
        "the refused constructor is a rejected row, not a binding: {applicability}"
    );
    assert!(
        candidates
            .iter()
            .all(|row| row["verdict"] != "inapplicable" || row["selected"] == json!(false)),
        "nothing the check refused is bound: {applicability}"
    );
    // With no constructor bound, the seam reports the declaring type instead.
    // That row is not a callable verdict and must not claim one.
    assert!(
        candidates
            .iter()
            .filter(|row| row["selected"] == json!(true))
            .all(|row| row["verdict"] == "unknown"),
        "the type the seam falls back to states no applicability: {applicability}"
    );
}

/// A language whose adapter does not declare the callable-applicability axis
/// still gets the mandatory summary, and that summary says the site is
/// undecidable rather than reporting a confident answer nobody computed.
#[test]
fn an_unfactored_language_reports_unknown_shape_rather_than_a_guess() {
    let files = [(
        "app.ts",
        r#"class Service { run(width: number) {} }
export function caller(service: Service) { service.run(1); }
"#,
    )];
    let selection = serialized(&run(
        &files,
        occurrence_query("app.ts", "overload_selection"),
    ));
    assert_eq!(rows(&selection).len(), 1, "{selection}");
    let summary = &rows(&selection)[0];
    assert_eq!(summary["resolution"], "unknown_shape", "{selection}");
    assert_eq!(summary["supported"], json!(false), "{selection}");
}

/// Both rows reach their workspace file like every other row domain, and both
/// runs of one query agree byte for byte.
#[test]
fn applicability_rows_reach_their_file_and_are_deterministic() {
    let files = [("app/Widget.java", OVERLOADS)];
    let query = json!({
        "where": ["app/Widget.java"],
        "occurrences": { "role": ["member_position"] },
        "steps": [{ "op": "callable_applicability" }, { "op": "file_of" }],
        "result_detail": "full"
    });
    let files_result = serialized(&run(&files, query.clone()));
    assert!(!rows(&files_result).is_empty(), "{files_result}");
    for row in rows(&files_result) {
        assert_eq!(row["result_type"], "file", "{files_result}");
        assert_eq!(row["path"], "app/Widget.java", "{files_result}");
    }

    let first = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    let second = serialized(&run(
        &files,
        occurrence_query("app/Widget.java", "callable_applicability"),
    ));
    assert_eq!(first, second, "row identities are content-derived");
}
