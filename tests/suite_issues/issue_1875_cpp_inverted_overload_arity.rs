//! Issue #1875: inherited bare calls must reject visible members whose arity
//! does not match before the inverted usage graph records an edge.

use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::Language;

#[test]
fn cpp_inverted_bare_member_calls_filter_by_arity() {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "overloads.h",
            r#"struct Base {
    void pick(int value);
};

struct Leaf : Base {
    void call_valid();
    void call_invalid();
};
"#,
        )
        .file(
            "overloads.cpp",
            r#"#include "overloads.h"
void Base::pick(int value) {}
void Leaf::call_valid() { pick(1); }
void Leaf::call_invalid() { pick(1, 2); }
"#,
        )
        .build();
    let graph = usage_graph_at(project.root(), "{}");

    assert!(
        has_edge(&graph, "Leaf.call_valid", "Base.pick"),
        "the applicable inherited call must remain recorded: {}",
        graph["edges"]
    );
    assert!(
        !has_edge(&graph, "Leaf.call_invalid", "Base.pick"),
        "an inapplicable inherited overload must not produce a false edge: {}",
        graph["edges"]
    );
}
