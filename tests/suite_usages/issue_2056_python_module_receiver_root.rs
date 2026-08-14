//! Issue #2056: a Python module variable used as an attribute receiver is a
//! reference to that variable, including when the attribute is a decorator.

use crate::common::InlineTestProject;
use brokk_bifrost::CodeUnitIndex;
use brokk_bifrost::usages::{PythonExportUsageGraphStrategy, UsageAnalyzer, UsageFinder};
use brokk_bifrost::{Language, PythonAnalyzer};
use std::collections::BTreeSet;

#[test]
fn module_receiver_roots_use_the_enclosing_scope_and_preserve_shadowing() {
    let source = r#"class App:
    pass

app = App()

@app.before
def decorated(app):
    return None

def ordinary():
    return app.before

@app.chain.before
def chained(app):
    return None

def shadowed(app):
    return app.before

other = App()

def unrelated():
    return other.before
"#;
    let imported = r#"import remote as app

@app.before
def imported():
    return None
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("fixture.py", source)
        .file("imported.py", imported)
        .file("remote.py", "before = object()\n")
        .build();
    let analyzer = PythonAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("fixture.app")
        .into_iter()
        .next()
        .expect("module variable app");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let expected = ["@app.before", "return app.before", "@app.chain.before"]
        .into_iter()
        .map(|needle| {
            source.find(needle).expect("expected receiver expression")
                + needle.find("app").expect("receiver token")
        })
        .collect::<BTreeSet<_>>();

    let targeted = PythonExportUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&target), &candidates, 1_000)
        .into_either()
        .expect("targeted receiver-root usages");
    let targeted_offsets = targeted
        .iter()
        .filter(|hit| hit.file == project.file("fixture.py"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(targeted_offsets, expected, "{targeted:#?}");
    assert!(
        targeted
            .iter()
            .all(|hit| hit.file != project.file("imported.py")),
        "an imported module alias is not the module variable: {targeted:#?}"
    );

    let default_scope = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .into_either()
        .expect("default receiver-root usages");
    let default_offsets = default_scope
        .iter()
        .filter(|hit| hit.file == project.file("fixture.py"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(default_offsets, expected, "{default_scope:#?}");
}
