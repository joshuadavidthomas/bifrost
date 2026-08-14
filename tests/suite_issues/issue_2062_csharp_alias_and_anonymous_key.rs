//! Issue #2062: C# alias-qualified types and anonymous-object output names
//! retain their structured roles instead of falling through to flat lookup.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    occurrence: &str,
    needle: &str,
) -> Value {
    let occurrence_start = source.find(occurrence).expect("occurrence");
    let start = occurrence_start
        + source[occurrence_start..]
            .find(needle)
            .expect("needle after occurrence");
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
fn alias_qualified_type_does_not_fall_through_to_same_named_workspace_type() {
    let source = r#"using pb = global::External.Protobuf;
using local = global::Workspace;
namespace Example;

public interface IMessage {}
public sealed class Request : pb::IMessage<Request> {}
public sealed class LocalRequest : local::IMessage {}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Request.cs", source)
        .file(
            "Workspace.cs",
            "namespace Workspace; public interface IMessage {}\n",
        )
        .build();
    let result = definition_at(&project, "Request.cs", source, "pb::IMessage", "IMessage");

    assert_eq!(
        result["status"], "unresolvable_import_boundary",
        "{result:#}"
    );
    assert!(result["definitions"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(
        result["diagnostics"][0]["kind"], "unresolvable_import_boundary",
        "{result:#}"
    );

    let local = definition_at(
        &project,
        "Request.cs",
        source,
        "local::IMessage",
        "IMessage",
    );
    assert_eq!(local["status"], "resolved", "{local:#}");
    assert_eq!(
        local["definitions"][0]["fqn"], "Workspace.IMessage",
        "{local:#}"
    );
}

#[test]
fn anonymous_object_name_is_adjudicated_while_its_value_remains_a_reference() {
    let source = r#"namespace Example;

public sealed class Probe
{
    private void OnCompleted() {}
    private string Label = "label";

    public object Build() => new
    {
        OnCompleted = nameof(OnCompleted),
        Label,
    };
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Probe.cs", source)
        .build();

    let key = definition_at(&project, "Probe.cs", source, "OnCompleted =", "OnCompleted");
    assert_eq!(key["status"], "no_definition", "{key:#}");
    assert!(key["definitions"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(
        key["diagnostics"][0]["kind"], "declaration_or_import_site",
        "{key:#}"
    );

    let value = definition_at(
        &project,
        "Probe.cs",
        source,
        "nameof(OnCompleted)",
        "OnCompleted",
    );
    assert_eq!(value["status"], "resolved", "{value:#}");
    assert_eq!(
        value["definitions"][0]["fqn"], "Example.Probe.OnCompleted",
        "{value:#}"
    );

    let shorthand = definition_at(&project, "Probe.cs", source, "        Label,", "Label");
    assert_eq!(shorthand["status"], "resolved", "{shorthand:#}");
    assert_eq!(
        shorthand["definitions"][0]["fqn"], "Example.Probe.Label",
        "{shorthand:#}"
    );
}
