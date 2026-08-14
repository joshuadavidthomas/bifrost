//! Issue #2134: a preprocessor family selecting the middle of a C typedef
//! must not invent a guard around every later declaration in the file.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn c_split_typedef_guard_ends_before_later_functions() {
    let source = r#"typedef
#ifdef WIDE_NODE
struct Node *
#else
UInt32
#endif
NodeRef;

static int local_helper(int value) {
    return value + 1;
}

int selected(void) {
    return local_helper(3); // selected-local
}

#ifdef OPTIONAL_HELPER
static int guarded_only(int value) {
    return value + 2;
}
#endif

int outside_guard(void) {
    return guarded_only(3); // guarded-near-miss
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("split.c", source)
        .build();

    let selected = definition_at(
        &project,
        "split.c",
        source,
        "local_helper(3); // selected-local",
    );
    assert_eq!(selected["status"], "resolved", "{selected:#}");
    assert_eq!(
        selected["definitions"][0]["fqn"], "local_helper",
        "{selected:#}"
    );

    let guarded = definition_at(
        &project,
        "split.c",
        source,
        "guarded_only(3); // guarded-near-miss",
    );
    assert_eq!(guarded["status"], "no_definition", "{guarded:#}");
}
