//! Issue #1838: C++ operator member pointers must retain the operator terminal.

use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::Language;

#[test]
fn cpp_operator_member_pointer_records_the_operator_reference() {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "bitmap.cpp",
            r#"class Bitmap {
public:
    int operator&(const Bitmap& other) const { return 0; }
    int plain(const Bitmap& other) const { return 0; }
};

using Operation = int (Bitmap::*)(const Bitmap&) const;

Operation bind_operator() { return &Bitmap::operator&; }
Operation bind_plain() { return &Bitmap::plain; }
"#,
        )
        .build();
    let value = usage_graph_at(project.root(), "{}");

    assert!(
        has_edge(&value, "bind_operator", "Bitmap.operator&"),
        "the operator member pointer must record its terminal: {}",
        value["edges"]
    );
    assert!(
        has_edge(&value, "bind_plain", "Bitmap.plain"),
        "the ordinary member-pointer control must remain resolved: {}",
        value["edges"]
    );
}
