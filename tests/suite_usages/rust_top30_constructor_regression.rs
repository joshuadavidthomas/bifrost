use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::UsageAnalyzer;
use brokk_bifrost::{CodeUnit, CodeUnitIndex, Language, RustAnalyzer};

fn rust_analyzer_with_files(
    files: &[(&str, &str)],
) -> (crate::common::BuiltInlineTestProject, RustAnalyzer) {
    let mut builder = InlineTestProject::with_language(Language::Rust);
    for (path, contents) in files {
        builder = builder.file(path, *contents);
    }
    let project = builder.build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

fn definition(analyzer: &RustAnalyzer, fq_name: &str) -> CodeUnit {
    analyzer
        .get_definitions(fq_name)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("missing definition for {fq_name}"))
}

#[test]
fn imported_tuple_constructor_with_documented_field_keeps_public_visibility() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        ("src/lib.rs", "pub mod types;\npub mod user;\n"),
        ("src/types/mod.rs", "mod oid;\npub use oid::Oid;\n"),
        (
            "src/types/oid.rs",
            "pub struct Oid(\n    /// The raw value.\n    pub u32,\n);\n",
        ),
        (
            "src/user.rs",
            "use crate::types::Oid;\n\npub fn run() { let _ = Oid(1); }\n",
        ),
        (
            "examples/decoy.rs",
            "struct Oid(pub u32);\nfn run() { let _ = Oid(9); }\n",
        ),
    ]);
    let target = definition(&analyzer, "types.oid.Oid");
    let hits = brokk_bifrost::usages::RustExportUsageGraphStrategy::new()
        .find_usages(
            &analyzer,
            std::slice::from_ref(&target),
            &HashSet::from_iter([
                project.file("src/user.rs"),
                project.file("examples/decoy.rs"),
            ]),
            100,
        )
        .into_either()
        .expect("documented tuple constructor should resolve");

    assert_eq!(1, hits.len(), "tuple constructor hits: {hits:#?}");
    let hit = hits.iter().next().expect("tuple constructor hit");
    assert_eq!(hit.file, project.file("src/user.rs"));
    assert!(hit.snippet.contains("Oid(1)"));
}
