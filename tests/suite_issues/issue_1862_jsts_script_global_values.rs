//! Issue #1862: plain top-level values in JavaScript and TypeScript scripts
//! must use the shared program-scope identity.

use crate::common::InlineTestProject;
use crate::common::search_tools::definition_at;
use brokk_bifrost::Language;

fn assert_script_globals(language: Language, extension: &str) {
    let declarations = r#"var scalarGlobal = 1;
const { patternGlobal } = { patternGlobal: 2 };
"#;
    let reader = r#"function read() {
    return scalarGlobal + patternGlobal + moduleOnly;
}

function shadow(scalarGlobal) {
    return scalarGlobal /* local */;
}
"#;
    let module = "export const moduleOnly = 3;\n";
    let declaration_path = format!("src/globals.{extension}");
    let reader_path = format!("src/reader.{extension}");
    let module_path = format!("src/module.{extension}");
    let project = InlineTestProject::with_language(language)
        .file(&declaration_path, declarations)
        .file(&reader_path, reader)
        .file(&module_path, module)
        .build();

    for name in ["scalarGlobal", "patternGlobal"] {
        let result = definition_at(&project, &reader_path, reader, name);
        assert_eq!(result["status"], "resolved", "{name}: {result:#}");
        assert_eq!(result["definitions"][0]["fqn"], name, "{name}: {result:#}");
        assert_eq!(
            result["definitions"][0]["path"], declaration_path,
            "{name}: {result:#}"
        );
        assert_eq!(
            result["definitions"][0]["kind"], "field",
            "{name}: {result:#}"
        );
    }

    let module_only = definition_at(&project, &reader_path, reader, "moduleOnly");
    assert_eq!(
        module_only["status"], "no_definition",
        "a module value must stay outside the script global scope: {module_only:#}"
    );

    let local = definition_at(&project, &reader_path, reader, "scalarGlobal /* local */");
    assert_eq!(local["status"], "resolved", "{local:#}");
    assert_eq!(
        local["definitions"][0]["path"], reader_path,
        "the parameter must shadow the shared program value: {local:#}"
    );
}

#[test]
fn javascript_plain_values_use_the_shared_script_global_scope() {
    assert_script_globals(Language::JavaScript, "js");
}

#[test]
fn typescript_plain_values_use_the_shared_script_global_scope() {
    assert_script_globals(Language::TypeScript, "ts");
}
