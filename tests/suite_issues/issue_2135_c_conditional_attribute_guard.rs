//! Issue #2135: a conditional declaration attribute must not guard the
//! function itself or every later declaration in a C file.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn c_conditional_attribute_ends_before_variadic_function_declarator() {
    let source = "#ifndef _MSC_VER\n__attribute__((format(printf, 1, 2)))\n#endif\nstatic void die(const char *message, ...) {}\nvoid caller(void) { die(\"one\"); die(\"%s\", \"two\"); }\n";
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("attribute.c", source)
        .build();
    let one = definition_at(&project, "attribute.c", source, "die(\"one\")");
    assert_eq!(one["status"], "resolved", "{one:#}");
    let two = definition_at(&project, "attribute.c", source, "die(\"%s\", \"two\")");
    assert_eq!(two["status"], "resolved", "{two:#}");
}
