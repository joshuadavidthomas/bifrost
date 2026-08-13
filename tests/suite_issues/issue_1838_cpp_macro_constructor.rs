//! Issue #1838: constructor recovery must not index member initializers as callables.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::{CodeUnitIndex, CodeUnitType, CppAnalyzer, Language};

#[test]
fn cpp_macro_qualified_constructor_keeps_its_name_and_fields() {
    let source = r#"using size_type = unsigned long;

template <typename T>
class Buffer {
public:
    using grow_fun = void (*)(Buffer& buffer, size_type capacity);
    grow_fun grow_;
    size_type size_;
    size_type capacity_;

    MSC_WARNING(suppress : 26495)
    CONSTEXPR Buffer(grow_fun grow, size_type size) noexcept
        : size_(size), capacity_(size), grow_(grow) {}

    void reserve(size_type capacity) {
        grow_(*this, capacity);
    }
};
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("buffer.hpp", source)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let declarations = analyzer.get_all_declarations();

    assert!(
        declarations.iter().any(|unit| {
            unit.kind() == CodeUnitType::Function && unit.fq_name() == "Buffer.Buffer"
        }),
        "the macro-qualified constructor must keep its owner name: {declarations:#?}"
    );
    for initializer in ["Buffer.size_", "Buffer.capacity_", "Buffer.grow_"] {
        assert!(
            declarations.iter().all(|unit| {
                unit.kind() != CodeUnitType::Function || unit.fq_name() != initializer
            }),
            "member initializer {initializer} must not become a function: {declarations:#?}"
        );
    }

    let result = definition_at(&project, "buffer.hpp", source, "grow_(*this, capacity)");
    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(result["definitions"][0]["kind"], "field", "{result:#}");
    assert_eq!(
        result["definitions"][0]["fqn"], "Buffer.grow_",
        "{result:#}"
    );
}
