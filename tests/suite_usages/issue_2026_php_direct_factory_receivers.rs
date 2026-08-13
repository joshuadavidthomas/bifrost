use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::usages::{PhpUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{CodeUnitIndex, Language, PhpAnalyzer};
use std::collections::BTreeSet;

const SOURCE: &str = r#"<?php
namespace App;

class Product {
    public function consume(): void {}
}

class Other {
    public function consume(): void {}
}

function makeProduct(): Product { return new Product(); }
function makeOther(): Other { return new Other(); }
function makeUnknown() { return new Product(); }

class Factory {
    public static function makeProduct(): Product { return new Product(); }
    public static function makeOther(): Other { return new Other(); }
}

class Consumer {
    public static function makeProduct(): Product { return new Product(); }

    public function viaFree(): void { makeProduct()->consume(); }
    public function viaScoped(): void { Factory::makeProduct()->consume(); }
    public function viaSelf(): void { self::makeProduct()->consume(); }
    public function viaParenthesized(): void { (Factory::makeProduct())->consume(); }

    public function otherFree(): void { makeOther()->consume(); }
    public function otherScoped(): void { Factory::makeOther()->consume(); }

    public function unknown(): void { makeUnknown()->consume(); }
    public function dynamic(): void {
        $factory = fn(): Product => new Product();
        $factory()->consume();
    }
}
"#;

fn member_offset(expression: &str) -> usize {
    let expression_start = SOURCE
        .find(expression)
        .unwrap_or_else(|| panic!("missing expression {expression}"));
    expression_start
        + expression
            .rfind("consume")
            .unwrap_or_else(|| panic!("missing consume member in {expression}"))
}

fn project() -> crate::common::BuiltInlineTestProject {
    InlineTestProject::with_language(Language::Php)
        .file("FactoryReceivers.php", SOURCE)
        .build()
}

#[test]
fn targeted_php_usages_follow_direct_factory_return_types() {
    let project = project();
    let analyzer = PhpAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .definitions("App.Product.consume")
        .next()
        .expect("Product.consume declaration");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let hits = PhpUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&target), &candidates, 1000)
        .into_either()
        .expect("targeted PHP usage scan");

    let actual = hits
        .iter()
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect::<BTreeSet<_>>();
    let expected = [
        "makeProduct()->consume()",
        "Factory::makeProduct()->consume()",
        "self::makeProduct()->consume()",
        "(Factory::makeProduct())->consume()",
    ]
    .into_iter()
    .map(|expression| {
        let start = member_offset(expression);
        (start, start + "consume".len())
    })
    .collect::<BTreeSet<_>>();

    assert_eq!(
        actual, expected,
        "unexpected Product.consume usages: {hits:?}"
    );
}

#[test]
fn whole_php_graph_follows_direct_factory_return_types_without_guessing() {
    let project = project();
    let graph = usage_graph_at(project.root(), "{}");

    for caller in ["viaFree", "viaScoped", "viaSelf", "viaParenthesized"] {
        assert!(
            has_edge(
                &graph,
                &format!("App.Consumer.{caller}"),
                "App.Product.consume"
            ),
            "expected {caller} -> Product.consume: {}",
            graph["edges"]
        );
    }
    for caller in ["otherFree", "otherScoped"] {
        assert!(
            has_edge(
                &graph,
                &format!("App.Consumer.{caller}"),
                "App.Other.consume"
            ),
            "expected {caller} -> Other.consume: {}",
            graph["edges"]
        );
        assert!(
            !has_edge(
                &graph,
                &format!("App.Consumer.{caller}"),
                "App.Product.consume"
            ),
            "{caller} must not edge to Product.consume: {}",
            graph["edges"]
        );
    }
    for caller in ["unknown", "dynamic"] {
        assert!(
            !has_edge(
                &graph,
                &format!("App.Consumer.{caller}"),
                "App.Product.consume"
            ),
            "untyped {caller} must not edge to Product.consume: {}",
            graph["edges"]
        );
    }
}
