use crate::common::{InlineTestProject, call_search_tool_json};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn lookup(root: &std::path::Path, source: &str, needle: &str) -> Value {
    let start = source.find(needle).expect("reference marker");
    let line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let line_start = source[..start].rfind('\n').map_or(0, |offset| offset + 1);
    let column = source[line_start..start].chars().count() + 1;
    call_search_tool_json(
        root,
        "get_definitions_by_location",
        &json!({"references": [{"path": "src/Both.php", "line": line, "column": column}]})
            .to_string(),
    )
}

#[test]
fn php_property_and_method_with_the_same_name_use_separate_namespaces() {
    let source = r#"<?php
namespace App;

final class Both {
    public string $value;
    public const string LABEL = 'constant';

    public function value(): string {
        return 'method';
    }

    public static function LABEL(): string {
        return 'method';
    }

    public function read(): string {
        return $this->value; // property-reference
    }

    public function call(): string {
        return $this->value(); // method-reference
    }

    public function constant(): string {
        return self::LABEL; // constant-reference
    }

    public function staticCall(): string {
        return self::LABEL(); // static-method-reference
    }
}
"#;
    let project = InlineTestProject::with_language(Language::Php)
        .file("src/Both.php", source)
        .build();

    let property = lookup(project.root(), source, "value; // property-reference");
    let property_result = &property["results"][0];
    assert_eq!(property_result["status"], "resolved", "{property}");
    assert_eq!(
        property_result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{property}"
    );
    assert_eq!(
        property_result["definitions"][0]["kind"], "field",
        "{property}"
    );

    let method = lookup(project.root(), source, "value(); // method-reference");
    let method_result = &method["results"][0];
    assert_eq!(method_result["status"], "resolved", "{method}");
    assert_eq!(
        method_result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{method}"
    );
    assert_eq!(
        method_result["definitions"][0]["kind"], "function",
        "{method}"
    );

    let constant = lookup(project.root(), source, "LABEL; // constant-reference");
    let constant_result = &constant["results"][0];
    assert_eq!(constant_result["status"], "resolved", "{constant}");
    assert_eq!(
        constant_result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{constant}"
    );
    assert_eq!(
        constant_result["definitions"][0]["kind"], "field",
        "{constant}"
    );

    let static_method = lookup(
        project.root(),
        source,
        "LABEL(); // static-method-reference",
    );
    let static_method_result = &static_method["results"][0];
    assert_eq!(
        static_method_result["status"], "resolved",
        "{static_method}"
    );
    assert_eq!(
        static_method_result["definitions"].as_array().map(Vec::len),
        Some(1),
        "{static_method}"
    );
    assert_eq!(
        static_method_result["definitions"][0]["kind"], "function",
        "{static_method}"
    );
}
