//! Issue #2139: calls inside an `else if` fragment selected by the
//! preprocessor retain that physical guard even when parser recovery moves the
//! call outside the conditional node.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn c_fragmented_statement_reference_retains_its_preprocessor_guard() {
    let source = r#"#if HAVE_ONE && HAVE_TWO
static int helper(int value) { return value; }
#endif

int fragmented(int value) {
    if (value == 0) {
        return 0;
#if HAVE_ONE && HAVE_TWO
    } else if (value == 1) {
        return helper(value); /* fragmented */
#endif
    }
    return 0;
}

int unguarded(int value) {
    return helper(value); /* unguarded */
}

#if HAVE_ONE && HAVE_TWO
int enabled(void) { return 0; }
#else
int contradictory(int value) {
    return helper(value); /* contradictory */
}
#endif
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("fragmented.c", source)
        .build();

    let fragmented = definition_at(
        &project,
        "fragmented.c",
        source,
        "helper(value); /* fragmented */",
    );
    assert_eq!(fragmented["status"], "resolved", "{fragmented:#}");

    let unguarded = definition_at(
        &project,
        "fragmented.c",
        source,
        "helper(value); /* unguarded */",
    );
    assert_eq!(unguarded["status"], "no_definition", "{unguarded:#}");

    let contradictory = definition_at(
        &project,
        "fragmented.c",
        source,
        "helper(value); /* contradictory */",
    );
    assert_eq!(
        contradictory["status"], "no_definition",
        "{contradictory:#}"
    );
}
