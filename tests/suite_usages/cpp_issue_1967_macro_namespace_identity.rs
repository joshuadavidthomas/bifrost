//! Issue #1967: one logical namespace must keep one identity across files when
//! unknown begin/end macros damage only one file's tree-sitter namespace node.

use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, CppAnalyzer, Language};
use std::sync::Arc;

#[test]
fn macro_export_boundary_keeps_following_namespace_identity_across_files() {
    let primary = r#"FMT_BEGIN_NAMESPACE
template <typename T> struct prelude {};
namespace detail {
struct before_export {};
}  // namespace detail

FMT_BEGIN_EXPORT
class FMT_SO_VISIBILITY("default") format_error : public runtime_error {
 public:
  using runtime_error::runtime_error;
};
class loc_value;
FMT_END_EXPORT
namespace detail {
auto write_console(int fd) -> bool;
void print();
}  // namespace detail

namespace detail {
namespace dragonbox {
template <typename T> struct float_info {
  using carrier_uint = unsigned;
};
}  // namespace dragonbox
}  // namespace detail
FMT_END_NAMESPACE
"#;
    let consumer = r#"#include "primary.h"
namespace detail {
namespace dragonbox {
template <typename T> struct cache_accessor {
  using carrier_uint = typename float_info<T>::carrier_uint;
  carrier_uint result;
};
}  // namespace dragonbox
}  // namespace detail
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("primary.h", primary)
        .file("consumer.h", consumer)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let primary_definitions = analyzer.get_declarations(&project.file("primary.h"));
    let target = primary_definitions
        .iter()
        .find(|unit| unit.identifier() == "float_info")
        .unwrap_or_else(|| panic!("primary float_info template: {primary_definitions:#?}"))
        .clone();
    assert_eq!(
        "detail::dragonbox.float_info",
        target.fq_name(),
        "the macro-damaged file must retain the ordinary namespace prefix"
    );
    let consumer_definitions = analyzer.get_declarations(&project.file("consumer.h"));
    let cache = consumer_definitions
        .into_iter()
        .find(|unit| unit.identifier() == "cache_accessor")
        .expect("consumer cache_accessor template");
    assert_eq!(
        "detail::dragonbox.cache_accessor",
        cache.fq_name(),
        "both files must use the same namespace prefix"
    );

    let provider = ExplicitCandidateProvider::new(Arc::new(
        [project.file("primary.h"), project.file("consumer.h")]
            .into_iter()
            .collect::<HashSet<_>>(),
    ));
    let result = UsageFinder::new()
        .with_authoritative_scope(true)
        .query_with_provider(&analyzer, &[target], Some(&provider), 2, 1000)
        .result;
    let FuzzyResult::Success {
        hits_by_overload,
        unproven_by_overload,
        ..
    } = result
    else {
        panic!("expected authoritative C++ usage result");
    };
    let reference_start = consumer
        .find("float_info<T>::carrier_uint")
        .expect("dependent primary-template reference");
    let reference_end = reference_start + "float_info".len();
    assert!(
        hits_by_overload
            .values()
            .chain(unproven_by_overload.values())
            .flatten()
            .any(|hit| {
                hit.file == project.file("consumer.h")
                    && hit.start_offset <= reference_start
                    && hit.end_offset >= reference_end
            }),
        "complete inverse lookup must cover the dependent alias owner: hits={hits_by_overload:#?}, unproven={unproven_by_overload:#?}"
    );
}
