use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder, UsageHit};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, IAnalyzer, Language, ProjectFile, RustAnalyzer};
use std::collections::BTreeSet;
use std::sync::Arc;

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

fn member(
    analyzer: &RustAnalyzer,
    file: &ProjectFile,
    owner_name: &str,
    member_name: &str,
) -> CodeUnit {
    analyzer
        .exact_member(file, owner_name, member_name, true)
        .or_else(|| analyzer.exact_member(file, owner_name, member_name, false))
        .unwrap_or_else(|| panic!("missing member {owner_name}.{member_name}"))
}

fn authoritative_hits(
    analyzer: &dyn IAnalyzer,
    target: &CodeUnit,
    files: HashSet<ProjectFile>,
) -> BTreeSet<UsageHit> {
    let provider = ExplicitCandidateProvider::new(Arc::new(files));
    match UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            analyzer,
            std::slice::from_ref(target),
            Some(&provider),
            100,
            100,
        )
        .result
    {
        FuzzyResult::Success {
            hits_by_overload, ..
        } => hits_by_overload
            .into_values()
            .flat_map(BTreeSet::into_iter)
            .collect(),
        other => panic!("expected authoritative Rust usage success, got {other:#?}"),
    }
}

#[test]
fn rust_top30_macro_crate_associated_call_resolves_across_files() {
    let macro_source = r#"
macro_rules! call_len {
    ($row:expr) => {
        let _ = $crate::row::Row::len($row);
    };
}
"#;
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"macro_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        ("src/lib.rs", "pub mod row;\npub mod macros;\n"),
        (
            "src/row.rs",
            "pub trait Row { fn len(row: &Self) -> usize; }\n",
        ),
        ("src/macros.rs", macro_source),
    ]);
    let target = member(&analyzer, &project.file("src/row.rs"), "Row", "len");
    let hits = authoritative_hits(
        &analyzer,
        &target,
        [project.file("src/macros.rs")].into_iter().collect(),
    );
    let len = macro_source
        .find("Row::len")
        .map(|start| start + "Row::".len())
        .expect("macro associated path");
    assert!(
        hits.iter()
            .any(|hit| hit.file == project.file("src/macros.rs") && hit.start_offset == len),
        "macro token-tree associated call must resolve exactly: {hits:#?}"
    );
}

#[test]
fn rust_top30_macro_where_clause_paths_resolve_across_files() {
    let macro_source = r#"
macro_rules! require_traits {
    ($( $T:ident ),+ $(,)?) => {
        impl<'r, R, $($T,)+> crate::from_row::FromRow<'r, R> for ($($T,)+)
        where
            R: crate::row::Row,
            usize: crate::column::ColumnIndex<R>,
            $($T: crate::decode::Decode<'r, R::Database> + crate::types::Type<R::Database>,)+
        {
        }
    };
}
"#;
    let (project, analyzer) = rust_analyzer_with_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"macro_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod column;\npub mod decode;\npub mod from_row;\npub mod row;\npub mod types;\npub mod macros;\n",
        ),
        ("src/column.rs", "pub trait ColumnIndex<R> {}\n"),
        ("src/decode.rs", "pub trait Decode<'r, DB> {}\n"),
        ("src/from_row.rs", "pub trait FromRow<'r, R> {}\n"),
        ("src/row.rs", "pub trait Row { type Database; }\n"),
        ("src/types.rs", "pub trait Type<DB> {}\n"),
        ("src/macros.rs", macro_source),
    ]);
    for (target_file_path, target_name, path) in [
        ("src/decode.rs", "Decode", "crate::decode::Decode"),
        ("src/types.rs", "Type", "crate::types::Type"),
    ] {
        let target_file = project.file(target_file_path);
        let target = analyzer
            .declarations(&target_file)
            .into_iter()
            .find(|unit| unit.identifier() == target_name)
            .expect("trait definition");
        let hits = authoritative_hits(
            &analyzer,
            &target,
            [project.file("src/macros.rs")].into_iter().collect(),
        );
        let path_start = macro_source.find(path).expect("macro where-clause path");
        let terminal_start = path_start + path.len() - target_name.len();
        assert!(
            hits.iter().any(|hit| {
                hit.file == project.file("src/macros.rs")
                    && hit.start_offset == terminal_start
                    && hit.end_offset == terminal_start + target_name.len()
            }),
            "macro where-clause path must resolve {target_name}: {hits:#?}"
        );
    }
}
