//! Issue #1971: Python module identity must start at a nested project's
//! configured import root.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::{CodeUnitIndex, Language, PythonAnalyzer};
use serde_json::{Value, json};

const ADMIN: &str = r#"from pkg.user import helper

class Controller:
    def helper(self):
        return "wrong"

    def run(self):
        return helper()
"#;

fn definitions(value: &Value) -> Vec<(String, String)> {
    value["results"][0]["definitions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|definition| {
            (
                definition["fqn"].as_str().unwrap_or_default().to_string(),
                definition["path"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

#[test]
fn nested_setuptools_project_root_resolves_imported_function() {
    let project = InlineTestProject::with_language(Language::Python)
        .file(
            "backend/pyproject.toml",
            "[tool.setuptools.packages.find]\nwhere = [\".\"]\ninclude = [\"pkg*\"]\n",
        )
        // This file makes the old __init__.py heuristic include `backend`.
        .file("backend/__init__.py", "")
        .file("backend/pkg/__init__.py", "")
        .file(
            "backend/pkg/user.py",
            "def helper():\n    return 'right'\n\nclass Other:\n    def helper(self):\n        return 'wrong'\n",
        )
        .file("backend/pkg/admin.py", ADMIN)
        .build();
    let line_index = ADMIN
        .lines()
        .position(|line| line.trim() == "return helper()")
        .expect("call line");
    let column = ADMIN
        .lines()
        .nth(line_index)
        .unwrap()
        .find("helper")
        .unwrap();
    let args = json!({"references": [{
        "path": "backend/pkg/admin.py",
        "line": line_index + 1,
        "column": column + 1
    }]})
    .to_string();
    let result = call_search_tool_json(project.root(), "get_definitions_by_location", &args);

    assert_eq!(result["results"][0]["status"], "resolved", "{result}");
    assert_eq!(
        definitions(&result),
        vec![(
            "pkg.user.helper".to_string(),
            "backend/pkg/user.py".to_string()
        )],
        "the imported function must win over the same-name class method: {result}"
    );
}

#[test]
fn setuptools_src_root_and_unrelated_manifest_keep_structured_module_names() {
    let project = InlineTestProject::with_language(Language::Python)
        .file(
            "src_project/pyproject.toml",
            "[tool.setuptools.packages.find]\nwhere = [\"src\"]\n",
        )
        .file("src_project/src/__init__.py", "")
        .file("src_project/src/pkg/__init__.py", "")
        .file(
            "src_project/src/pkg/service.py",
            "class Service:\n    pass\n",
        )
        .file("plain/pyproject.toml", "[project]\nname = \"plain\"\n")
        .file("plain/__init__.py", "")
        .file("plain/pkg/__init__.py", "")
        .file("plain/pkg/model.py", "class Model:\n    pass\n")
        .build();
    let analyzer = PythonAnalyzer::from_project(project.project().clone());

    let service = analyzer
        .get_declarations(&project.file("src_project/src/pkg/service.py"))
        .into_iter()
        .find(|unit| unit.identifier() == "Service")
        .expect("src-layout class");
    assert_eq!(service.fq_name(), "pkg.service.Service");

    let model = analyzer
        .get_declarations(&project.file("plain/pkg/model.py"))
        .into_iter()
        .find(|unit| unit.identifier() == "Model")
        .expect("plain package class");
    assert_eq!(
        model.fq_name(),
        "plain.pkg.model.Model",
        "a pyproject without supported package-root metadata changes nothing"
    );
}
