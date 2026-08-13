//! Issue #2027: qualified PHP ancestry names retain their type role.

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
fn qualified_extends_and_implements_terminals_resolve_as_types() {
    let parent = r#"<?php
namespace Faker\Provider;
class Address {}
"#;
    let contract = r#"<?php
namespace Faker\Contracts;
interface Generator {}
"#;
    let child = r#"<?php
namespace Faker\Provider\en_US;
final class Address extends \Faker\Provider\Address implements \Faker\Contracts\Generator {}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Faker/Provider/Address.php", parent)
        .file("src/Faker/Contracts/Generator.php", contract)
        .file("src/Faker/Provider/en_US/Address.php", child)
        .build();

    let extends = definition_at(
        &project,
        "src/Faker/Provider/en_US/Address.php",
        child,
        "extends",
        "Address",
    );
    assert_eq!(extends["status"], "resolved", "{extends:#}");
    assert_eq!(
        extends["definitions"][0]["fqn"], "Faker.Provider.Address",
        "{extends:#}"
    );

    let implements = definition_at(
        &project,
        "src/Faker/Provider/en_US/Address.php",
        child,
        "implements",
        "Generator",
    );
    assert_eq!(implements["status"], "resolved", "{implements:#}");
    assert_eq!(
        implements["definitions"][0]["fqn"], "Faker.Contracts.Generator",
        "{implements:#}"
    );
}

#[test]
fn qualified_import_function_and_declaration_roles_do_not_become_ancestry_types() {
    let parent = r#"<?php
namespace Faker\Provider;
class Address {}
"#;
    let caller = r#"<?php
namespace App;
use Faker\Provider\Address;
final class Consumer {
    public function length(): int { return \strlen('x'); }
}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Faker/Provider/Address.php", parent)
        .file("src/App/Consumer.php", caller)
        .build();

    let import = definition_at(
        &project,
        "src/App/Consumer.php",
        caller,
        "use Faker",
        "Address",
    );
    assert_eq!(import["status"], "no_definition", "{import:#}");
    assert!(import["definitions"].as_array().is_none_or(Vec::is_empty));
    assert_eq!(
        import["diagnostics"][0]["kind"], "declaration_or_import_site",
        "{import:#}"
    );

    let declaration = definition_at(
        &project,
        "src/Faker/Provider/Address.php",
        parent,
        "class Address",
        "Address",
    );
    assert_eq!(declaration["status"], "no_definition", "{declaration:#}");
    assert_eq!(
        declaration["diagnostics"][0]["kind"], "declaration_or_import_site",
        "{declaration:#}"
    );

    let builtin = definition_at(
        &project,
        "src/App/Consumer.php",
        caller,
        "return \\strlen",
        "strlen",
    );
    assert_ne!(builtin["status"], "resolved", "{builtin:#}");
    assert!(builtin["definitions"].as_array().is_none_or(Vec::is_empty));
}
