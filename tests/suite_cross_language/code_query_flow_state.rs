//! End-to-end coverage of the flow-sensitive state-event and flow-relation
//! query surface (#1480, Milestone 3).
//!
//! Every assertion here is about *behavior visible on the wire*: the rows a
//! query returns, the fields they carry, and the diagnostics a partial
//! derivation leaves behind. Nothing asserts a registry list.
//!
//! The load-bearing property is that the answers come from the production
//! control-flow graph and nothing else. A read that follows its establishment
//! in straight-line code is reached exactly; a read that precedes the only
//! establishment gets no reaching row even though the write is right there in
//! the source. Source order is not evidence, and this suite is where that stops
//! being a claim.

use crate::common::InlineTestProject;
use brokk_bifrost::analyzer::structural::{CodeQuery, CodeQueryResult, execute_workspace};
use brokk_bifrost::{AnalyzerConfig, WorkspaceAnalyzer};
use serde_json::{Value, json};

/// A straight-line body: the binding is established, then read.
const JS_STRAIGHT_LINE: &str = r#"
function afterEstablishment(seed) {
  let total = seed;
  return total;
}
"#;

/// The only write to `later` follows the read of it on every path.
const JS_READ_BEFORE_WRITE: &str = r#"
function beforeEstablishment(seed) {
  let later = seed;
  const early = later;
  later = 2;
  return early;
}
"#;

/// One-armed conditional establishment: some entry-to-read path misses the
/// write, so the relation is `may` and no dominance row exists for it.
const JS_CONDITIONAL: &str = r#"
function conditionalEstablishment(flag, seed) {
  let value = seed;
  if (flag) {
    value = 1;
  }
  return value;
}
"#;

/// `x = wrap(x)`: the read feeds the value the write assigns.
const JS_SAME_EVALUATION: &str = r#"
function wrap(value) {
  return value;
}

function sameAssignment(x) {
  x = wrap(x);
  return x;
}
"#;

/// A file whose language has no CFG lowering at all, so every axis of the
/// derivation is uncovered and the response must say so.
const PLAIN_TEXT: &str = "this file is not a program\n";

fn workspace(files: &[(&str, &str)]) -> (WorkspaceAnalyzer, crate::common::BuiltInlineTestProject) {
    let mut project = InlineTestProject::new();
    for (path, source) in files {
        project = project.file(*path, *source);
    }
    let project = project.build();
    let workspace = WorkspaceAnalyzer::build(project.project_dyn(), AnalyzerConfig::default());
    (workspace, project)
}

fn run(files: &[(&str, &str)], query: Value) -> Value {
    let (workspace, _project) = workspace(files);
    let query = CodeQuery::from_json(&query).expect("query should parse");
    serialize(&execute_workspace(&workspace, &query))
}

