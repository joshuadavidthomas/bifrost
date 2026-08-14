//! Issue #2092: in a proven C source, `this` is an ordinary typed local
//! binding rather than the C++ implicit receiver keyword.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnit, CodeUnitIndex, CodeUnitType, CppAnalyzer, Language, ProjectFile};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

fn definition_at_offset(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    offset: usize,
) -> Value {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]}).to_string();
    call_tool(project, "get_definitions_by_location", &args)["results"][0].clone()
}

fn occurrence(source: &str, line: &str, token: &str) -> usize {
    source
        .find(line)
        .unwrap_or_else(|| panic!("missing fixture line {line:?}"))
        + line
            .find(token)
            .unwrap_or_else(|| panic!("missing token {token:?} in {line:?}"))
}

fn field(analyzer: &CppAnalyzer, owner: &str, name: &str) -> CodeUnit {
    analyzer
        .get_all_declarations()
        .into_iter()
        .find(|unit| {
            unit.kind() == CodeUnitType::Field
                && unit.identifier() == name
                && analyzer
                    .parent_of(unit)
                    .is_some_and(|parent| parent.identifier() == owner)
        })
        .unwrap_or_else(|| panic!("missing field {owner}.{name}"))
}

fn authoritative_ranges(
    analyzer: &CppAnalyzer,
    target: &CodeUnit,
    candidate: &ProjectFile,
) -> BTreeSet<(usize, usize)> {
    let provider =
        ExplicitCandidateProvider::new(Arc::new(std::iter::once(candidate.clone()).collect()));
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
        panic!("expected authoritative C usage result");
    };
    hits_by_overload
        .values()
        .flatten()
        .filter(|hit| &hit.file == candidate)
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect()
}

