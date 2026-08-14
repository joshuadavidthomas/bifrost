//! Issue #2093: ordinary C expression tokens use the exact active macro.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, CodeUnitType, CppAnalyzer, Language, ProjectFile};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

fn occurrence(source: &str, marker: &str, token: &str) -> usize {
    source
        .find(marker)
        .unwrap_or_else(|| panic!("missing marker {marker:?}"))
        + marker
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} in {marker:?}"))
}

fn definition_at(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    offset: usize,
) -> Value {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, line)| line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn macro_named(analyzer: &CppAnalyzer, name: &str) -> CodeUnit {
    analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| unit.kind() == CodeUnitType::Macro && unit.identifier() == name)
        .unwrap_or_else(|| panic!("missing macro {name}"))
}

fn authoritative_ranges(
    analyzer: &CppAnalyzer,
    target: &CodeUnit,
    file: &ProjectFile,
) -> BTreeSet<(usize, usize)> {
    let provider =
        ExplicitCandidateProvider::new(Arc::new(std::iter::once(file.clone()).collect()));
    let FuzzyResult::Success {
        hits_by_overload, ..
    } = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            analyzer,
            std::slice::from_ref(target),
            Some(&provider),
            1,
            1000,
        )
        .result
    else {
        panic!("expected authoritative macro usage result");
    };
    hits_by_overload
        .values()
        .flatten()
        .filter(|hit| &hit.file == file)
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect()
}

#[test]
fn active_object_macros_resolve_in_ordinary_expression_positions() {
    let source = r#"int before(void) { return LATE; }
#define LATE 3
#define ACTIVE 7
#define FIELD_NAME field
#define CALL(value) (value)
#define PARAM(PARAM) (PARAM)
#define LABEL next

struct Holder { int field; };
void consume(int value);

int use(struct Holder *holder) {
    int array[ACTIVE]; // array-bound
    int initialized = ACTIVE; // initializer
    int binary = ACTIVE + 1; // binary
    consume(ACTIVE); // argument
    int casted = (int)ACTIVE; // cast
    int selected = holder->FIELD_NAME; // member-selector
    if (binary) goto LABEL;
LABEL:
    return ACTIVE + array[0] + initialized + casted + selected; // return
}

int called(void) { return CALL(1); }

#if UNKNOWN_FEATURE
#define CONDITIONAL 1
#else
#define CONDITIONAL 2
#endif
int conditional(void) { return CONDITIONAL; }

#undef ACTIVE
int after(int ACTIVE) { return ACTIVE; }
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("ordinary.c", source)
        .build();

    for marker in [
        "    int array[ACTIVE]; // array-bound",
        "    int initialized = ACTIVE; // initializer",
        "    int binary = ACTIVE + 1; // binary",
        "    consume(ACTIVE); // argument",
        "    int casted = (int)ACTIVE; // cast",
        "    return ACTIVE + array[0] + initialized + casted + selected; // return",
    ] {
        let result = definition_at(
            &project,
            "ordinary.c",
            source,
            occurrence(source, marker, "ACTIVE"),
        );
        assert_eq!(result["status"], "resolved", "{marker}: {result:#}");
        assert_eq!(result["definitions"][0]["kind"], "macro", "{result:#}");
        assert_eq!(result["definitions"][0]["name"], "ACTIVE", "{result:#}");
    }

    let selector = definition_at(
        &project,
        "ordinary.c",
        source,
        occurrence(
            source,
            "    int selected = holder->FIELD_NAME; // member-selector",
            "FIELD_NAME",
        ),
    );
    assert_eq!(selector["status"], "resolved", "{selector:#}");
    assert_eq!(selector["definitions"][0]["kind"], "macro", "{selector:#}");
    assert_eq!(
        selector["definitions"][0]["name"], "FIELD_NAME",
        "{selector:#}"
    );

    let call = definition_at(
        &project,
        "ordinary.c",
        source,
        occurrence(source, "int called(void) { return CALL(1); }", "CALL"),
    );
    assert_eq!(call["status"], "resolved", "{call:#}");
    assert_eq!(call["definitions"][0]["kind"], "macro", "{call:#}");

    for (marker, token) in [
        ("int before(void) { return LATE; }", "LATE"),
        (
            "int conditional(void) { return CONDITIONAL; }",
            "CONDITIONAL",
        ),
        ("    if (binary) goto LABEL;", "LABEL"),
        ("LABEL:", "LABEL"),
    ] {
        let result = definition_at(
            &project,
            "ordinary.c",
            source,
            occurrence(source, marker, token),
        );
        assert_ne!(
            result["definitions"]
                .get(0)
                .and_then(|definition| definition["kind"].as_str()),
            Some("macro"),
            "non-reference or inactive {token} must not resolve as a macro: {result:#}"
        );
    }

    let after = definition_at(
        &project,
        "ordinary.c",
        source,
        occurrence(
            source,
            "int after(int ACTIVE) { return ACTIVE; }",
            "return ACTIVE",
        ) + "return ".len(),
    );
    assert_ne!(after["definitions"][0]["kind"], "macro", "{after:#}");

    let formal =
        occurrence(source, "#define PARAM(PARAM) (PARAM)", "PARAM(PARAM)") + "PARAM(".len();
    let formal_result = definition_at(&project, "ordinary.c", source, formal);
    assert_ne!(
        formal_result["definitions"]
            .get(0)
            .and_then(|definition| definition["kind"].as_str()),
        Some("macro"),
        "macro formal parameters are binders: {formal_result:#}"
    );
    let definition_name = occurrence(source, "#define ACTIVE 7", "ACTIVE");
    let definition_result = definition_at(&project, "ordinary.c", source, definition_name);
    assert_ne!(
        definition_result["definitions"]
            .get(0)
            .and_then(|definition| definition["kind"].as_str()),
        Some("macro"),
        "the macro definition name is not a reference to itself: {definition_result:#}"
    );

    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let file = project.file("ordinary.c");
    let active_ranges = authoritative_ranges(&analyzer, &macro_named(&analyzer, "ACTIVE"), &file);
    for marker in [
        "    int array[ACTIVE]; // array-bound",
        "    int initialized = ACTIVE; // initializer",
        "    int binary = ACTIVE + 1; // binary",
        "    consume(ACTIVE); // argument",
        "    int casted = (int)ACTIVE; // cast",
        "    return ACTIVE + array[0] + initialized + casted + selected; // return",
    ] {
        let start = occurrence(source, marker, "ACTIVE");
        assert!(
            active_ranges.contains(&(start, start + "ACTIVE".len())),
            "targeted inverse omitted {marker}: {active_ranges:?}"
        );
    }
    let field_name_ranges =
        authoritative_ranges(&analyzer, &macro_named(&analyzer, "FIELD_NAME"), &file);
    let selector_start = occurrence(
        source,
        "    int selected = holder->FIELD_NAME; // member-selector",
        "FIELD_NAME",
    );
    assert!(
        field_name_ranges.contains(&(selector_start, selector_start + "FIELD_NAME".len())),
        "targeted inverse omitted the macro-produced member selector: {field_name_ranges:?}"
    );
    let after_start = occurrence(
        source,
        "int after(int ACTIVE) { return ACTIVE; }",
        "return ACTIVE",
    ) + "return ".len();
    assert!(
        !active_ranges.contains(&(after_start, after_start + "ACTIVE".len())),
        "the post-undef local must not be a macro usage: {active_ranges:?}"
    );
}