fn run_rql(files: &[(&str, &str)], source: &str) -> Value {
    let (workspace, _project) = workspace(files);
    let query = CodeQuery::from_sexp(source).expect("RQL should parse");
    serialize(&execute_workspace(&workspace, &query))
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

fn state_events(value: &Value) -> Vec<&Value> {
    rows_of(value, "state_event")
}

fn flow_relations(value: &Value) -> Vec<&Value> {
    rows_of(value, "flow_relation")
}

fn diagnostic_codes(value: &Value) -> Vec<String> {
    value["diagnostics"]
        .as_array()
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic["code"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The state-event query for one named function, in JSON.
fn state_events_query(function: &str, filters: Value) -> Value {
    let mut step = json!({ "op": "state_events_of" });
    if let Some(object) = filters.as_object() {
        for (key, value) in object {
            step[key] = value.clone();
        }
    }
    json!({
        "schema_version": 1,
        "match": { "kind": "function", "name": function },
        "steps": [{ "op": "procedure_of" }, step]
    })
}

// ---------------------------------------------------------------------------
// State events
// ---------------------------------------------------------------------------

#[test]
fn a_straight_line_body_yields_binding_state_events_with_both_classes() {
    let value = run(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        state_events_query("afterEstablishment", json!({})),
    );
    let events = state_events(&value);
    assert!(!events.is_empty(), "expected state events; got {value}");
    for event in &events {
        assert_eq!(event["path"], "src/main.js", "{value}");
        assert!(event["procedure_id"].is_string(), "{value}");
        assert!(event["id"].is_string(), "{value}");
        assert!(
            ["binding", "property"].contains(&event["subject"].as_str().unwrap_or_default()),
            "{value}"
        );
        assert!(event["program_point"].is_u64(), "{value}");
        assert!(event["generation"].is_u64(), "{value}");
    }
    let classes: Vec<&str> = events
        .iter()
        .filter_map(|event| event["event_class"].as_str())
        .collect();
    assert!(classes.contains(&"establish"), "{value}");
    assert!(classes.contains(&"read"), "{value}");
}

#[test]
fn a_class_filter_keeps_only_the_named_event_class() {
    let value = run(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        state_events_query("afterEstablishment", json!({ "event_class": ["read"] })),
    );
    let events = state_events(&value);
    assert!(!events.is_empty(), "expected read events; got {value}");
    for event in &events {
        assert_eq!(event["event_class"], "read", "{value}");
    }
}

#[test]
fn a_subject_filter_keeps_only_the_named_subject_kind() {
    let value = run(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        state_events_query("afterEstablishment", json!({ "subject": ["binding"] })),
    );
    for event in state_events(&value) {
        assert_eq!(event["subject"], "binding", "{value}");
    }
}

// ---------------------------------------------------------------------------
// Flow relations
// ---------------------------------------------------------------------------

/// The acceptance shape: a read *after* its establishment in straight-line code
/// is reached, and with `exact` certainty, because the write is the only
/// definition in the read's IN set and it dominates the read.
#[test]
fn a_read_after_its_establishment_is_reached_exactly() {
    let value = run_rql(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        r#"(flow-relations-of :relation [reaching] :certainty [exact]
             (state-events-of (procedure-of (function :name "afterEstablishment"))))"#,
    );
    let relations = flow_relations(&value);
    assert!(
        !relations.is_empty(),
        "a read after its establishment must be reached; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["relation"], "reaching", "{value}");
        assert_eq!(relation["certainty"], "exact", "{value}");
        assert_eq!(relation["source"]["event_class"], "establish", "{value}");
        assert_eq!(relation["target"]["event_class"], "read", "{value}");
        assert!(relation["source"]["path"].is_string(), "{value}");
        assert!(relation["target"]["program_point"].is_u64(), "{value}");
    }
}

/// The other half of the same claim: the source has a write to `later`, but it
/// follows the read on every path, so nothing reaches that read. Source
/// co-presence is not evidence.
#[test]
fn a_read_before_the_only_write_is_not_reached_by_it() {
    let value = run_rql(
        &[("src/main.js", JS_READ_BEFORE_WRITE)],
        r#"(flow-relations-of :relation [reaching]
             (state-events-of :class [read] :subject [binding]
               (procedure-of (function :name "beforeEstablishment"))))"#,
    );
    // The seeded event is the `const early = later` read plus the `return early`
    // read. Neither may be served by the `later = 2` write, whose only path to
    // them runs backwards.
    let reaching_from_the_later_write = flow_relations(&value).into_iter().filter(|relation| {
        relation["source"]["event_class"] == "establish"
            && relation["target"]["range"]["start_line"].as_u64()
                < relation["source"]["range"]["start_line"].as_u64()
    });
    assert_eq!(
        reaching_from_the_later_write.count(),
        0,
        "a write that follows a read on every path must not reach it; got {value}"
    );
}

/// A one-armed conditional establishment: some entry-to-read path misses the
/// write, so the surviving reaching row is `may`.
#[test]
fn a_one_armed_conditional_establishment_reaches_with_may_certainty() {
    let value = run_rql(
        &[("src/main.js", JS_CONDITIONAL)],
        r#"(flow-relations-of :relation [reaching] :certainty [may]
             (state-events-of (procedure-of (function :name "conditionalEstablishment"))))"#,
    );
    let relations = flow_relations(&value);
    assert!(
        !relations.is_empty(),
        "a one-armed conditional write must reach its read with may certainty; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["certainty"], "may", "{value}");
    }
}

/// `x = wrap(x)`: the read feeds the value the write assigns, so the write
/// cannot serve it. This is the `9e60fddcb` shape.
#[test]
fn a_write_does_not_serve_the_read_inside_its_own_evaluation() {
    let value = run_rql(
        &[("src/main.js", JS_SAME_EVALUATION)],
        r#"(flow-relations-of :relation [same-evaluation]
             (state-events-of (procedure-of (function :name "sameAssignment"))))"#,
    );
    let relations = flow_relations(&value);
    assert!(
        !relations.is_empty(),
        "x = wrap(x) must relate its read and its write as same-evaluation; got {value}"
    );
    for relation in &relations {
        assert_eq!(relation["relation"], "same_evaluation", "{value}");
    }
}

