//! Issue #1838: qualified C++ callables used as values need inverse attribution.

use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::Language;

#[test]
fn cpp_qualified_callable_values_record_member_and_free_function_targets() {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "callables.cpp",
            r#"namespace work {
void run() {}
}

class Config {
public:
    static void configure() {}
};

void bind_member() { auto callback = Config::configure; }
void bind_free() { auto callback = work::run; }
"#,
        )
        .build();
    let value = usage_graph_at(project.root(), "{}");

    assert!(
        has_edge(&value, "bind_member", "Config.configure"),
        "the qualified member value must record its callable: {}",
        value["edges"]
    );
    assert!(
        has_edge(&value, "bind_free", "work.run"),
        "the qualified free-function value must record its callable: {}",
        value["edges"]
    );
}
