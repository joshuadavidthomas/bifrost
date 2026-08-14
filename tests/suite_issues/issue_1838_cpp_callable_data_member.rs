//! Issue #1838: C++ calls through data members must navigate to the member.

use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::json;

#[test]
fn cpp_callable_data_member_calls_resolve_the_field_declaration() {
    let source = r#"struct Equal {
    bool operator()(int left, int right) const;
};

struct Table {
    Equal equal;
    int (*callback)(int);

    bool contains(int value) { return equal(value, value); }
    int invoke(int value) { return callback(value); }
    int explicit_invoke(int value) { return this->callback(value); }
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("table.cpp", source)
        .build();
    let location = |needle: &str, offset: usize| {
        let start = source.find(needle).expect("reference") + offset;
        let line = source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column = source[..start]
            .rsplit_once('\n')
            .map_or(&source[..start], |(_, current)| current)
            .chars()
            .count()
            + 1;
        json!({"path": "table.cpp", "line": line, "column": column})
    };
    let value = call_search_tool_json(
        project.root(),
        "get_definitions_by_location",
        &json!({
            "references": [
                location("return equal", "return ".len()),
                location("return callback", "return ".len()),
                location("return this->callback", "return this->".len())
            ]
        })
        .to_string(),
    );

    let results = value["results"].as_array().expect("definition results");
    for (result, expected) in
        results
            .iter()
            .zip(["Table.equal", "Table.callback", "Table.callback"])
    {
        assert_eq!(result["status"], "resolved", "{value}");
        assert_eq!(result["definitions"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["definitions"][0]["fqn"], expected, "{value}");
        assert_eq!(result["definitions"][0]["kind"], "field", "{value}");
    }
}
