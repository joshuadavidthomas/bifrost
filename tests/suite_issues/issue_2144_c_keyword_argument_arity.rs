//! Issue #2144: a legal C identifier parsed as a C++ keyword remains one call
//! argument when its enclosing parameter binding proves the recovery.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn c_keyword_parameter_use_preserves_call_arity() {
    let c_source = r#"static int c_helper(const char *tmpdir, wchar_t *value) { return 0; }

int ordinary(const char *tmpdir) {
    return c_helper(tmpdir, NULL); /* ordinary */
}

int recovered(wchar_t *template) {
    return c_helper(NULL, template); /* recovered */
}

int unbound(void) {
    return c_helper(NULL, template); /* unbound */
}
"#;
    let cpp_source = r#"static int cpp_helper(const char *tmpdir, wchar_t *value) { return 0; }

int cpp_near_miss(wchar_t *template) {
    return cpp_helper(NULL, template); /* cpp-near-miss */
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("keyword.c", c_source)
        .file("keyword.cpp", cpp_source)
        .build();

    let ordinary = definition_at(
        &project,
        "keyword.c",
        c_source,
        "c_helper(tmpdir, NULL); /* ordinary */",
    );
    assert_eq!(ordinary["status"], "resolved", "{ordinary:#}");

    let recovered = definition_at(
        &project,
        "keyword.c",
        c_source,
        "c_helper(NULL, template); /* recovered */",
    );
    assert_eq!(recovered["status"], "resolved", "{recovered:#}");

    let unbound = definition_at(
        &project,
        "keyword.c",
        c_source,
        "c_helper(NULL, template); /* unbound */",
    );
    assert_eq!(unbound["status"], "no_definition", "{unbound:#}");

    let cpp_near_miss = definition_at(
        &project,
        "keyword.cpp",
        cpp_source,
        "cpp_helper(NULL, template); /* cpp-near-miss */",
    );
    assert_eq!(
        cpp_near_miss["status"], "no_definition",
        "{cpp_near_miss:#}"
    );
}
