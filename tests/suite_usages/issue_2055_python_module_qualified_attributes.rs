//! Issue #2055: `from package import submodule` introduces a module binding,
//! so ordinary attributes read through that binding belong to the submodule.

use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::usages::{PythonExportUsageGraphStrategy, UsageAnalyzer, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, Language, PythonAnalyzer};
use std::collections::BTreeSet;

fn occurrence(source: &str, needle: &str) -> usize {
    source.find(needle).expect("fixture occurrence")
}

#[test]
fn named_submodule_bindings_resolve_ordinary_attributes_on_every_usage_surface() {
    let source = r#"try:
    from pkg import models
except ImportError:
    models = None

from external import models as outside

class Holder:
    class Model:
        pass

if models is None:
    ModelAlias = None
    VALUE_ALIAS = None
else:
    ModelAlias = models.Model
    VALUE_ALIAS = models.VALUE

def unrelated():
    return Holder.Model, outside.Model

def shadow(models):
    return models.Model
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("pkg/__init__.py", "")
        .file("pkg/models.py", "class Model:\n    pass\n\nVALUE = 1\n")
        .file("user.py", source)
        .file(
            "alias_user.py",
            "from pkg import models as alias\n\ndef load():\n    return alias.Model\n",
        )
        .build();
    let analyzer = PythonAnalyzer::from_project(project.project().clone());
    let model = analyzer
        .get_definitions("pkg.models.Model")
        .into_iter()
        .next()
        .expect("module class");
    let value = analyzer
        .get_definitions("pkg.models.VALUE")
        .into_iter()
        .next()
        .expect("module field");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let expected_model_offsets =
        BTreeSet::from([occurrence(source, "models.Model") + "models.".len()]);

    let targeted = PythonExportUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&model), &candidates, 1_000)
        .into_either()
        .expect("targeted module-qualified class usages");
    let targeted_offsets = targeted
        .iter()
        .filter(|hit| hit.file == project.file("user.py"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(targeted_offsets, expected_model_offsets, "{targeted:#?}");

    let default_scope = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&model))
        .into_either()
        .expect("default module-qualified class usages");
    let default_offsets = default_scope
        .iter()
        .filter(|hit| hit.file == project.file("user.py"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        default_offsets, expected_model_offsets,
        "{default_scope:#?}"
    );

    let value_hits = PythonExportUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&value), &candidates, 1_000)
        .into_either()
        .expect("targeted module-qualified field usages");
    assert_eq!(
        value_hits
            .iter()
            .filter(|hit| hit.file == project.file("user.py"))
            .map(|hit| hit.start_offset)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([occurrence(source, "models.VALUE") + "models.".len()]),
        "{value_hits:#?}"
    );

    let graph = usage_graph_at(project.root(), "{}");
    assert!(
        has_edge(&graph, "alias_user.load", "pkg.models.Model"),
        "aliased module-class edge missing: {}",
        graph["edges"]
    );
}
