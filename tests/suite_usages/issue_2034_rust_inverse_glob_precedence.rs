use crate::common::InlineTestProject;
use brokk_bifrost::usages::{UsageFinder, UsageHit};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, Language, RustAnalyzer};
use std::collections::BTreeSet;

fn declaration_in(
    analyzer: &RustAnalyzer,
    file: &brokk_bifrost::ProjectFile,
    name: &str,
) -> CodeUnit {
    analyzer
        .declarations(file)
        .into_iter()
        .find(|declaration| declaration.is_function() && declaration.identifier() == name)
        .unwrap_or_else(|| panic!("missing function {name} in {file:?}"))
}

fn spans_in(hits: &BTreeSet<UsageHit>, file: &brokk_bifrost::ProjectFile) -> Vec<(usize, usize)> {
    let mut spans = hits
        .iter()
        .filter(|hit| &hit.file == file)
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect::<Vec<_>>();
    spans.sort_unstable();
    spans
}

#[test]
fn same_file_function_precedes_glob_but_explicit_and_unshadowed_imports_remain() {
    let shadowed = r#"use crate::rounding::{round_nearest_tie_even as imported_round, *};

fn round_nearest_tie_even(value: i32, floor: i32, ceiling: i32) -> i32 {
    value.clamp(floor, ceiling)
}

pub fn direct() -> i32 {
    round_nearest_tie_even(5, 0, 10)
}

pub fn closure() -> i32 {
    let apply = || round_nearest_tie_even(7, 0, 10);
    apply()
}

pub fn explicit() -> i32 {
    imported_round(9)
}
"#;
    let glob_only = r#"use crate::rounding::*;

pub fn call() -> i32 {
    round_nearest_tie_even(11)
}
"#;
    let explicit = r#"use crate::decoy::*;
use crate::rounding::round_nearest_tie_even;

pub fn call() -> i32 {
    round_nearest_tie_even(13)
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"issue_2034\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .file(
            "src/lib.rs",
            "pub mod rounding;\npub mod decoy;\npub mod shadowed;\npub mod glob_only;\npub mod explicit;\n",
        )
        .file(
            "src/rounding.rs",
            "pub fn round_nearest_tie_even(value: i32) -> i32 { value }\n",
        )
        .file(
            "src/decoy.rs",
            "pub fn round_nearest_tie_even(value: i32) -> i32 { value + 1 }\n",
        )
        .file("src/shadowed.rs", shadowed)
        .file("src/glob_only.rs", glob_only)
        .file("src/explicit.rs", explicit)
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let shadowed_file = project.file("src/shadowed.rs");
    let glob_only_file = project.file("src/glob_only.rs");
    let explicit_file = project.file("src/explicit.rs");
    let local = declaration_in(&analyzer, &shadowed_file, "round_nearest_tie_even");
    let imported = declaration_in(
        &analyzer,
        &project.file("src/rounding.rs"),
        "round_nearest_tie_even",
    );
    let decoy = declaration_in(
        &analyzer,
        &project.file("src/decoy.rs"),
        "round_nearest_tie_even",
    );

    let direct = shadowed.find("round_nearest_tie_even(5").unwrap();
    let closure = shadowed.find("round_nearest_tie_even(7").unwrap();
    let alias_call = shadowed.find("imported_round(9").unwrap();
    let glob_call = glob_only.find("round_nearest_tie_even(11").unwrap();
    let explicit_call = explicit.find("round_nearest_tie_even(13").unwrap();
    let name_len = "round_nearest_tie_even".len();

    let local_hits = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&local))
        .all_hits();
    assert_eq!(
        vec![(direct, direct + name_len), (closure, closure + name_len)],
        spans_in(&local_hits, &shadowed_file),
        "the same-file function must own direct and closure calls: {local_hits:#?}"
    );

    let imported_hits = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&imported))
        .all_hits();
    assert_eq!(
        vec![(alias_call, alias_call + "imported_round".len())],
        spans_in(&imported_hits, &shadowed_file),
        "the explicit alias remains a reference without claiming shadowed calls: {imported_hits:#?}"
    );
    assert_eq!(
        vec![(glob_call, glob_call + name_len)],
        spans_in(&imported_hits, &glob_only_file),
        "a glob remains usable when no same-file declaration competes: {imported_hits:#?}"
    );
    assert_eq!(
        vec![(explicit_call, explicit_call + name_len)],
        spans_in(&imported_hits, &explicit_file),
        "an explicit import must beat a competing glob: {imported_hits:#?}"
    );

    let decoy_hits = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&decoy))
        .all_hits();
    assert!(
        spans_in(&decoy_hits, &explicit_file).is_empty(),
        "the lower-precedence glob must not claim the explicitly imported call: {decoy_hits:#?}"
    );
}
