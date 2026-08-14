//! #1788: a non-JavaScript hit in an explicit unindexed candidate must use
//! the shared file-scope identity instead of disappearing.

use crate::common::InlineTestProject;
use brokk_bifrost::hash::HashSet;
use brokk_bifrost::usages::{
    DEFAULT_MAX_FILES, DEFAULT_MAX_USAGES, ExplicitCandidateProvider, FuzzyResult, UsageFinder,
};
use brokk_bifrost::{AnalyzerConfig, CodeUnit, Language, ProjectFile};
use std::sync::Arc;

struct LanguageCase {
    language: Language,
    target_path: &'static str,
    target_source: &'static str,
    indexed_path: &'static str,
    caller_source: &'static str,
    target_name: &'static str,
    call_name: &'static str,
}

fn cases() -> [LanguageCase; 5] {
    [
        LanguageCase {
            language: Language::Go,
            target_path: "target.go",
            target_source: "package sample\nfunc target() {}\n",
            indexed_path: "indexed.go",
            caller_source: "package sample\nfunc caller() { target() }\n",
            target_name: "target",
            call_name: "target",
        },
        LanguageCase {
            language: Language::Ruby,
            target_path: "target.rb",
            target_source: "module Functions\n  def self.target\n  end\nend\n",
            indexed_path: "indexed.rb",
            caller_source: "class Caller\n  def run\n    Functions.target\n  end\nend\n",
            target_name: "target",
            call_name: "target",
        },
        LanguageCase {
            language: Language::Php,
            target_path: "target.php",
            target_source: "<?php\nfunction target() {}\n",
            indexed_path: "indexed.php",
            caller_source: "<?php\nfunction caller() { target(); }\n",
            target_name: "target",
            call_name: "target",
        },
        LanguageCase {
            language: Language::Java,
            target_path: "Functions.java",
            target_source: "package app;\nclass Functions { static void target() {} }\n",
            indexed_path: "Caller.java",
            caller_source: "package app;\nclass Caller { void run() { Functions.target(); } }\n",
            target_name: "target",
            call_name: "target",
        },
        LanguageCase {
            language: Language::Kotlin,
            target_path: "Functions.kt",
            target_source: "package app\nobject Functions {\n  fun target() {}\n}\n",
            indexed_path: "Caller.kt",
            caller_source: "package app\nclass Caller {\n  fun run() { Functions.target() }\n}\n",
            target_name: "target",
            call_name: "target",
        },
    ]
}

fn oversized_comment(language: Language) -> String {
    let marker = if language == Language::Ruby { '#' } else { '/' };
    if marker == '#' {
        format!("# {}\n", "x".repeat(20_000))
    } else {
        format!("// {}\n", "x".repeat(20_000))
    }
}

fn unindexed_source(language: Language, source: &str) -> String {
    let comment = oversized_comment(language);
    if language == Language::Php {
        let rest = source.strip_prefix("<?php\n").expect("PHP open tag");
        format!("<?php\n{comment}{rest}")
    } else {
        format!("{comment}{source}")
    }
}

#[test]
fn non_js_explicit_unindexed_candidates_keep_file_scope_hits() {
    for case in cases() {
        let unindexed_path = format!(
            "unindexed.{}",
            case.indexed_path.rsplit_once('.').unwrap().1
        );
        let unindexed_source = unindexed_source(case.language, case.caller_source);
        let project = InlineTestProject::with_language(case.language)
            .file(case.target_path, case.target_source)
            .file(case.indexed_path, case.caller_source)
            .file(&unindexed_path, &unindexed_source)
            .build();
        let workspace = project.workspace_analyzer(AnalyzerConfig::default());
        let analyzer = workspace.analyzer();
        let target_file = project.file(case.target_path);
        let indexed_file = project.file(case.indexed_path);
        let unindexed_file = project.file(&unindexed_path);

        assert!(
            analyzer.get_declarations(&unindexed_file).is_empty(),
            "{:?} witness needs an unindexed explicit candidate",
            case.language
        );
        let declarations = analyzer.get_all_declarations();
        let target = declarations
            .iter()
            .find(|unit| {
                unit.source() == &target_file
                    && unit.is_function()
                    && unit.identifier() == case.target_name
            })
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "missing {:?} target in {:#?}",
                    case.language,
                    declarations
                        .iter()
                        .filter(|unit| unit.source() == &target_file)
                        .collect::<Vec<_>>()
                )
            });

        let files: HashSet<ProjectFile> = [indexed_file.clone(), unindexed_file.clone()]
            .into_iter()
            .collect();
        let provider = ExplicitCandidateProvider::new(Arc::new(files));
        let result = UsageFinder::new()
            .query_with_provider(
                analyzer,
                std::slice::from_ref(&target),
                Some(&provider),
                DEFAULT_MAX_FILES,
                DEFAULT_MAX_USAGES,
            )
            .result;
        let FuzzyResult::Success {
            hits_by_overload,
            unproven_by_overload,
            ..
        } = &result
        else {
            panic!("expected {:?} success, got {result:#?}", case.language);
        };
        let hits = hits_by_overload
            .values()
            .chain(unproven_by_overload.values())
            .flatten()
            .collect::<Vec<_>>();

        let unindexed_hit = hits
            .iter()
            .find(|hit| hit.file == unindexed_file)
            .unwrap_or_else(|| {
                panic!(
                    "{:?} unindexed reference disappeared: {result:#?}",
                    case.language
                )
            });
        assert_eq!(
            CodeUnit::file_scope(unindexed_file.clone()),
            unindexed_hit.enclosing,
            "{:?} unindexed hit needs the shared file scope",
            case.language
        );
        assert!(
            unindexed_hit.snippet.contains(case.call_name),
            "{:?} unindexed hit must cover the requested call: {unindexed_hit:#?}",
            case.language
        );

        let indexed_hit = hits
            .iter()
            .find(|hit| hit.file == indexed_file)
            .unwrap_or_else(|| {
                panic!(
                    "{:?} indexed reference disappeared: {result:#?}",
                    case.language
                )
            });
        assert!(
            indexed_hit.enclosing.is_function(),
            "{:?} indexed hit must keep its callable owner: {indexed_hit:#?}",
            case.language
        );
    }
}
