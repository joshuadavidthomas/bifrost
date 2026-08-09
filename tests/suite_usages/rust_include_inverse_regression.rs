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
fn included_file_imported_free_function_is_an_inverse_hit() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub mod eval; pub mod generated;\n"),
        ("src/eval/mod.rs", "pub mod defaults; pub mod other;\n"),
        (
            "src/eval/defaults.rs",
            "pub fn default_schema_parity_scorers() {}\n",
        ),
        (
            "src/eval/other.rs",
            "pub fn default_schema_parity_scorers() {}\n",
        ),
        ("src/generated/mod.rs", "pub mod registry;\n"),
        (
            "src/generated/registry.rs",
            "mod nested { include!(\"../../tools/benchmarks/nested.rs\"); }\nmod nested_host { include!(\"../../foreign/src/nested_host.rs\"); }\ninclude!(\"../../tools/benchmarks/spec.rs\");\ninclude!(\"../../tools/benchmarks/decoy.rs\");\ninclude!(\"../../tools/benchmarks/shadow.rs\");\ninclude!(\"../../tools/benchmarks/glob.rs\");\ninclude!(\"../../foreign/src/cross.rs\");\n",
        ),
        (
            "tools/benchmarks/spec.rs",
            "use crate::eval::defaults::default_schema_parity_scorers;\n\nfn query() { default_schema_parity_scorers(); }\n\nmod glob_scope { use crate::eval::defaults::*; fn glob_query() { default_schema_parity_scorers(); } }\n",
        ),
        (
            "tools/benchmarks/decoy.rs",
            "use crate::eval::other::default_schema_parity_scorers;\n\nfn decoy() { default_schema_parity_scorers(); }\n",
        ),
        (
            "tools/benchmarks/standalone.rs",
            "use self::eval::defaults::default_schema_parity_scorers;\n\nfn standalone() { default_schema_parity_scorers(); }\n",
        ),
        ("tools/benchmarks/nested.rs", "include!(\"leaf.rs\");\n"),
        (
            "tools/benchmarks/leaf.rs",
            "use super::super::super::eval::defaults::default_schema_parity_scorers;\n\nfn nested_leaf() { default_schema_parity_scorers(); }\n",
        ),
        (
            "tools/benchmarks/shadow.rs",
            "use crate::eval::defaults::default_schema_parity_scorers;\n\nfn shadowed() { let default_schema_parity_scorers = || {}; default_schema_parity_scorers(); }\n",
        ),
        (
            "tools/benchmarks/glob.rs",
            "use crate::eval::defaults::*;\n\nfn glob_query() { default_schema_parity_scorers(); }\n",
        ),
        (
            "foreign/Cargo.toml",
            "[package]\nname = \"foreign\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("foreign/src/lib.rs", "pub mod cross;\n"),
        (
            "foreign/src/cross.rs",
            "use crate::eval::defaults::default_schema_parity_scorers;\n\nfn cross_cargo() { default_schema_parity_scorers(); }\n",
        ),
        (
            "foreign/src/nested_host.rs",
            "use super::super::super::eval::defaults::default_schema_parity_scorers;\ninclude!(\"nested_host_leaf.rs\");\n",
        ),
        (
            "foreign/src/nested_host_leaf.rs",
            "fn nested_host_leaf() { default_schema_parity_scorers(); }\n",
        ),
        (
            "other/Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("other/src/lib.rs", "pub mod eval;\n"),
        ("other/src/eval/mod.rs", "pub mod defaults;\n"),
        (
            "other/src/eval/defaults.rs",
            "pub fn default_schema_parity_scorers() {}\n",
        ),
    ]);
    let target_file = project.file("src/eval/defaults.rs");
    let target = analyzer
        .get_definitions("fixture.eval.defaults.default_schema_parity_scorers")
        .into_iter()
        .find(|unit| unit.source() == &target_file)
        .unwrap_or_else(|| panic!("missing definition in {target_file:?}"));
    let rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .all_hits_including_imports();
    assert!(
        rows.iter().any(|hit| {
            hit.file == project.file("tools/benchmarks/spec.rs")
                && hit.kind == UsageHitKind::Reference
        }),
        "included free-function call must be an inverse hit: {rows:#?}"
    );
    for included in [
        "tools/benchmarks/leaf.rs",
        "tools/benchmarks/glob.rs",
        "foreign/src/cross.rs",
        "foreign/src/nested_host_leaf.rs",
    ] {
        assert!(
            rows.iter().any(|hit| {
                hit.file == project.file(included) && hit.kind == UsageHitKind::Reference
            }),
            "included route must hit target in {included}: {rows:#?}"
        );
    }
    for near_miss in [
        "tools/benchmarks/decoy.rs",
        "tools/benchmarks/shadow.rs",
        "tools/benchmarks/standalone.rs",
    ] {
        assert!(
            rows.iter().all(|hit| hit.file != project.file(near_miss)),
            "near-miss file must not hit target in {near_miss}: {rows:#?}"
        );
    }

    let duplicate_target_file = project.file("other/src/eval/defaults.rs");
    let duplicate_target = analyzer
        .get_definitions("fixture.eval.defaults.default_schema_parity_scorers")
        .into_iter()
        .find(|unit| unit.source() == &duplicate_target_file)
        .unwrap_or_else(|| panic!("missing duplicate definition in {duplicate_target_file:?}"));
    let duplicate_rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&duplicate_target))
        .all_hits_including_imports();
    for included in [
        "tools/benchmarks/spec.rs",
        "tools/benchmarks/leaf.rs",
        "tools/benchmarks/glob.rs",
        "foreign/src/cross.rs",
    ] {
        assert!(
            duplicate_rows
                .iter()
                .all(|hit| hit.file != project.file(included)),
            "duplicate Cargo-root identity must not hit included file {included}: {duplicate_rows:#?}"
        );
    }
}