#[test]
fn c_this_uses_the_visible_typed_binding_on_every_forward_surface() {
    let source = r#"struct S { int field; };
struct Other { int field; };
void consume(void *value);

int read_field(struct S *this, struct Other *other) {
    consume(this); // direct-local
    int outer = this->field; // outer-s
    int unrelated = other->field; // unrelated-other
    {
        struct Other *this = other;
        unrelated += this->field; // inner-other
    }
    return outer + unrelated;
}

int broken(void) {
    return this->field; // no-c-binding
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("binding.c", source)
        .build();

    let outer = definition_at_offset(
        &project,
        "binding.c",
        source,
        occurrence(source, "    int outer = this->field; // outer-s", "field"),
    );
    assert_eq!(outer["status"], "resolved", "{outer:#}");
    assert_eq!(outer["definitions"][0]["fqn"], "S.field", "{outer:#}");

    let inner = definition_at_offset(
        &project,
        "binding.c",
        source,
        occurrence(
            source,
            "        unrelated += this->field; // inner-other",
            "field",
        ),
    );
    assert_eq!(inner["status"], "resolved", "{inner:#}");
    assert_eq!(inner["definitions"][0]["fqn"], "Other.field", "{inner:#}");

    let direct = definition_at_offset(
        &project,
        "binding.c",
        source,
        occurrence(source, "    consume(this); // direct-local", "this"),
    );
    assert_eq!(direct["status"], "resolved", "{direct:#}");
    assert_eq!(direct["definitions"][0]["name"], "this", "{direct:#}");
    assert_eq!(direct["definitions"][0]["kind"], "parameter", "{direct:#}");

    let missing = definition_at_offset(
        &project,
        "binding.c",
        source,
        occurrence(source, "    return this->field; // no-c-binding", "field"),
    );
    assert_eq!(missing["status"], "no_definition", "{missing:#}");
    assert!(missing["definitions"].as_array().is_none_or(Vec::is_empty));
}

#[test]
fn c_this_targeted_inverse_uses_the_parameter_owner_and_excludes_decoys() {
    let source = r#"struct S { int field; };
struct Other { int field; };

int read_field(struct S *this, struct Other *other) {
    int selected = this->field; // selected-s
    int decoy = other->field; // decoy-other
    return selected + decoy;
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("binding.c", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let file = project.file("binding.c");
    let selected_start = occurrence(
        source,
        "    int selected = this->field; // selected-s",
        "field",
    );
    let decoy_start = occurrence(
        source,
        "    int decoy = other->field; // decoy-other",
        "field",
    );

    let s_ranges = authoritative_ranges(&analyzer, &field(&analyzer, "S", "field"), &file);
    assert!(
        s_ranges.contains(&(selected_start, selected_start + "field".len())),
        "typed C `this` must attribute the field read to S: {s_ranges:?}"
    );
    assert!(
        !s_ranges.contains(&(decoy_start, decoy_start + "field".len())),
        "the unrelated receiver must not be attributed to S: {s_ranges:?}"
    );
}

#[test]
fn c_this_declared_by_an_active_function_macro_is_typed_after_the_invocation() {
    let source = r#"#include "object.h"
#include "unavailable-generated.h"
#include "repeat.h"

typedef struct Storage {
    int field;
} Storage;

void consume(void *value);

int read_field(void *thisVoid) {
    THIS(Storage);
    consume(this); // macro-local
    return this->field; // macro-field
}

#include "conflict.h"
int conflicted(void *thisVoid) {
    THIS(Storage);
    return this->field; // ambiguous-macro
}

#undef THIS
int unresolved(void *thisVoid) {
    THIS(Storage);
    return this->field; // inactive-macro
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "object.h",
            r#"#ifndef OBJECT_H
#define OBJECT_H
#define THIS(type) type *this = thisVoid
#endif
"#,
        )
        .file(
            "repeat.h",
            r#"#ifndef REPEAT_H
#define REPEAT_H
#include "object.h"
#endif
"#,
        )
        .file(
            "conflict.h",
            r#"#if MAYBE_REDEFINE_THIS
#define THIS(type) type *other = thisVoid
#endif
"#,
        )
        .file("macro_binding.c", source)
        .build();

    let member_start = occurrence(source, "    return this->field; // macro-field", "field");
    let member = definition_at_offset(&project, "macro_binding.c", source, member_start);
    assert_eq!(member["status"], "resolved", "{member:#}");
    assert_eq!(
        member["definitions"][0]["fqn"], "Storage.field",
        "{member:#}"
    );

    let direct = definition_at_offset(
        &project,
        "macro_binding.c",
        source,
        occurrence(source, "    consume(this); // macro-local", "this"),
    );
    assert_eq!(direct["status"], "no_definition", "{direct:#}");
    assert_eq!(
        direct["diagnostics"][0]["kind"], "local_variable_reference",
        "the unindexed macro declaration must still adjudicate the local use: {direct:#}"
    );

    let inactive_start = occurrence(source, "    return this->field; // inactive-macro", "field");
    let inactive = definition_at_offset(&project, "macro_binding.c", source, inactive_start);
    assert_eq!(inactive["status"], "no_definition", "{inactive:#}");
    assert!(
        inactive["definitions"].as_array().is_none_or(Vec::is_empty),
        "an invocation after #undef must not create a binding: {inactive:#}"
    );

    let ambiguous_start = occurrence(
        source,
        "    return this->field; // ambiguous-macro",
        "field",
    );
    let ambiguous = definition_at_offset(&project, "macro_binding.c", source, ambiguous_start);
    assert_eq!(ambiguous["status"], "no_definition", "{ambiguous:#}");
    assert!(
        ambiguous["definitions"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "a conflicting conditional definition must fail closed: {ambiguous:#}"
    );

    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let file = project.file("macro_binding.c");
    let ranges = authoritative_ranges(&analyzer, &field(&analyzer, "Storage", "field"), &file);
    assert!(
        ranges.contains(&(member_start, member_start + "field".len())),
        "the targeted graph must use the same macro-local owner: {ranges:?}"
    );
    assert!(
        !ranges.contains(&(inactive_start, inactive_start + "field".len())),
        "an inactive declaration macro must not create a targeted hit: {ranges:?}"
    );
    assert!(
        !ranges.contains(&(ambiguous_start, ambiguous_start + "field".len())),
        "a conflicting declaration macro must not create a targeted hit: {ranges:?}"
    );
}

#[test]
fn cpp_this_keeps_implicit_receiver_semantics() {
    let source = r#"struct Widget {
    int field;
    int read() { return this->field; }
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("widget.cpp", source)
        .build();
    let result = definition_at_offset(
        &project,
        "widget.cpp",
        source,
        occurrence(source, "    int read() { return this->field; }", "field"),
    );
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(
        result["definitions"][0]["fqn"], "Widget.field",
        "{result:#}"
    );
}
