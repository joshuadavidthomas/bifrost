//! Issue #2061: a parameter default value is a value reference, not part of
//! the parameter declaration role that contains it.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(project: &BuiltInlineTestProject, source: &str, occurrence: &str) -> Value {
    let start = source.find(occurrence).expect("occurrence");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": "Defaults.cs", "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

#[test]
fn parameter_default_values_resolve_enclosing_constants() {
    let source = r#"namespace App;

class DefaultLimit { }

class Service
{
    private const int DefaultLimitValue = 30;

    public void Run(int count = DefaultLimitValue) { }
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Defaults.cs", source)
        .build();

    let default_value = definition_at(&project, source, "DefaultLimitValue)");
    assert_eq!(default_value["status"], "resolved", "{default_value:#}");
    assert_eq!(
        default_value["definitions"][0]["fqn"], "App.Service.DefaultLimitValue",
        "{default_value:#}"
    );

    let parameter_name = definition_at(&project, source, "count =");
    assert_eq!(parameter_name["status"], "resolved", "{parameter_name:#}");
    assert_eq!(
        parameter_name["definitions"][0]["kind"], "parameter",
        "{parameter_name:#}"
    );
}
