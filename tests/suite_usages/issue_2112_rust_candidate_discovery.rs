use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{RustExportUsageGraphStrategy, UsageAnalyzer, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, Language, RustAnalyzer};

#[test]
fn default_discovery_finds_imported_usage_without_rust_graph_augmentation() {
    const DECOY_FILES: usize = 32;

    let mut project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"candidate_discovery\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", "pub mod target;\npub mod consumer;\n")
        .file("src/target.rs", "pub fn read_disk_usage() {}\n")
        .file(
            "src/consumer.rs",
            "use crate::target::read_disk_usage;\npub fn call() { read_disk_usage(); }\n",
        );
    for index in 0..DECOY_FILES {
        project = project.file(
            format!("src/decoy_{index}.rs"),
            "// read_disk_usage is documentation, not a reference.\npub fn unrelated() {}\n",
        );
    }
    let project = project.build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .declarations(&project.file("src/target.rs"))
        .into_iter()
        .find(|declaration| declaration.identifier() == "read_disk_usage")
        .expect("target declaration");

    let query = UsageFinder::new().query(&analyzer, std::slice::from_ref(&target), 1000, 1000);

    let hits = query
        .result
        .into_either()
        .expect("Rust usage scan should succeed");
    assert!(
        hits.iter()
            .any(|hit| hit.file == project.file("src/consumer.rs")),
        "the structurally imported usage must remain: {hits:#?}"
    );
}

#[test]
fn graph_augments_a_nonempty_scope_with_structured_importers() {
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"candidate_discovery\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file("src/lib.rs", "pub mod target;\npub mod consumer;\n")
        .file("src/target.rs", "pub struct UnsizedHandler;\n")
        .file(
            "src/consumer.rs",
            "use crate::target::UnsizedHandler;\npub fn make() { let _ = UnsizedHandler; }\n",
        )
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .declarations(&project.file("src/target.rs"))
        .into_iter()
        .find(|declaration| declaration.identifier() == "UnsizedHandler")
        .expect("target declaration");
    let candidates: HashSet<_> = [project.file("src/target.rs")].into_iter().collect();

    let result = RustExportUsageGraphStrategy::new().find_usages(
        &analyzer,
        std::slice::from_ref(&target),
        &candidates,
        1_000,
    );

    let hits = result
        .into_either()
        .expect("Rust usage scan should succeed");
    assert!(
        hits.iter()
            .any(|hit| hit.file == project.file("src/consumer.rs")),
        "the graph must add structurally discovered importers to the supplied scope: {hits:#?}"
    );
}
