use crate::common::InlineTestProject;
use brokk_bifrost::usages::{UsageFinder, UsageHitKind};
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
fn nested_bench_group_imports_are_inverse_hits_for_private_items() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"candle-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("benches/bench_main.rs", "mod benchmarks;\n"),
        (
            "benches/benchmarks/mod.rs",
            "pub struct BenchDevice; struct BenchDeviceHandler;\nmod cat;\n",
        ),
        (
            "benches/benchmarks/cat.rs",
            "use crate::benchmarks::{BenchDevice, BenchDeviceHandler};\nfn run() { let _: BenchDeviceHandler; }\n",
        ),
        (
            "benches/other.rs",
            "use crate::benchmarks::BenchDeviceHandler;\n",
        ),
    ]);
    let target = definition(
        &analyzer,
        "candle_core.benches.benchmarks.BenchDeviceHandler",
    );
    let rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .all_hits_including_imports();
    let import = rows
        .iter()
        .find(|hit| {
            hit.file == project.file("benches/benchmarks/cat.rs")
                && hit.kind == UsageHitKind::Import
        })
        .expect("grouped import terminal should resolve");
    assert_eq!((import.start_offset, import.end_offset), (37, 55));
    assert!(rows.iter().all(|hit| {
        hit.file != project.file("benches/other.rs") || hit.kind != UsageHitKind::Import
    }));
}

#[test]
fn generated_super_imports_are_inverse_hits() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"sdk-test-procedure-cpp-client\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/main.rs", "mod module_bindings;\n"),
        (
            "src/module_bindings/mod.rs",
            "pub mod my_table_table; pub mod return_struct_type;\n",
        ),
        (
            "src/module_bindings/my_table_table.rs",
            "use super::return_struct_type::ReturnStruct;\nfn consume(_: ReturnStruct) {}\n",
        ),
        (
            "src/module_bindings/return_struct_type.rs",
            "pub struct ReturnStruct;\n",
        ),
    ]);
    let target = definition(
        &analyzer,
        "sdk_test_procedure_cpp_client.module_bindings.return_struct_type.ReturnStruct",
    );
    let rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .all_hits_including_imports();
    assert!(rows.iter().any(|hit| {
        hit.file == project.file("src/module_bindings/my_table_table.rs")
            && hit.kind == UsageHitKind::Import
    }));
}

#[test]
fn cfg_test_external_crate_alias_imports_are_inverse_hits() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "tokenizers/Cargo.toml",
            "[package]\nname = \"tokenizers\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "tokenizers/src/lib.rs",
            "pub mod models; pub mod pre_tokenizers;\n",
        ),
        ("tokenizers/src/models/mod.rs", "pub struct ModelWrapper;\n"),
        (
            "tokenizers/src/pre_tokenizers/mod.rs",
            "pub mod whitespace; pub struct PreTokenizerWrapper;\n",
        ),
        (
            "tokenizers/src/pre_tokenizers/whitespace.rs",
            "pub struct Whitespace; pub struct WhitespaceSplit;\n",
        ),
        (
            "bindings/python/Cargo.toml",
            "[package]\nname = \"tokenizers-python\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\nname = \"tokenizers\"\n[dependencies]\ntokenizers = { path = \"../../tokenizers\" }\n",
        ),
        (
            "bindings/python/src/lib.rs",
            "extern crate tokenizers as tk;\nmod models; mod pre_tokenizers;\n",
        ),
        (
            "bindings/python/src/models.rs",
            "#[cfg(test)]\nmod test {\n    use tk::models::ModelWrapper;\n    fn check(_: ModelWrapper) {}\n}\n",
        ),
        (
            "bindings/python/src/pre_tokenizers.rs",
            "#[cfg(test)]\nmod test {\n    use tk::pre_tokenizers::whitespace::{Whitespace, WhitespaceSplit};\n    use tk::pre_tokenizers::PreTokenizerWrapper;\n    fn check(_: Whitespace, _: WhitespaceSplit, _: PreTokenizerWrapper) {}\n}\n",
        ),
    ]);
    for (target_fqn, file) in [
        (
            "tokenizers.models.ModelWrapper",
            "bindings/python/src/models.rs",
        ),
        (
            "tokenizers.pre_tokenizers.whitespace.Whitespace",
            "bindings/python/src/pre_tokenizers.rs",
        ),
        (
            "tokenizers.pre_tokenizers.whitespace.WhitespaceSplit",
            "bindings/python/src/pre_tokenizers.rs",
        ),
        (
            "tokenizers.pre_tokenizers.PreTokenizerWrapper",
            "bindings/python/src/pre_tokenizers.rs",
        ),
    ] {
        let target = definition(&analyzer, target_fqn);
        let rows = UsageFinder::new()
            .find_usages_default(&analyzer, std::slice::from_ref(&target))
            .all_hits_including_imports();
        assert!(
            rows.iter()
                .any(|hit| hit.file == project.file(file) && hit.kind == UsageHitKind::Import)
        );
    }
}

#[test]
fn extern_crate_alias_stays_namespace_scoped() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\ndep = { path = \"../dep\" }\n",
        ),
        (
            "app/src/lib.rs",
            "extern crate dep as tk;\nmod dep;\nfn run() { let _: tk::Item; }\n",
        ),
        ("app/src/dep/mod.rs", "pub struct Item;\n"),
        (
            "dep/Cargo.toml",
            "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("dep/src/lib.rs", "pub struct Item;\n"),
    ]);
    let local = definition(&analyzer, "app.dep");
    let dependency = definition(&analyzer, "dep.Item");
    let local_rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&local))
        .all_hits_including_imports();
    let dependency_rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&dependency))
        .all_hits_including_imports();
    let alias_file = project.file("app/src/lib.rs");
    let alias_source = alias_file.read_to_string().expect("alias source");
    let alias_start = alias_source.find("tk::Item").expect("alias reference");
    assert!(
        local_rows.iter().all(|hit| hit.file != alias_file
            || hit.start_offset > alias_start
            || hit.end_offset < alias_start + "tk".len()),
        "the extern alias must not resolve to the same-named local module: {local_rows:?}"
    );
    assert!(
        dependency_rows
            .iter()
            .any(|hit| hit.file == alias_file && hit.kind == UsageHitKind::Reference),
        "rows={dependency_rows:?}"
    );
}
