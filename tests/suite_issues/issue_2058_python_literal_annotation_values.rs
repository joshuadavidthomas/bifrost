//! Issue #2058: string values inside `Literal[...]` are runtime values, not
//! deferred annotation references.

use crate::common::InlineTestProject;
use brokk_bifrost::usages::{PythonExportUsageGraphStrategy, UsageAnalyzer};
use brokk_bifrost::{CodeUnitIndex, Language, PythonAnalyzer};

#[test]
fn literal_string_values_do_not_reference_same_named_module_fields() {
    let source = r#"import typing
import typing_extensions
from typing import Literal

NOASSERTION = object()

def deferred(value: "NOASSERTION") -> None:
    pass

def direct(value: Literal["NOASSERTION"]) -> None:
    pass

def qualified(value: typing.Literal["NOASSERTION"]) -> None:
    pass

def extension(value: typing_extensions.Literal["NOASSERTION"]) -> None:
    pass
"#;
    let project = InlineTestProject::with_language(Language::Python)
        .file("service.py", source)
        .build();
    let analyzer = PythonAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("service.NOASSERTION")
        .into_iter()
        .find(|unit| unit.is_field())
        .expect("module field");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let hits = PythonExportUsageGraphStrategy::new()
        .find_usages(&analyzer, &[target], &candidates, 1000)
        .into_either()
        .expect("usage result");

    let expected = source.find("\"NOASSERTION\"").expect("deferred annotation") + 1;
    assert_eq!(hits.len(), 1, "{hits:#?}");
    let hit = hits.iter().next().expect("deferred hit");
    assert_eq!(
        (hit.start_offset, hit.end_offset),
        (expected, expected + 11)
    );
}
