//! Issue #1819: C and C++ macro targets need an inverse usage path.

use crate::common::InlineTestProject;
use brokk_bifrost::usages::{FuzzyResult, UsageFinder, UsageHitKind};
use brokk_bifrost::{CodeUnitIndex, CodeUnitType, CppAnalyzer, Language};

#[test]
fn cpp_macro_targets_find_function_like_and_object_like_invocations() {
    let macros = r#"#ifndef MACROS_H
#define MACROS_H
#define SELECT(value) (value)
#define ALIAS callback
#define OTHER(value) (value)
#endif
"#;
    let source = r#"#include "macros.h"

typedef void (*callback_t)(int);

void consume(callback_t callback) {
    SELECT(callback)(1); // function-like-positive
    ALIAS(2); // object-like-positive
    OTHER(callback)(3); // other-macro-near-miss
    struct { __uint(max_entries, SELECT); } map; // recovered-type-positive
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("macros.h", macros)
        .file("macros.c", source)
        .file("decoy.h", "#define SELECT(value) (value)\n")
        .file(
            "decoy.c",
            "#include \"decoy.h\"\nvoid decoy(int value) { SELECT(value); }\n",
        )
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let declarations = analyzer.get_all_declarations();
    let source_file = project.file("macros.c");
    let macro_file = project.file("macros.h");
    let decoy_file = project.file("decoy.c");

    for (name, markers) in [
        ("SELECT", &["SELECT(callback)(1)", "SELECT); } map"][..]),
        ("ALIAS", &["ALIAS(2)"][..]),
    ] {
        let target = declarations
            .iter()
            .find(|unit| {
                unit.kind() == CodeUnitType::Macro
                    && unit.identifier() == name
                    && unit.source() == &macro_file
            })
            .cloned()
            .unwrap_or_else(|| panic!("missing macro {name}: {declarations:#?}"));
        let result = UsageFinder::new().find_usages_default(&analyzer, &[target]);
        let hits = result.all_hits();
        for marker in markers {
            let expected = source.find(marker).expect("positive macro marker");
            assert!(
                hits.iter().any(|hit| {
                    hit.file == source_file
                        && hit.kind == UsageHitKind::Reference
                        && hit.start_offset == expected
                }),
                "missing {name} macro invocation at {marker}: {hits:#?}"
            );
        }
        let other = source
            .find("OTHER(callback)(3)")
            .expect("other macro marker");
        assert!(
            hits.iter()
                .all(|hit| hit.file != source_file || hit.start_offset != other),
            "the other macro must not match {name}: {hits:#?}"
        );
        assert!(
            hits.iter().all(|hit| hit.file != decoy_file),
            "the same-name macro from another header must not match {name}: {hits:#?}"
        );
    }
}

#[test]
fn cpp_macro_target_is_retained_as_unproven_after_unknown_include() {
    let header = "#ifndef ASSERT_H\n#define ASSERT_H\n#define ASSERT(value) (value)\n#endif\n";
    let source = "#include \"assert.h\"\n#undef ASSERT\n#include \"missing.h\"\nvoid check(int value) { ASSERT(value); }\n";
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("assert.h", header)
        .file("check.c", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.kind() == CodeUnitType::Macro && unit.identifier() == "ASSERT")
        .expect("ASSERT macro");

    let FuzzyResult::Success {
        hits_by_overload,
        unproven_by_overload,
        ..
    } = UsageFinder::new().find_usages_default(&analyzer, &[target])
    else {
        panic!("macro query must complete");
    };
    assert!(
        hits_by_overload.values().flatten().next().is_none(),
        "an unresolved include prevents a proven binding: {hits_by_overload:#?}"
    );
    let expected = source.find("ASSERT(value)").expect("macro call");
    assert!(
        unproven_by_overload
            .values()
            .flatten()
            .any(|hit| hit.start_offset == expected),
        "the unique visible macro must remain unproven: {unproven_by_overload:#?}"
    );
}

#[test]
fn conditional_macro_redefinitions_keep_the_indexed_target_unproven() {
    let header = r#"#ifndef PROBE_H
#define PROBE_H
#ifdef USE_FIRST
#include "probe_custom.h"
#else
#define PROBE(name, count, ...) CHECK##count(__VA_ARGS__)
#endif
#endif
"#;
    let custom = "#ifndef PROBE_CUSTOM_H\n#define PROBE_CUSTOM_H\n#define PROBE(name, count, ...) /* custom */\n#endif\n";
    let source = "#include \"probe.h\"\nvoid run(int value) { PROBE(event, 1, value); }\n";
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("probe.h", header)
        .file("probe_custom.h", custom)
        .file("run.c", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let targets = analyzer
        .get_all_declarations()
        .into_iter()
        .filter(|unit| unit.kind() == CodeUnitType::Macro && unit.identifier() == "PROBE")
        .collect::<Vec<_>>();
    assert_eq!(targets.len(), 2, "main and custom PROBE definitions");
    let expected = source.find("PROBE(event").expect("macro call");
    for target in targets {
        let FuzzyResult::Success {
            hits_by_overload,
            unproven_by_overload,
            ..
        } = UsageFinder::new().find_usages_default(&analyzer, &[target.clone()])
        else {
            panic!("macro query must complete");
        };
        assert!(
            hits_by_overload
                .values()
                .chain(unproven_by_overload.values())
                .flatten()
                .any(|hit| hit.start_offset == expected),
            "each possible conditional macro target must retain an unproven site: target={target:#?}, proven={hits_by_overload:#?}, unproven={unproven_by_overload:#?}"
        );
    }
}
