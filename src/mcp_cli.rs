use crate::mcp_common::tool_descriptor;
use serde_json::{Value, json};

pub(crate) fn cli_tool_descriptors() -> Vec<Value> {
    vec![
        tool_descriptor(
            "contains_tests",
            "Return whether each requested workspace file semantically contains test code according to Bifrost's language analyzer. This is path-independent and does not identify the full test surface; use classify_test_files for fixtures, helpers, and hermetic test-surface identification.",
            json!({
                "type": "object",
                "properties": {
                    "file_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Project-relative paths of files to check."
                    }
                },
                "required": ["file_paths"]
            }),
        ),
        tool_descriptor(
            "classify_test_files",
            "Classify each workspace file as test, test_support, production, or ambiguous for test-surface identification. Combines path conventions with semantic test detection; ambiguous means path conventions were inconclusive - consult contains_test_code. Use contains_tests for the purely semantic predicate.",
            json!({
                "type": "object",
                "properties": {
                    "file_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Project-relative paths of files to classify."
                    }
                },
                "required": ["file_paths"]
            }),
        ),
    ]
}
