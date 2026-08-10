use crate::common::InlineTestProject;
use brokk_bifrost::searchtools::{
    DefinitionReferenceQuery, GetDefinitionParams, get_definitions_by_location,
};
use brokk_bifrost::{AnalyzerConfig, Language};

fn query(path: &str, source: &str, line: usize, name: &str) -> DefinitionReferenceQuery {
    let text = source.lines().nth(line - 1).expect("fixture line");
    DefinitionReferenceQuery {
        path: path.to_string(),
        line: Some(line),
        column: Some(text.rfind(name).expect("name on fixture line") + 1),
    }
}

#[test]
fn typescript_later_lexical_binding_resolves_throughout_its_scope() {
    let source = r#"export function setup() {
  const deferred = () => later();
  const eager = later();
  const later = () => 1;
  const siblingRead = () => sibling();
  {
    const sibling = () => 2;
  }
  return { deferred, eager, siblingRead };
}
"#;
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("scope.ts", source)
        .build();
    let workspace = project.workspace_analyzer(AnalyzerConfig::default());
    let response = get_definitions_by_location(
        workspace.analyzer(),
        GetDefinitionParams {
            references: vec![
                query("scope.ts", source, 2, "later"),
                query("scope.ts", source, 3, "later"),
                query("scope.ts", source, 5, "sibling"),
            ],
        },
    );

    for result in &response.results[..2] {
        assert_eq!(result.status, "resolved", "{response:#?}");
        assert_eq!(result.definitions.len(), 1, "{response:#?}");
        assert_eq!(result.definitions[0].name, "later", "{response:#?}");
        assert_eq!(result.definitions[0].start_line, 4, "{response:#?}");
    }
    assert_eq!(
        response.results[2].status, "no_definition",
        "a declaration in a sibling block must not bind the callback: {response:#?}"
    );
}
