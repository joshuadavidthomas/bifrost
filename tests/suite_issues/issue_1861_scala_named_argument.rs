//! Issue #1861: Scala named arguments belong to the selected callable.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, Language, ScalaAnalyzer};
use std::sync::Arc;

#[test]
fn scala_named_argument_does_not_select_an_unrelated_member() {
    let source = r#"package app

class PersistentSegmentOne {
  def updateCount: Int = 0
}

object PersistentSegmentOne {
  def apply(updateCount: Int): PersistentSegmentOne = new PersistentSegmentOne
}

object Use {
  val segment = PersistentSegmentOne(
    updateCount = 1
  )
}
"#;
    let project = InlineTestProject::with_language(Language::Scala)
        .file("app/PersistentSegmentOne.scala", source)
        .build();
    let result = definition_at(
        &project,
        "app/PersistentSegmentOne.scala",
        source,
        "updateCount = 1",
    );

    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(
        result["definitions"][0]["fqn"], "app.PersistentSegmentOne$.apply",
        "the parameter is not the class member with the same name: {result:#}"
    );

    let analyzer = ScalaAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.fq_name() == "app.PersistentSegmentOne$.apply")
        .expect("companion apply declaration");
    let file = project.file("app/PersistentSegmentOne.scala");
    let provider = ExplicitCandidateProvider::new(Arc::new(std::iter::once(file).collect()));
    let result = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            &analyzer,
            std::slice::from_ref(&target),
            Some(&provider),
            1,
            100,
        )
        .result;
    let FuzzyResult::Success {
        hits_by_overload, ..
    } = result
    else {
        panic!("expected authoritative Scala usage result");
    };
    let label_start = source.rfind("updateCount = 1").expect("named argument");
    assert!(
        hits_by_overload
            .get(&target)
            .into_iter()
            .flatten()
            .any(|hit| {
                hit.start_offset == label_start
                    && hit.end_offset == label_start + "updateCount".len()
            }),
        "the inverse result must attribute the label to the selected apply callable: {hits_by_overload:#?}"
    );
}
