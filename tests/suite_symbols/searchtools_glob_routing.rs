//! End-to-end contract pins for how `get_summaries` resolves directory and glob
//! targets (#1738).
//!
//! Routing matches these targets against the session's cached workspace listing
//! and then confirms each match against the analyzer, instead of enumerating the
//! whole analyzed file set once per language per request. That is a pure cost
//! change, so these tests pin the parts a caller can observe: which files a glob
//! answers with, the order they come back in, and the completeness markers a
//! container listing carries.

use crate::common::{BuiltInlineTestProject, InlineTestProject};
use brokk_bifrost::SearchToolsService;
use brokk_bifrost::searchtools::GET_SUMMARIES_MAX_FILES_PER_TARGET;
use serde_json::Value;
use std::path::Path;

fn service(project: &BuiltInlineTestProject) -> SearchToolsService {
    SearchToolsService::new_without_semantic_index(project.root().to_path_buf()).unwrap()
}

fn summaries_json(service: &SearchToolsService, targets_json: &str) -> Value {
    let payload = service
        .call_tool_json("get_summaries", targets_json)
        .unwrap();
    serde_json::from_str(&payload).unwrap()
}

fn summary_paths(value: &Value) -> Vec<String> {
    value["summaries"]
        .as_array()
        .expect("summaries array")
        .iter()
        .map(|summary| summary["path"].as_str().expect("summary path").to_string())
        .collect()
}

/// The advertised order is path order, and it must not depend on the order the
/// match universe happened to produce candidates in.
#[test]
fn glob_summaries_keep_deterministic_path_order() {
    let project = InlineTestProject::new()
        .file(
            "src/zulu/Zulu.java",
            "public class Zulu { int z() { return 0; } }\n",
        )
        .file(
            "src/alpha/Alpha.java",
            "public class Alpha { int a() { return 0; } }\n",
        )
        .file(
            "src/mike/Mike.java",
            "public class Mike { int m() { return 0; } }\n",
        )
        .file("src/notes.txt", "not source\n")
        .build();
    let service = service(&project);

    let value = summaries_json(&service, r#"{"targets":["src/**/*.java"]}"#);

    assert_eq!(
        vec![
            "src/alpha/Alpha.java".to_string(),
            "src/mike/Mike.java".to_string(),
            "src/zulu/Zulu.java".to_string(),
        ],
        summary_paths(&value),
        "{value}"
    );
    assert!(
        value["too_broad"].as_array().is_none_or(Vec::is_empty),
        "{value}"
    );
}

/// The workspace listing is a superset of the analyzed set, so resolving globs
/// against it only works because every match is confirmed afterwards. A file the
/// analyzer was told to ignore is in the listing and must still never appear as
/// a summary.
#[test]
fn bifrostignored_file_is_listed_but_never_summarized_by_a_glob() {
    let project = InlineTestProject::new()
        .file(".bifrostignore", "vendor/\n")
        .file(
            "src/Real.java",
            "public class Real { int r() { return 0; } }\n",
        )
        .file(
            "vendor/Ghost.java",
            "public class Ghost { int g() { return 0; } }\n",
        )
        .build();
    let repository = git2::Repository::init(project.root()).unwrap();
    let mut index = repository.index().unwrap();
    for path in [".bifrostignore", "src/Real.java", "vendor/Ghost.java"] {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let service = service(&project);

    // The ignored file is genuinely part of the workspace listing: a directory
    // target lists it, which is the contract for `.bifrostignore` (it hides
    // symbols, not files).
    let listing = summaries_json(&service, r#"{"targets":["vendor"]}"#);
    let entries = listing["listings"][0]["entries"]
        .as_array()
        .expect("vendor listing entries");
    assert!(
        entries
            .iter()
            .any(|entry| entry["path"] == "vendor/Ghost.java"),
        "{listing}"
    );

    let value = summaries_json(&service, r#"{"targets":["**/*.java"]}"#);

    assert_eq!(
        vec!["src/Real.java".to_string()],
        summary_paths(&value),
        "{value}"
    );
}

/// The work layer hands every listing its own completeness markers: the true
/// entry count and whether what it carries is all of them. The byte-budget
/// truncation that later flips `truncated` lives in the MCP response layer
/// (`fit_get_summaries_output_to_budget`, reached only through
/// `rmcp_host`), so what a work-layer listing must guarantee is that the two
/// fields agree with the entries it actually shipped -- including for a
/// directory holding far more files than any target may summarize.
#[test]
fn directory_listing_reports_every_entry_and_marks_itself_complete() {
    let mut project = InlineTestProject::new();
    for index in 0..(GET_SUMMARIES_MAX_FILES_PER_TARGET * 6) {
        project = project.file(
            format!("wide/LongEnoughFileName{index:03}.java"),
            format!("public class LongEnoughFileName{index:03} {{ }}\n"),
        );
    }
    let project = project.build();
    let service = service(&project);

    let value = summaries_json(&service, r#"{"targets":["wide"]}"#);

    let listing = &value["listings"][0];
    let entries = listing["entries"].as_array().unwrap();
    assert_eq!(
        GET_SUMMARIES_MAX_FILES_PER_TARGET * 6,
        entries.len(),
        "a directory listing is not subject to the per-target summary cap: {value}"
    );
    assert_eq!(
        entries.len() as u64,
        listing["total_entries"].as_u64().unwrap(),
        "{value}"
    );
    assert_eq!(Some(false), listing["truncated"].as_bool(), "{value}");
    let first = entries[0]["path"].as_str().unwrap();
    let last = entries[entries.len() - 1]["path"].as_str().unwrap();
    assert!(
        first < last,
        "entries must be in path order: {first} {last}"
    );
}
