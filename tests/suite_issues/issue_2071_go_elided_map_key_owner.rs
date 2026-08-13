//! Issue #2071: an elided struct literal used as a map key inherits the map's
//! structured key type, not its value type.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::{GoUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{CodeUnitIndex, GoAnalyzer, Language};
use serde_json::{Value, json};

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
fn elided_map_key_and_value_literals_keep_their_distinct_owners() {
    let source = r#"package app

type Key struct {
    TypeName string
    VarName string
}

type Value struct {
    Label string
}

const Named = "named"

var values = map[Key]Value{
    {TypeName: "bool", VarName: "collect"}: {Label: "known"},
}
var direct = map[string]Value{Named: {Label: "direct"}}
"#;
    let project = InlineTestProject::with_language(Language::Go)
        .file("go.mod", "module example.com/app\n")
        .file("model.go", source)
        .build();

    for (occurrence, expected) in [
        ("TypeName: \"bool\"", "example.com/app.Key.TypeName"),
        ("VarName: \"collect\"", "example.com/app.Key.VarName"),
        ("Label: \"known\"", "example.com/app.Value.Label"),
    ] {
        let result = definition_at(&project, "model.go", source, occurrence);
        assert_eq!(result["status"], "resolved", "{result:#}");
        assert_eq!(result["definitions"][0]["fqn"], expected, "{result:#}");
    }

    let direct_key = definition_at(&project, "model.go", source, "Named: {Label");
    assert_eq!(direct_key["status"], "resolved", "{direct_key:#}");
    assert_eq!(
        direct_key["definitions"][0]["fqn"], "example.com/app._module_.Named",
        "{direct_key:#}"
    );

    let analyzer = GoAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("example.com/app.Key.VarName")
        .into_iter()
        .find(|unit| unit.is_field())
        .expect("map-key field");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let hits = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, &[target], &candidates, 1000)
        .into_either()
        .expect("usage result");
    let expected = source.find("VarName: \"collect\"").expect("field label");
    assert_eq!(hits.len(), 1, "{hits:#?}");
    let hit = hits.iter().next().expect("field-label hit");
    assert_eq!(
        (hit.start_offset, hit.end_offset),
        (expected, expected + "VarName".len())
    );
}