#[test]
fn included_file_inherits_host_extern_crate_alias() {
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndep = { path = \"dep\" }\ndep2 = { path = \"dep2\" }\n",
        ),
        (
            "src/lib.rs",
            "extern crate dep as tk; use dep::*; use dep2::*; pub mod generated;\n",
        ),
        ("src/generated/mod.rs", "pub mod registry;\n"),
        (
            "src/generated/registry.rs",
            "mod named_host { use dep::make; include!(\"../../tools/named_host.rs\"); }\ninclude!(\"../../tools/named_import.rs\");\ninclude!(\"../../tools/alias.rs\");\ninclude!(\"../../tools/glob_alias.rs\");\n",
        ),
        ("tools/alias.rs", "fn query() { tk::make(); }\n"),
        ("tools/named_host.rs", "fn query() { make(); }\n"),
        (
            "tools/named_import.rs",
            "use dep::make;\nfn query() { make(); }\n",
        ),
        ("tools/glob_alias.rs", "fn query() { make(); }\n"),
        (
            "dep/Cargo.toml",
            "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("dep/src/lib.rs", "pub fn make() {}\n"),
        (
            "dep2/Cargo.toml",
            "[package]\nname = \"dep2\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("dep2/src/lib.rs", "pub fn make() {}\n"),
    ]);
    let target = definition(&analyzer, "dep.make");
    let rows = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .all_hits_including_imports();
    assert!(
        rows.iter().any(|hit| {
            hit.file == project.file("tools/alias.rs") && hit.kind == UsageHitKind::Reference
        }),
        "included alias call must be an inverse hit: {rows:#?}"
    );
    assert!(
        rows.iter().any(|hit| {
            hit.file == project.file("tools/named_host.rs") && hit.kind == UsageHitKind::Reference
        }),
        "included host named import call must be an inverse hit: {rows:#?}"
    );
    assert!(
        rows.iter().any(|hit| {
            hit.file == project.file("tools/named_import.rs") && hit.kind == UsageHitKind::Reference
        }),
        "included dependency import call must be an inverse hit: {rows:#?}"
    );
    assert!(
        rows.iter()
            .all(|hit| { hit.file != project.file("tools/glob_alias.rs") })
    );
}
