//! Issue #1838: a macro-prefixed return type is not an out-of-line owner.

use crate::common::{InlineTestProject, definition_at};
use brokk_bifrost::Language;

#[test]
fn cpp_macro_prefixed_member_return_type_keeps_class_scope() {
    let source = r#"namespace nonstd { namespace expected_lite {
template <typename E>
void make_unexpected(E value) {}

#define nsel_constexpr14

#if USE_DEFAULT_ERROR
template <typename T, typename E = int>
class expected
#else
template <typename T, typename E>
class expected
#endif
{
public:
    template <typename F>
    nsel_constexpr14 expected<T, E> transform(F&&) &&
    {
        make_unexpected(1);
        return {};
    }
};
}}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("expected.hpp", source)
        .build();
    let result = definition_at(&project, "expected.hpp", source, "make_unexpected(1)");

    assert_eq!(result["status"], "resolved", "{result:#}");
    assert_eq!(
        result["definitions"].as_array().unwrap().len(),
        1,
        "{result:#}"
    );
    assert_eq!(
        result["definitions"][0]["fqn"], "nonstd::expected_lite.make_unexpected",
        "{result:#}"
    );
}
