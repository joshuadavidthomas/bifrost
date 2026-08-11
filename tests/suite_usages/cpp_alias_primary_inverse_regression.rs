use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{ExplicitCandidateProvider, FuzzyResult, UsageFinder};
use brokk_bifrost::{CodeUnitIndex, CppAnalyzer, Language};
use std::collections::BTreeSet;
use std::sync::Arc;

fn token_range(source: &str, marker: &str, token: &str) -> (usize, usize) {
    let marker_start = source.find(marker).expect("fixture marker");
    let token_start = marker.find(token).expect("token in fixture marker");
    let start = marker_start + token_start;
    (start, start + token.len())
}

#[test]
fn cpp_class_owned_alias_references_retain_the_primary_template_identity() {
    let header = r#"template <typename V>
struct MissingBacking {};

template <typename V>
struct MissingOtherBacking {};

template <typename V>
struct ExternalMap {
  typedef MissingBacking<V> Type;
};

template <typename V>
struct OtherMap {
  typedef MissingOtherBacking<V> Type;
};

struct BuildLog {
  typedef ExternalMap<int>::Type Entries;
  Entries direct;
  void member_use();
};

struct Decoy {
  typedef OtherMap<int>::Type Entries;
  Entries wrong;
  void member_use();
};
"#;
    let consumer = r#"#include "types.h"

void member_use(BuildLog::Entries value) {
  BuildLog::Entries::iterator iter;
}

void BuildLog::member_use() {
  Entries::iterator iter;
}

void wrong_qualified(Decoy::Entries value) {
  Decoy::Entries::iterator iter;
}

void Decoy::member_use() {
  Entries::iterator iter;
}

void shadowed() {
  typedef OtherMap<int>::Type Entries;
  Entries wrong;
}
"#;
    let project = InlineTestProject::with_language(Language::Cpp)
        .file("types.h", header)
        .file("consumer.cc", consumer)
        .build();
    let analyzer = CppAnalyzer::from_project(project.project().clone());
    let target = analyzer
        .get_definitions("ExternalMap")
        .into_iter()
        .find(|unit| unit.source() == &project.file("types.h"))
        .expect("ExternalMap target");
    let candidates = Arc::new(
        [project.file("types.h"), project.file("consumer.cc")]
            .into_iter()
            .collect::<HashSet<_>>(),
    );
    let provider = ExplicitCandidateProvider::new(candidates);
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
    let ranges = hits_by_overload
        .values()
        .chain(unproven_by_overload.values())
        .flatten()
        .map(|hit| (hit.file.clone(), hit.start_offset, hit.end_offset))
        .collect::<BTreeSet<_>>();

    for (file, source, marker, token) in [
        (
            project.file("types.h"),
            header,
            "Entries direct;",
            "Entries",
        ),
        (
            project.file("consumer.cc"),
            consumer,
            "BuildLog::Entries value",
            "Entries",
        ),
        (
            project.file("consumer.cc"),
            consumer,
            "BuildLog::Entries::iterator iter",
            "Entries",
        ),
        (
            project.file("consumer.cc"),
            consumer,
            "Entries::iterator iter;\n}",
            "Entries",
        ),
    ] {
        let (start, end) = token_range(source, marker, token);
        assert!(
            ranges.contains(&(file, start, end)),
            "primary-template target must retain alias reference `{marker}`: {ranges:#?}"
        );
    }

    for (file, source, marker) in [
        (project.file("types.h"), header, "Entries wrong;"),
        (
            project.file("consumer.cc"),
            consumer,
            "Decoy::Entries value",
        ),
        (
            project.file("consumer.cc"),
            consumer,
            "Decoy::Entries::iterator iter",
        ),
        (
            project.file("consumer.cc"),
            consumer,
            "void Decoy::member_use() {\n  Entries::iterator iter;",
        ),
        (project.file("consumer.cc"), consumer, "Entries wrong;"),
    ] {
        let (start, end) = token_range(source, marker, "Entries");
        assert!(
            !ranges.contains(&(file, start, end)),
            "different primary-template alias must not match: {ranges:#?}"
        );
    }
}
