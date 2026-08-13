//! Issue #2073: a Go package variable remains a reference when tree-sitter
//! spells its selector-base token `package_identifier`.

use crate::common::{definition, go_analyzer_with_files};
use brokk_bifrost::CodeUnitIndex;
use brokk_bifrost::usages::{GoUsageGraphStrategy, UsageAnalyzer, UsageFinder};
use std::collections::BTreeSet;

#[test]
fn package_variable_selector_bases_exclude_imports_and_local_shadows() {
    let consumer = r#"package app

func use() bool {
    return Service.Allowed[Service.Default]
}

func shadow() bool {
    Service := Settings{}
    return Service.Allowed[Service.Default]
}
"#;
    let (project, analyzer) = go_analyzer_with_files(&[
        (
            "state.go",
            r#"package app

type Settings struct {
    Allowed []bool
    Default int
}

var Service = Settings{}
"#,
        ),
        ("consumer.go", consumer),
        (
            "imported.go",
            r#"package app

import Service "example.com/app/remote"

func imported() int {
    return Service.Value
}
"#,
        ),
        ("remote/remote.go", "package remote\n\nvar Value = 1\n"),
    ]);
    let target = definition(&analyzer, "example.com/app._module_.Service");
    let candidates = analyzer.get_analyzed_files().into_iter().collect();
    let expected_offsets = consumer
        .match_indices("Service")
        .take(2)
        .map(|(offset, _)| offset)
        .collect::<BTreeSet<_>>();

    let targeted = GoUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&target), &candidates, 1_000)
        .into_either()
        .expect("targeted package-variable lookup");
    let targeted_offsets = targeted
        .iter()
        .filter(|hit| hit.file == project.file("consumer.go"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(targeted_offsets, expected_offsets, "{targeted:#?}");
    assert!(
        targeted
            .iter()
            .all(|hit| hit.file != project.file("imported.go")),
        "an imported package alias is not the package variable: {targeted:#?}"
    );

    let whole_workspace = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .into_either()
        .expect("whole-workspace package-variable lookup");
    let whole_offsets = whole_workspace
        .iter()
        .filter(|hit| hit.file == project.file("consumer.go"))
        .map(|hit| hit.start_offset)
        .collect::<BTreeSet<_>>();
    assert_eq!(whole_offsets, expected_offsets, "{whole_workspace:#?}");
    assert!(
        whole_workspace
            .iter()
            .all(|hit| hit.file != project.file("imported.go")),
        "an imported package alias is not the package variable: {whole_workspace:#?}"
    );
}
