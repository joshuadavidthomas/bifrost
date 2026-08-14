//! Issue #2047: plain annotation names and static receiver qualifiers use the
//! same lexical nested-type lookup as ordinary Java type positions.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool};
use brokk_bifrost::Language;
use serde_json::{Value, json};

fn definition_after(
    project: &BuiltInlineTestProject,
    path: &str,
    source: &str,
    anchor: &str,
    needle: &str,
) -> Value {
    let anchor_start = source.find(anchor).expect("anchor");
    let start = anchor_start
        + source[anchor_start..]
            .find(needle)
            .expect("needle after anchor");
    let prefix = &source[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    let args = json!({"references": [{"path": path, "line": line, "column": column}]});
    call_tool(project, "get_definitions_by_location", &args.to_string())["results"][0].clone()
}

fn assert_resolved(result: &Value, fqn: &str) {
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(result["definitions"][0]["fqn"], fqn, "{result:#}");
}

#[test]
fn lexical_nested_annotations_and_static_receivers_beat_imported_names() {
    let outer = r#"package p;

import q.Factory;
import q.Marker;

class Outer {
    @Marker int tagged;
    static int value = Factory.create();

    class Inner {
        @Marker int nested;
        int other = Factory.create();
    }

    @interface Marker {}
    static class Factory {
        static int create() { return 1; }
    }
}

class Outside {
    @Marker int tagged;
    int value = Factory.create();
}
"#;
    let imported_factory = r#"package q;
public class Factory {
    public static int create() { return 2; }
}
"#;
    let imported_marker = "package q;\npublic @interface Marker {}\n";
    let project = InlineTestProject::with_language(Language::Java)
        .file("src/p/Outer.java", outer)
        .file("src/q/Factory.java", imported_factory)
        .file("src/q/Marker.java", imported_marker)
        .build();
    let path = "src/p/Outer.java";

    assert_resolved(
        &definition_after(&project, path, outer, "@Marker int tagged", "Marker"),
        "p.Outer.Marker",
    );
    assert_resolved(
        &definition_after(
            &project,
            path,
            outer,
            "static int value = Factory",
            "Factory",
        ),
        "p.Outer.Factory",
    );
    assert_resolved(
        &definition_after(
            &project,
            path,
            outer,
            "static int value = Factory.create",
            "create",
        ),
        "p.Outer.Factory.create",
    );

    assert_resolved(
        &definition_after(&project, path, outer, "@Marker int nested", "Marker"),
        "p.Outer.Marker",
    );
    assert_resolved(
        &definition_after(&project, path, outer, "int other = Factory", "Factory"),
        "p.Outer.Factory",
    );
    assert_resolved(
        &definition_after(
            &project,
            path,
            outer,
            "int other = Factory.create",
            "create",
        ),
        "p.Outer.Factory.create",
    );

    assert_resolved(
        &definition_after(
            &project,
            path,
            outer,
            "class Outside {\n    @Marker",
            "Marker",
        ),
        "q.Marker",
    );
    assert_resolved(
        &definition_after(
            &project,
            path,
            outer,
            "class Outside {\n    @Marker int tagged;\n    int value = Factory",
            "Factory",
        ),
        "q.Factory",
    );
}
