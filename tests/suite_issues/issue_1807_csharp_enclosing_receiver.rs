//! Issue #1807: a bare factory receiver must search each enclosing C# type in
//! lexical order without merging their inheritance walks.

use crate::common::InlineTestProject;
use crate::common::search_tools::definition_at;
use brokk_bifrost::Language;

#[test]
fn csharp_bare_factory_receiver_walks_enclosing_types_in_order() {
    let source = r#"namespace Demo;

class OuterProduct {
    public int Value { get; }
}

class NearProduct {
    public string Value { get; } = "near";
}

class Outer {
    private static OuterProduct Create() => new();

    class Middle {
        class InnerWithoutShadow {
            int Read() => Create().Value;
        }

        class NearBase {
            protected static NearProduct Create() => new();
        }

        class InnerWithShadow : NearBase {
            string Read() => Create().Value;
        }
    }
}
"#;
    let project = InlineTestProject::with_language(Language::CSharp)
        .file("Nested.cs", source)
        .build();

    let outer = definition_at(
        &project,
        "Nested.cs",
        source,
        "Value;\n        }\n\n        class NearBase",
    );
    assert_eq!(outer["status"], "resolved", "{outer:#}");
    assert_eq!(
        outer["definitions"][0]["fqn"], "Demo.OuterProduct.Value",
        "the outer type factory must type the receiver: {outer:#}"
    );

    let near = definition_at(&project, "Nested.cs", source, "Value;\n        }\n    }\n}");
    assert_eq!(near["status"], "resolved", "{near:#}");
    assert_eq!(
        near["definitions"][0]["fqn"], "Demo.NearProduct.Value",
        "the nearer base factory must win before the enclosing type: {near:#}"
    );
}
