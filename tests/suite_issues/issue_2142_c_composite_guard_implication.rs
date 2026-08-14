//! Issue #2142: composite C preprocessor guards use Boolean equivalence and
//! implication rather than opaque expression-string equality.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn c_composite_guards_prove_equivalence_and_implication() {
    let source = r#"#if !defined(WIN32) || defined(CYGWIN)
static int equivalent_helper(void) { return 1; }
#endif

int equivalent_caller(void) {
#if defined(WIN32) && !defined(CYGWIN)
    return 0;
#else
    return equivalent_helper();
#endif
}

#if !defined(A) || !defined(B) || !defined(C)
static int implied_helper(void) { return 1; }
#endif

int implied_caller(void) {
#if defined(A) && defined(B)
    return 0;
#else
    return implied_helper(); /* implied */
#endif
}

#if defined(ONLY)
static int guarded_helper(void) { return 1; }
#endif

#if !defined(ONLY)
int contradictory_caller(void) { return guarded_helper(); }
#endif

#if defined(UNRELATED)
int unrelated_caller(void) { return implied_helper(); /* unrelated */ }
#endif
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("guards.c", source)
        .build();

    let equivalent = definition_at(&project, "guards.c", source, "equivalent_helper();");
    assert_eq!(equivalent["status"], "resolved", "{equivalent:#}");

    let implied = definition_at(
        &project,
        "guards.c",
        source,
        "implied_helper(); /* implied */",
    );
    assert_eq!(implied["status"], "resolved", "{implied:#}");

    let contradictory = definition_at(&project, "guards.c", source, "guarded_helper();");
    assert_eq!(
        contradictory["status"], "no_definition",
        "{contradictory:#}"
    );

    let unrelated = definition_at(
        &project,
        "guards.c",
        source,
        "implied_helper(); /* unrelated */",
    );
    assert_eq!(unrelated["status"], "no_definition", "{unrelated:#}");
}
