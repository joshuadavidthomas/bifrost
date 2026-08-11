//! Issue #1860: Scala self-type and anonymous-class member declarations.
//!
//! Self-type member visibility already uses structured type bounds. Anonymous
//! class bodies are also template bodies, but before this issue their named
//! members had no `CodeUnit` owner and were absent from the declaration index.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::searchtools::{
    ScanUsagesByLocationParams, ScanUsagesTarget, scan_usages_by_location,
};
use brokk_bifrost::{CodeUnitIndex, Language, ScalaAnalyzer};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    after: &str,
    needle: &str,
) -> Value {
    let anchor = source
        .find(after)
        .unwrap_or_else(|| panic!("`{after}` is not present in {path}"));
    let start = anchor
        + source[anchor..]
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` is not present in {path} after `{after}`"));
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]}).to_string();
    call_tool(project, "get_definitions_by_location", &args)["results"][0].clone()
}

const TASK: &str = r#"package app

trait Task {
  def run(x: Int): String
}
"#;

const HOLDERS: &str = r#"package app

object Holders {
  val first: Task = new Task {
    def run(x: Int): String = run("first")(x)
    private def run(label: String)(x: Int): String = label + x
  }

  val second: Task = new Task {
    def run(x: Int): String = run("second")(x)
    private def run(label: String)(x: Int): String = label + x
  }
}
"#;

fn anonymous_project() -> BuiltInlineTestProject {
    InlineTestProject::with_language(Language::Scala)
        .file("app/Task.scala", TASK)
        .file("app/Holders.scala", HOLDERS)
        .build()
}

#[test]
fn anonymous_class_methods_have_distinct_source_backed_owners() {
    let project = anonymous_project();
    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let file = project.file("app/Holders.scala");
    let methods = analyzer
        .declarations(&file)
        .into_iter()
        .filter(|unit| unit.is_function() && unit.identifier() == "run")
        .collect::<Vec<_>>();

    assert_eq!(
        methods.len(),
        4,
        "each anonymous class must publish both overloads: {methods:#?}"
    );
    assert!(
        methods.iter().all(|method| !method.is_synthetic()),
        "named anonymous-class members must remain user-visible: {methods:#?}"
    );
    let owners = methods
        .iter()
        .map(|method| method.owner_identifier().expect("anonymous method owner"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        owners.len(),
        2,
        "the two anonymous classes must have different owners: {methods:#?}"
    );
    assert!(
        owners.iter().all(|owner| owner.contains("anon$")),
        "anonymous owners must use an explicit synthetic identity: {owners:#?}"
    );
}

#[test]
fn anonymous_class_sibling_overload_resolves_inside_its_own_body() {
    let project = anonymous_project();
    let mut owners = Vec::new();
    for after in ["= run(\"first\")", "= run(\"second\")"] {
        let result = definition_at(&project, "app/Holders.scala", HOLDERS, after, "run");
        assert_eq!(
            result["definitions"].as_array().map(Vec::len),
            Some(2),
            "both same-owner overloads must resolve: {result:#}"
        );
        let result_owners = result["definitions"]
            .as_array()
            .expect("definitions")
            .iter()
            .filter_map(|definition| definition["fqn"].as_str())
            .collect::<BTreeSet<_>>();
        let owner = result_owners.first().expect("anonymous owner fqn");
        assert!(
            result_owners.len() == 1 && owner.contains("anon$") && owner.ends_with(".run"),
            "the call must resolve only within one anonymous owner: {result:#}"
        );
        owners.push((*owner).to_string());
    }
    assert_ne!(
        owners[0], owners[1],
        "the two bodies must not share an owner"
    );
}

#[test]
fn anonymous_class_private_overload_has_an_inverse_call_site() {
    let project = anonymous_project();
    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let scan = scan_usages_by_location(
        &analyzer,
        ScanUsagesByLocationParams {
            targets: vec![ScanUsagesTarget {
                path: "app/Holders.scala".to_string(),
                line: 6,
                column: Some(17),
                symbol: None,
            }],
            include_tests: true,
            paths: None,
            include_same_owner: true,
            max_duration_secs: None,
        },
    );
    let lines = scan.results[0]
        .files
        .iter()
        .chain(&scan.results[0].same_owner_files)
        .flat_map(|file| file.hits.iter().map(|hit| hit.line))
        .collect::<BTreeSet<_>>();
    assert!(
        lines.contains(&5),
        "the private overload must own the sibling call in its anonymous body: {scan:#?}"
    );
    assert!(
        !lines.contains(&10),
        "the second anonymous body must not leak into the first owner's scan: {scan:#?}"
    );
}

#[test]
fn package_less_root_anonymous_class_has_a_valid_identity() {
    const SOURCE: &str = r#"trait Task { def run: Int }

new Task {
  def run: Int = 1
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("Root.scala", SOURCE)
        .build();
    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let declarations = analyzer.declarations(&project.file("Root.scala"));

    assert!(
        declarations.iter().any(|unit| {
            unit.is_class() && unit.is_synthetic() && unit.identifier().starts_with("$anon$")
        }),
        "the package-less anonymous class must have a consistent root identity: {declarations:#?}"
    );
}

#[test]
fn packaged_root_anonymous_class_has_a_valid_identity() {
    const SOURCE: &str = r#"package app

trait Task { def run: Int }

new Task {
  def run: Int = 1
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("Root.scala", SOURCE)
        .build();
    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let declarations = analyzer.declarations(&project.file("Root.scala"));

    assert!(
        declarations.iter().any(|unit| {
            unit.is_class()
                && unit.is_synthetic()
                && unit.package_name() == "app"
                && unit.identifier().starts_with("$anon$")
        }),
        "the packaged anonymous class must have a consistent root identity: {declarations:#?}"
    );
}

#[test]
fn root_anonymous_class_identity_includes_its_source() {
    const FIRST: &str = r#"package app

trait Task { def run: Int }

new Task {
  def run: Int = 1
}
"#;
    const SECOND: &str = r#"package app

trait Task { def run: Int }

new Task {
  def run: Int = 1
}
// A different source blob must not reuse the anonymous owner identity.
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("First.scala", FIRST)
        .file("Second.scala", SECOND)
        .build();
    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let owners = ["First.scala", "Second.scala"]
        .map(|path| {
            analyzer
                .declarations(&project.file(path))
                .into_iter()
                .find(|unit| {
                    unit.is_class()
                        && unit.is_synthetic()
                        && unit.identifier().starts_with("$anon$")
                })
                .expect("root anonymous owner")
                .fq_name()
        })
        .into_iter()
        .collect::<BTreeSet<_>>();

    assert_eq!(
        owners.len(),
        2,
        "root anonymous owners at the same location in different files must remain distinct: {owners:#?}"
    );
}

#[test]
fn compound_self_type_exposes_members_from_each_bound() {
    const SOURCE: &str = r#"package query

trait Parser { def parse(value: Int): Int = value }
trait Tokens { def ws: Int = 2 }
trait QueryParser { self: Parser with Tokens =>
  def query: Int = parse(1) + ws
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("query/QueryParser.scala", SOURCE)
        .build();

    for (after, expected) in [
        ("= parse", "query.Parser.parse"),
        ("parse(1) + ws", "query.Tokens.ws"),
    ] {
        let result = definition_at(
            &project,
            "query/QueryParser.scala",
            SOURCE,
            after,
            expected.rsplit('.').next().expect("member name"),
        );
        assert_eq!(
            result["definitions"][0]["fqn"], expected,
            "the compound self-type bound must expose {expected}: {result:#}"
        );
    }
}