/// The projections are exactly the two ends of the row they project.
#[test]
fn flow_source_and_flow_target_project_the_relations_two_ends() {
    let files = [("src/main.js", JS_STRAIGHT_LINE)];
    let sources = run_rql(
        &files,
        r#"(flow-source (flow-relations-of :relation [reaching]
             (state-events-of (procedure-of (function :name "afterEstablishment")))))"#,
    );
    let targets = run_rql(
        &files,
        r#"(flow-target (flow-relations-of :relation [reaching]
             (state-events-of (procedure-of (function :name "afterEstablishment")))))"#,
    );
    let source_events = state_events(&sources);
    let target_events = state_events(&targets);
    assert!(!source_events.is_empty(), "{sources}");
    assert!(!target_events.is_empty(), "{targets}");
    for event in source_events {
        assert_eq!(event["event_class"], "establish", "{sources}");
    }
    for event in target_events {
        assert_eq!(event["event_class"], "read", "{targets}");
    }
}

/// Seeded from a state event rather than a procedure, only the relations
/// incident to that event come back.
#[test]
fn relations_seeded_from_an_event_are_incident_to_that_event() {
    let value = run_rql(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        r#"(flow-relations-of
             (state-events-of :class [establish]
               (procedure-of (function :name "afterEstablishment"))))"#,
    );
    let relations = flow_relations(&value);
    assert!(!relations.is_empty(), "{value}");
    for relation in &relations {
        assert!(
            relation["source"]["event_class"] == "establish"
                || relation["target"]["event_class"] == "establish",
            "every relation seeded from an establishment must name one; got {value}"
        );
    }
}

// ---------------------------------------------------------------------------
// Honesty surface
// ---------------------------------------------------------------------------

/// A file that does not lower to a control-flow graph produces no rows *and* an
/// explicit incomplete diagnostic. An empty answer is never silently complete.
#[test]
fn a_non_lowering_file_reports_incompleteness_rather_than_an_empty_answer() {
    let value = run(
        &[("src/main.js", JS_STRAIGHT_LINE), ("notes.txt", PLAIN_TEXT)],
        json!({
            "schema_version": 1,
            "where": ["notes.txt"],
            "match": { "kind": "function" },
            "steps": [{ "op": "procedure_of" }, { "op": "state_events_of" }]
        }),
    );
    assert!(
        state_events(&value).is_empty(),
        "a non-lowering file has no state events; got {value}"
    );
    assert_ne!(
        value["completion"], "complete",
        "an answer with no evidence must not report itself complete; got {value}"
    );
}

/// A derivation that leaves an axis uncovered says so on the row itself, so a
/// filtered row set still carries its own account.
#[test]
fn rows_carry_their_derivations_completeness_account() {
    let value = run(
        &[("src/main.js", JS_STRAIGHT_LINE)],
        state_events_query("afterEstablishment", json!({})),
    );
    let events = state_events(&value);
    assert!(!events.is_empty(), "{value}");
    for event in &events {
        let completeness = event["completeness"].as_str().unwrap_or_default();
        assert!(
            completeness == "complete" || completeness == "partial",
            "every state event states its derivation's completeness; got {value}"
        );
        if completeness == "partial" {
            assert!(
                event["uncovered_axes"].is_array(),
                "a partial row names the axes it does not answer; got {value}"
            );
            assert!(
                diagnostic_codes(&value)
                    .iter()
                    .any(|code| code.starts_with("flow_state_")),
                "a partial derivation leaves a typed diagnostic; got {value}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Frontend parity
// ---------------------------------------------------------------------------

/// Both frontends spell the same query and both produce the same rows.
#[test]
fn the_json_and_rql_frontends_agree_on_one_flow_state_query() {
    let files = [("src/main.js", JS_STRAIGHT_LINE)];
    let from_json = run(
        &files,
        json!({
            "schema_version": 1,
            "match": { "kind": "function", "name": "afterEstablishment" },
            "steps": [
                { "op": "procedure_of" },
                { "op": "state_events_of", "event_class": ["read"] },
                { "op": "flow_relations_of", "flow_relation": ["reaching"] }
            ]
        }),
    );
    let from_rql = run_rql(
        &files,
        r#"(flow-relations-of :relation [reaching]
             (state-events-of :class [read]
               (procedure-of (function :name "afterEstablishment"))))"#,
    );
    let json_ids: Vec<&Value> = flow_relations(&from_json)
        .into_iter()
        .map(|relation| &relation["id"])
        .collect();
    let rql_ids: Vec<&Value> = flow_relations(&from_rql)
        .into_iter()
        .map(|relation| &relation["id"])
        .collect();
    assert!(!json_ids.is_empty(), "{from_json}");
    assert_eq!(json_ids, rql_ids, "{from_json}\n{from_rql}");
}
