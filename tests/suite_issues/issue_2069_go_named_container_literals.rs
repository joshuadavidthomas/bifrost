//! Issue #2069: named Go container types retain their structured underlying
//! shape when an elided composite literal selects an element, map value, or
//! map key owner.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::{GoUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{CodeUnitIndex, GoAnalyzer, Language};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    occurrence: &str,
) -> Value {
    let start = source.find(occurrence).expect("occurrence");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

#[test]
fn named_container_literals_preserve_nested_elided_field_owners() {
    let source = r#"package app

import ext "example.com/external"

type Item struct { Field string }
type Other struct { Field string }
type NamedArray [2]*Item
type NestedArray [1][1]*Item
type NamedSlice []Item
type NamedMap map[string]Item
type NamedKeyMap map[Item]string

var array = NamedArray{{Field: "array"}}
var nested = NestedArray{{{Field: "nested"}}}
var slice = NamedSlice{{Field: "slice"}}
var mapped = NamedMap{"item": {Field: "map"}}
var keyed = NamedKeyMap{{Field: "key"}: "value"}
var other = []Other{{Field: "other"}}
var unknown = ext.Named{{Field: "unknown"}}
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("model.go", source)
        .build();

    let positives = [
        "Field: \"array\"",
        "Field: \"nested\"",
        "Field: \"slice\"",
        "Field: \"map\"",
        "Field: \"key\"",
    ];
    for occurrence in positives {
        let result = definition_at(&project, "model.go", source, occurrence);
        assert_eq!(result["status"], "resolved", "{occurrence}: {result:#}");
        assert_eq!(
            result["definitions"][0]["fqn"], "example.com/app.Item.Field",
            "{occurrence}: {result:#}"
        );
    }

    let other = definition_at(&project, "model.go", source, "Field: \"other\"");
    assert_eq!(other["status"], "resolved", "{other:#}");
    assert_eq!(
        other["definitions"][0]["fqn"], "example.com/app.Other.Field",
        "{other:#}"
    );

    let unknown = definition_at(&project, "model.go", source, "Field: \"unknown\"");
    assert_ne!(unknown["status"], "resolved", "{unknown:#}");

    let analyzer = GoAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("example.com/app.Item.Field")
        .into_iter()
        .find(|unit| unit.is_field())
        .expect("Item.Field");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let hits = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, &[target], &candidates, 1_000)
        .into_either()
        .expect("targeted Item.Field lookup");
    let actual = hits
        .iter()
        .filter(|hit| hit.file == project.file("model.go"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    let expected = positives
        .into_iter()
        .map(|occurrence| source.find(occurrence).expect("positive field"))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "{hits:#?}");
}
