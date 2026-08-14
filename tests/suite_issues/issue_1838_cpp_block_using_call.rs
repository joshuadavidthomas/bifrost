//! Issue #1838: a block using-declaration must precede class-member lookup.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn cpp_block_using_to_unindexed_callable_reports_a_boundary() {
    let source = r#"class Expected {
public:
    void swap(Expected& other) {
        using std::swap;
        swap(error(), other.error());
    }
    int& error();
};

void swap(Expected&, Expected&);
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("expected.hpp", source)
        .build();
    let result = definition_at(
        &project,
        "expected.hpp",
        source,
        "swap(error(), other.error())",
    );

    assert_eq!(
        result["status"], "unresolvable_import_boundary",
        "the unindexed std::swap import must preempt the one-argument member: {result:#}"
    );
}

#[test]
fn cpp_block_using_to_indexed_callable_resolves_before_the_member() {
    let source = r#"namespace library {
void swap(int&, int&) {}
}

class Expected {
public:
    void swap(Expected& other) {
        using library::swap;
        swap(error(), other.error());
    }
    int& error();
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("expected.hpp", source)
        .build();
    let result = definition_at(
        &project,
        "expected.hpp",
        source,
        "swap(error(), other.error())",
    );

    assert_eq!(
        result["status"], "resolved",
        "the block import must preempt the same-named member: {result:#}"
    );
    assert_eq!(result["definitions"][0]["fqn"], "library.swap");
}

#[test]
fn cpp_member_lookup_stays_authoritative_without_a_block_using() {
    let source = r#"class Expected {
public:
    void swap(Expected& other) {
        swap(error(), other.error());
    }
    int& error();
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("expected.hpp", source)
        .build();
    let result = definition_at(
        &project,
        "expected.hpp",
        source,
        "swap(error(), other.error())",
    );

    assert_eq!(
        result["diagnostics"][0]["kind"], "no_applicable_overload",
        "the ordinary member-only call must keep its overload verdict: {result:#}"
    );
}

#[test]
fn cpp_block_using_does_not_apply_before_its_declaration() {
    let source = r#"class Expected {
public:
    void swap(Expected& other) {
        swap(error(), other.error());
        using std::swap;
    }
    int& error();
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("expected.hpp", source)
        .build();
    let result = definition_at(
        &project,
        "expected.hpp",
        source,
        "swap(error(), other.error())",
    );

    assert_eq!(
        result["diagnostics"][0]["kind"], "no_applicable_overload",
        "a later block import must not change an earlier call: {result:#}"
    );
}
