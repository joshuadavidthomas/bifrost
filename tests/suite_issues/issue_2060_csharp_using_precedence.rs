//! Issue #2060: C# file-local namespace imports are resolved before the
//! workspace-wide `global using` approximation.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
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
fn file_using_wins_before_workspace_global_using() {
    let explicit_source = r#"using Explicit;
namespace Consumer;
public sealed class Use { private Config value; }
"#;
    let global_source = r#"namespace GlobalOnly;
public sealed class Use { private Config value; }
"#;
    let current_source = r#"using Explicit;
namespace Current;
public sealed class Config {}
public sealed class Use { private Config value; }
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file(
            "Explicit/Config.cs",
            "namespace Explicit; public class Config {}\n",
        )
        .file(
            "Global/Config.cs",
            "namespace Global; public class Config {}\n",
        )
        .file("GlobalUsings.cs", "global using Global;\n")
        .file("Consumer/Use.cs", explicit_source)
        .file("GlobalOnly/Use.cs", global_source)
        .file("Current/Use.cs", current_source)
        .build();

    let explicit = definition_at(&project, "Consumer/Use.cs", explicit_source, "Config value");
    assert_eq!(explicit["status"], "resolved", "{explicit:#}");
    assert_eq!(
        explicit["definitions"][0]["fqn"], "Explicit.Config",
        "{explicit:#}"
    );

    let global = definition_at(&project, "GlobalOnly/Use.cs", global_source, "Config value");
    assert_eq!(global["status"], "resolved", "{global:#}");
    assert_eq!(
        global["definitions"][0]["fqn"], "Global.Config",
        "{global:#}"
    );

    let current = definition_at(&project, "Current/Use.cs", current_source, "Config value");
    assert_eq!(current["status"], "resolved", "{current:#}");
    assert_eq!(
        current["definitions"][0]["fqn"], "Current.Config",
        "{current:#}"
    );
}

#[test]
fn two_file_local_usings_remain_ambiguous() {
    let source = r#"using Left;
using Right;
namespace Consumer;
public sealed class Use { private Config value; }
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Left/Config.cs", "namespace Left; public class Config {}\n")
        .file(
            "Right/Config.cs",
            "namespace Right; public class Config {}\n",
        )
        .file("Consumer/Use.cs", source)
        .build();

    let result = definition_at(&project, "Consumer/Use.cs", source, "Config value");
    assert_eq!(result["status"], "ambiguous", "{result:#}");
    let mut fqns: Vec<_> = result["definitions"]
        .as_array()
        .expect("definitions")
        .iter()
        .filter_map(|definition| definition["fqn"].as_str())
        .collect();
    fqns.sort();
    assert_eq!(fqns, ["Left.Config", "Right.Config"], "{result:#}");
}
