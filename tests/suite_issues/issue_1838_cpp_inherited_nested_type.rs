//! Issue #1838: inherited C++ type qualifiers must retain nested-type usage.

use crate::common::InlineTestProject;
use crate::common::usage_graph::{has_edge, usage_graph_at};
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, CppAnalyzer, Language};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn cpp_inherited_type_qualifier_records_terminal_nested_type() {
    let source = r#"class RemoteStorage {
public:
    class Backend {
    public:
        class Attribute {};
    };
};

class HttpBackend : public RemoteStorage::Backend {
public:
    Backend::Attribute inherited;
    RemoteStorage::Backend::Attribute qualified;
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("nested.cpp", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let declarations = analyzer.get_all_declarations();
    let target = declarations
        .iter()
        .find(|unit| unit.fq_name() == "RemoteStorage$Backend$Attribute")
        .cloned()
        .unwrap_or_else(|| panic!("missing nested type in {declarations:#?}"));
    let file = project.file("nested.cpp");
    let provider = ExplicitCandidateProvider::new(Arc::new(std::iter::once(file).collect()));
    let result = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(
            &analyzer,
            std::slice::from_ref(&target),
            Some(&provider),
            1,
            100,
        )
        .result;
    let FuzzyResult::Success {
        hits_by_overload, ..
    } = result
    else {
        panic!("expected an authoritative usage result");
    };
    let hits = hits_by_overload
        .get(&target)
        .into_iter()
        .flatten()
        .map(|hit| (hit.start_offset, hit.end_offset))
        .collect::<BTreeSet<_>>();
    let type_range = |line: &str, type_text: &str| {
        let line_start = source
            .find(line)
            .unwrap_or_else(|| panic!("missing fixture line {line:?}"));
        let token_start = line_start
            + line
                .find(type_text)
                .unwrap_or_else(|| panic!("missing type in {line:?}"));
        (token_start, token_start + type_text.len())
    };
    let inherited = type_range("    Backend::Attribute inherited;", "Backend::Attribute");
    let qualified = type_range(
        "    RemoteStorage::Backend::Attribute qualified;",
        "RemoteStorage::Backend::Attribute",
    );

    assert!(
        hits.contains(&qualified),
        "the fully qualified control must resolve: {hits:#?}"
    );
    assert!(
        hits.contains(&inherited),
        "the inherited qualifier must record its terminal nested type: {hits:#?}"
    );
}

#[test]
fn cpp_usage_graph_records_nested_type_through_injected_base_name() {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "nested.cpp",
            r#"class RemoteStorage {
public:
    class Backend {
    public:
        class Attribute {};
    };
};

class HttpBackend : public RemoteStorage::Backend {
public:
    void use() { Backend::Attribute value; }
};
"#,
        )
        .build();
    let value = usage_graph_at(project.root(), "{}");

    assert!(
        has_edge(&value, "HttpBackend.use", "RemoteStorage$Backend$Attribute"),
        "the workspace graph must record the inherited nested type: {}",
        value["edges"]
    );
}

#[test]
fn cpp_injected_base_name_does_not_override_shadowing_or_ambiguity() {
    let project = InlineTestProject::with_language(Language::Cpp)
        .file(
            "controls.cpp",
            r#"namespace base {
class Backend { public: class Attribute {}; };
}

class Shadowed : public base::Backend {
public:
    class Backend { public: class Attribute {}; };
    void use() { Backend::Attribute value; }
};

namespace left {
class Backend { public: class Attribute {}; };
}
namespace right {
class Backend { public: class Attribute {}; };
}
class Ambiguous : public left::Backend, public right::Backend {
public:
    void use() { Backend::Attribute value; }
};
"#,
        )
        .build();
    let value = usage_graph_at(project.root(), "{}");

    assert!(
        !has_edge(&value, "Shadowed.use", "base.Backend$Attribute"),
        "the shadowed base type must not receive the reference: {}",
        value["edges"]
    );
    assert!(
        !has_edge(&value, "Ambiguous.use", "left.Backend$Attribute")
            && !has_edge(&value, "Ambiguous.use", "right.Backend$Attribute"),
        "distinct same-named bases must remain ambiguous: {}",
        value["edges"]
    );
}
