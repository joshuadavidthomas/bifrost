//! Issue #2138: a source-reachable inferred include root disambiguates a
//! project header from the same suffix beneath an unrelated nested tree.

use crate::common::{InlineTestProject, definition_at, definition_paths};
use brokk_bifrost::Language;

#[test]
fn c_include_prefers_the_unique_source_reachable_inferred_root() {
    let source = "#include \"config/parse.h\"\n\nint before(void) { return target(1); }\nint target(int value) { return value; }\n";
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("src/config/parse.c", source)
        .file("src/config/parse.h", "int target(int value);\n")
        .file(
            "src/build/config/parse.h",
            "int target(const char *value);\n",
        )
        .build();

    let result = definition_at(&project, "src/config/parse.c", source, "target(1)");
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(definition_paths(&result), vec!["src/config/parse.c"]);
}
