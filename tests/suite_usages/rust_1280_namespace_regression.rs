use crate::common::InlineTestProject;
use brokk_bifrost::usages::{UsageFinder, UsageHit, UsageHitKind};
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

fn reference_hits(analyzer: &RustAnalyzer, target: &CodeUnit) -> Vec<UsageHit> {
    UsageFinder::new()
        .find_usages_default(analyzer, std::slice::from_ref(target))
        .all_hits_including_imports()
        .into_iter()
        .filter(|hit| hit.kind == UsageHitKind::Reference)
        .collect()
}

fn has_marked_reference(hits: &[UsageHit], marker: &str) -> bool {
    hits.iter().any(|hit| hit.snippet.contains(marker))
}

#[test]
fn function_local_namespace_import_resolves_nested_type_without_cross_scope_fallback() {
    let (_project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod extjson;\n\
             pub mod sibling;\n\
             pub struct DateTimeBody;\n\
             mod consumer;\n",
        ),
        (
            "src/extjson/mod.rs",
            "pub mod models;\n\
             macro_rules! make_macro { () => {}; }\n\
             pub fn make() {}\n",
        ),
        ("src/extjson/models.rs", "pub struct DateTimeBody;\n"),
        ("src/sibling/mod.rs", "pub mod models;\n"),
        ("src/sibling/models.rs", "pub struct DateTimeBody;\n"),
        ("src/consumer/mod.rs", "pub mod nested;\n"),
        (
            "src/consumer/nested.rs",
            r#"
fn local_namespace() {
    use crate::extjson;
    let _: extjson::models::DateTimeBody = todo(); // POSITIVE_LOCAL_NAMESPACE
}

fn direct_namespace_alias() {
    use crate::extjson as ej;
    let _: ej::models::DateTimeBody = todo(); // POSITIVE_DIRECT_ALIAS
}

fn grouped_namespace_alias() {
    use crate::{extjson as grouped_ej};
    let _: grouped_ej::models::DateTimeBody = todo(); // POSITIVE_GROUPED_ALIAS
}

fn local_item_shadow() {
    use crate::extjson;
    struct extjson;
    let _: extjson::models::DateTimeBody = todo(); // LOCAL_ITEM_SHADOW
}

use crate::extjson as outer_module;
use crate::extjson as function_outer;
use crate::extjson as const_outer;
use crate::extjson as static_outer;
use crate::extjson as m;

fn outer_import_inner_item_shadow() {
    struct outer_module;
    let _: outer_module::models::DateTimeBody = todo(); // OUTER_IMPORT_INNER_ITEM_SHADOW
}

fn value_item_same_name_positive() {
    fn function_outer() {}
    let _: function_outer::models::DateTimeBody = todo(); // POSITIVE_FUNCTION_SAME_NAME
}

fn const_item_same_name_positive() {
    const const_outer: usize = 0;
    let _: const_outer::models::DateTimeBody = todo(); // POSITIVE_CONST_SAME_NAME
}

fn static_item_same_name_positive() {
    static static_outer: usize = 0;
    let _: static_outer::models::DateTimeBody = todo(); // POSITIVE_STATIC_SAME_NAME
}

fn macro_only_inner_alias() {
    use crate::extjson::make_macro as m;
    let _: m::models::DateTimeBody = todo(); // POSITIVE_MACRO_ONLY_INNER_ALIAS
}

fn parameter_shadow(extjson: usize) {
    use crate::extjson;
    let _ = extjson; // PARAMETER_SHADOW
}

fn alias_outside_function() {
    let _: extjson::models::DateTimeBody = todo(); // ALIAS_OUTSIDE_FUNCTION
}

fn sibling_same_name() {
    use crate::sibling;
    let _: sibling::models::DateTimeBody = todo(); // SIBLING_SAME_NAME
}

fn named_non_module_alias() {
    use crate::DateTimeBody as ej;
    let _: ej::models::DateTimeBody = todo(); // NAMED_NON_MODULE_ALIAS
}

fn value_only_inner_alias() {
    use crate::extjson::make as m;
    let _: m::models::DateTimeBody = todo(); // POSITIVE_VALUE_ONLY_INNER_ALIAS
}

fn type_item_inner_alias() {
    use crate::extjson::models::DateTimeBody as m;
    let _: m::models::DateTimeBody = todo(); // TYPE_ITEM_INNER_ALIAS
}

fn local_type_item_shadow() {
    use crate::extjson as type_outer;
    struct type_outer;
    let _: type_outer::models::DateTimeBody = todo(); // LOCAL_TYPE_ITEM_SHADOW
}
"#,
        ),
    ]);
    let target = definition(&analyzer, "fixture.extjson.models.DateTimeBody");
    let hits = reference_hits(&analyzer, &target);

    assert!(
        has_marked_reference(&hits, "POSITIVE_LOCAL_NAMESPACE"),
        "function-local namespace import must resolve the nested type: {hits:#?}"
    );
    for marker in [
        "POSITIVE_DIRECT_ALIAS",
        "POSITIVE_GROUPED_ALIAS",
        "POSITIVE_VALUE_ONLY_INNER_ALIAS",
        "POSITIVE_FUNCTION_SAME_NAME",
        "POSITIVE_CONST_SAME_NAME",
        "POSITIVE_STATIC_SAME_NAME",
        "POSITIVE_MACRO_ONLY_INNER_ALIAS",
    ] {
        assert!(
            has_marked_reference(&hits, marker),
            "function-local module alias must resolve the nested type ({marker}): {hits:#?}"
        );
    }
    for marker in [
        "LOCAL_ITEM_SHADOW",
        "PARAMETER_SHADOW",
        "ALIAS_OUTSIDE_FUNCTION",
        "SIBLING_SAME_NAME",
        "NAMED_NON_MODULE_ALIAS",
        "OUTER_IMPORT_INNER_ITEM_SHADOW",
        "TYPE_ITEM_INNER_ALIAS",
        "LOCAL_TYPE_ITEM_SHADOW",
    ] {
        assert!(
            hits.iter().all(|hit| !hit.snippet.contains(marker)),
            "near-miss path must not resolve the extjson target ({marker}): {hits:#?}"
        );
    }
}
