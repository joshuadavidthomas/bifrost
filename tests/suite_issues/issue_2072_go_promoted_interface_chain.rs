//! Issue #2072: Go inverse lookup follows promoted methods through embedded
//! concrete structs before reaching the declaring interface.

use crate::common::InlineTestProject;
use brokk_bifrost::usages::{GoUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{CodeUnitIndex, GoAnalyzer, Language};
use std::collections::BTreeSet;

#[test]
fn promoted_interface_method_usages_follow_transitive_struct_embedding() {
    let source = r#"package app

type Provider interface {
    Get() string
}

type Inner struct {
    Provider
}

type Outer struct {
    *Inner
}

type Override struct {
    *Outer
}

func (*Override) Get() string { return "override" }

type Left struct { Provider }
type Right struct { Provider }
type Ambiguous struct {
    Left
    Right
}

func readProvider(value Provider) string { return value.Get() }
func readInner(value *Inner) string { return value.Get() }
func readOuter(value *Outer) string { return value.Get() }
func readOverride(value *Override) string { return value.Get() }
func readAmbiguous(value *Ambiguous) string { return value.Get() }
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("model.go", source)
        .build();
    let analyzer = GoAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("example.com/app.Provider.Get")
        .into_iter()
        .find(|unit| unit.is_function())
        .expect("Provider.Get");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();

    let hits = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, &[target], &candidates, 1_000)
        .into_either()
        .expect("Provider.Get usage lookup");
    let actual = hits
        .iter()
        .filter(|hit| hit.file == project.file("model.go"))
        .map(|hit| hit.line)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual,
        BTreeSet::from([28, 29, 30]),
        "direct, one-hop, and two-hop promoted calls should resolve; direct override and ambiguous promotion must not: {hits:#?}",
    );
}

#[test]
fn promoted_interface_method_usages_follow_cross_package_struct_embedding() {
    let provider = r#"package provider

type Provider interface {
    Get() string
}
"#;
    let inner = r#"package inner

import "example.com/app/provider"

type Inner struct {
    provider.Provider
}
"#;
    let caller = r#"package app

import "example.com/app/inner"

type Outer struct {
    *inner.Inner
}

func read(value *Outer) string { return value.Get() }
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("provider/provider.go", provider)
        .file("inner/inner.go", inner)
        .file("caller.go", caller)
        .build();
    let analyzer = GoAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("example.com/app/provider.Provider.Get")
        .into_iter()
        .find(|unit| unit.is_function())
        .expect("Provider.Get");
    let candidates = BTreeSet::from([project.file("caller.go")])
        .into_iter()
        .collect();

    let hits = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, &[target], &candidates, 1_000)
        .into_either()
        .expect("cross-package Provider.Get usage lookup");

    assert!(
        hits.iter()
            .any(|hit| hit.file == project.file("caller.go") && hit.line == 9),
        "the two-hop cross-package promoted call should resolve: {hits:#?}",
    );
}
