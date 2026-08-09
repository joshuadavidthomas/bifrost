use crate::common::InlineTestProject;
use brokk_bifrost::usages::{UsageFinder, UsageHitKind};
use brokk_bifrost::{Language, RustAnalyzer};

fn rust_analyzer_with_source(
    source: &str,
) -> (crate::common::BuiltInlineTestProject, RustAnalyzer) {
    let project = InlineTestProject::with_language(Language::Rust)
        .file("src/lib.rs", source)
        .build();
    let analyzer = RustAnalyzer::from_project(project.project().clone());
    (project, analyzer)
}

#[test]
fn rust_1280_bare_enum_variant_values_require_exact_import_identity() {
    let source = r#"
enum CasingStyle {
    ScreamingSnake,
}

enum Decoy {
    ScreamingSnake,
}

enum Left {
    ScreamingSnake,
}

enum Right {
    ScreamingSnake,
}

impl CasingStyle {
    fn grouped_value() {
        use self::CasingStyle::{ScreamingSnake};
        let _ = ScreamingSnake; // positive
    }

    fn aliased_value() {
        use self::CasingStyle::ScreamingSnake as Alias;
        let _ = Alias; // alias-positive
    }

    fn unit_pattern() {
        use self::CasingStyle::{ScreamingSnake};
        match 0 {
            ScreamingSnake => (), // unit-pattern
            _ => (),
        }
    }

    fn aliased_unit_pattern() {
        use self::CasingStyle::ScreamingSnake as Alias;
        match 0 {
            Alias => (), // aliased-unit-pattern
            _ => (),
        }
    }

    fn local_binding_shadow() {
        use self::CasingStyle::{ScreamingSnake};
        let ScreamingSnake = 1;
        let _ = ScreamingSnake; // local-shadow
    }

    fn decoy_variant_import() {
        use self::Decoy::{ScreamingSnake};
        let _ = ScreamingSnake; // decoy-import
    }

    fn decoy_aliased_variant_import() {
        use self::Decoy::ScreamingSnake as Alias;
        let _ = Alias; // decoy-alias
    }

    fn local_unit_pattern_binding() {
        match 0 {
            ScreamingSnake => (), // local-pattern-binding
            _ => (),
        }
    }

    fn decoy_unit_pattern() {
        use self::Decoy::{ScreamingSnake};
        match 0 {
            ScreamingSnake => (), // decoy-pattern
            _ => (),
        }
    }

    fn ambiguous_variant_globs() {
        use self::Left::*;
        use self::Right::*;
        let _ = ScreamingSnake; // ambiguous-glob
    }

    fn qualified_decoy() {
        let _ = Decoy::ScreamingSnake; // qualified-decoy
    }

    fn generic_type_shadow<ScreamingSnake>() {
        use self::CasingStyle::{ScreamingSnake};
        let _: ScreamingSnake = todo!(); // generic-type
    }
}
"#;
    let (project, analyzer) = rust_analyzer_with_source(source);
    let file = project.file("src/lib.rs");
    let target = analyzer
        .exact_member(&file, "CasingStyle", "ScreamingSnake", false)
        .expect("CasingStyle::ScreamingSnake target");

    let hits = UsageFinder::new()
        .find_usages_default(&analyzer, std::slice::from_ref(&target))
        .all_hits();
    let reference_offsets = hits
        .iter()
        .filter(|hit| hit.file == file && hit.kind == UsageHitKind::Reference)
        .map(|hit| hit.start_offset)
        .collect::<Vec<_>>();
    let positives = [
        "let _ = ScreamingSnake; // positive",
        "let _ = Alias; // alias-positive",
        "ScreamingSnake => (), // unit-pattern",
        "Alias => (), // aliased-unit-pattern",
    ]
    .into_iter()
    .map(|marker| {
        source.find(marker).expect("positive bare variant value")
            + marker
                .find(if marker.contains("Alias") {
                    "Alias"
                } else {
                    "ScreamingSnake"
                })
                .expect("positive variant name")
    })
    .collect::<Vec<_>>();
    assert_eq!(reference_offsets, positives, "hits={hits:#?}");

    for marker in [
        "let _ = ScreamingSnake; // local-shadow",
        "let ScreamingSnake = 1;",
        "let _ = ScreamingSnake; // decoy-import",
        "let _ = Alias; // decoy-alias",
        "let _ = ScreamingSnake; // ambiguous-glob",
        "Decoy::ScreamingSnake; // qualified-decoy",
        "let _: ScreamingSnake = todo!(); // generic-type",
        "ScreamingSnake => (), // local-pattern-binding",
        "ScreamingSnake => (), // decoy-pattern",
    ] {
        let marker_start = source.find(marker).expect("near-miss marker");
        let needle = if marker.contains("Alias") {
            "Alias"
        } else {
            "ScreamingSnake"
        };
        let offset = marker_start + marker.find(needle).expect("near-miss variant");
        assert!(
            hits.iter().all(|hit| hit.start_offset != offset),
            "near miss {marker:?} must not hit target: {hits:#?}"
        );
    }
}
