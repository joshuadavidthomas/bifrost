use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{RustExportUsageGraphStrategy, UsageAnalyzer, UsageHitKind};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, Language, RustAnalyzer};

fn definition(analyzer: &RustAnalyzer, fq_name: &str) -> CodeUnit {
    analyzer
        .get_definitions(fq_name)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("missing definition for {fq_name}"))
}

#[test]
fn macro_wrapped_declarations_have_cross_file_inverse_usages() {
    let trace = r#"cfg_rt! {
    pub(crate) struct SpawnMeta {
        pub(crate) size: usize,
    }

    impl SpawnMeta {
        pub(crate) fn new_unnamed(size: usize) -> SpawnMeta {
            SpawnMeta { size }
        }
    }
}
"#;
    let pool = r#"use crate::trace::SpawnMeta;

pub fn spawn_blocking(fn_size: usize) -> usize {
    let meta = SpawnMeta::new_unnamed(fn_size);
    meta.size
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"macrodecl\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .file(
            "src/lib.rs",
            "#[macro_use]\nmod macros;\npub mod trace;\npub mod pool;\n",
        )
        .file(
            "src/macros.rs",
            "macro_rules! cfg_rt { ($($item:item)*) => { $($item)* }; }\n",
        )
        .file("src/trace.rs", trace)
        .file("src/pool.rs", pool)
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let pool_file = project.file("src/pool.rs");
    let candidates: HashSet<_> = analyzer.get_analyzed_files().into_iter().collect();

    let spawn_meta = definition(&analyzer, "macrodecl.trace.SpawnMeta");
    let type_hits = RustExportUsageGraphStrategy::new()
        .find_usages(
            &analyzer,
            std::slice::from_ref(&spawn_meta),
            &candidates,
            100,
        )
        .into_either()
        .expect("wrapped struct usage result");
    let type_reference = pool.find("SpawnMeta::new_unnamed").expect("type reference");
    assert!(
        type_hits.iter().any(|hit| {
            hit.file == pool_file
                && hit.kind == UsageHitKind::Reference
                && hit.start_offset == type_reference
                && hit.end_offset == type_reference + "SpawnMeta".len()
        }),
        "wrapped struct inverse hits: {type_hits:#?}"
    );

    let new_unnamed = analyzer
        .exact_member(
            &project.file("src/trace.rs"),
            "SpawnMeta",
            "new_unnamed",
            true,
        )
        .or_else(|| {
            analyzer.exact_member(
                &project.file("src/trace.rs"),
                "SpawnMeta",
                "new_unnamed",
                false,
            )
        })
        .expect("wrapped method definition");
    let method_hits = RustExportUsageGraphStrategy::new()
        .find_usages(
            &analyzer,
            std::slice::from_ref(&new_unnamed),
            &candidates,
            100,
        )
        .into_either()
        .expect("wrapped method usage result");
    let method_reference = type_reference + "SpawnMeta::".len();
    assert!(
        method_hits.iter().any(|hit| {
            hit.file == pool_file
                && hit.kind == UsageHitKind::Reference
                && hit.start_offset == method_reference
                && hit.end_offset == method_reference + "new_unnamed".len()
        }),
        "wrapped method inverse hits: {method_hits:#?}"
    );
}

#[test]
fn macro_wrapped_trait_has_bound_supertrait_and_impl_inverse_usages() {
    let consumer = r#"use crate::layer::Filter;

pub trait FilterExt<S>: Filter<S> {}

pub struct Wrapper;

impl<S> Filter<S> for Wrapper {}

pub fn apply<S, F>(filter: F)
where
    F: Filter<S>,
{
    let _ = filter;
}
"#;
    let project = InlineTestProject::with_language(Language::Rust)
        .file(
            "Cargo.toml",
            "[package]\nname = \"macrotrait\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .file(
            "src/lib.rs",
            "#[macro_use]\nmod macros;\npub mod layer;\npub mod consumer;\n",
        )
        .file(
            "src/macros.rs",
            "macro_rules! feature { ($($item:item)*) => { $($item)* }; }\n",
        )
        .file(
            "src/layer.rs",
            "feature! {\n    pub trait Filter<S> {}\n}\n",
        )
        .file("src/consumer.rs", consumer)
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    let target = definition(&analyzer, "macrotrait.layer.Filter");
    let candidates: HashSet<_> = analyzer.get_analyzed_files().into_iter().collect();
    let hits = RustExportUsageGraphStrategy::new()
        .find_usages(&analyzer, std::slice::from_ref(&target), &candidates, 100)
        .into_either()
        .expect("wrapped trait usage result");
    let consumer_file = project.file("src/consumer.rs");
    let expected = [
        consumer.find("Filter<S> {}").expect("supertrait bound"),
        consumer.find("Filter<S> for").expect("impl trait header"),
        consumer.rfind("Filter<S>").expect("where bound"),
    ];
    for start in expected {
        assert!(
            hits.iter().any(|hit| {
                hit.file == consumer_file
                    && hit.kind == UsageHitKind::Reference
                    && hit.start_offset == start
                    && hit.end_offset == start + "Filter".len()
            }),
            "wrapped trait inverse hit at {start}: {hits:#?}"
        );
    }
}
