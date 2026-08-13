//! #2111: a declaration whose terminal segment contains a path separator or an
//! extra dot must stay addressable and must be named by that whole segment.
//!
//! Bundled and generated JS/TS carries object-literal keys such as
//! `"data/web-interface.csv"`. The extractor records that key as one `Member`
//! segment, but the display helper split the rendered name on `.` and called
//! the declaration `csv`, and the resolver split a selector the same way, so
//! the name every outline offered resolved to nothing.

use crate::common::{BuiltInlineTestProject, InlineTestProject, call_tool, symbol_sources};
use serde_json::Value;

/// The reproduction shape: a `.mjs` module whose exported map is keyed by
/// workspace-relative CSV paths, next to an ordinary sibling key that shares
/// every other property of the declaration.
const MANIFEST_MJS: &str = r#"
export const uiUxDbFileRenames = {
  "data/web-interface.csv": "web interface renames",
  plain: "no slash here",
};
"#;

const MANIFEST_PATH: &str = "packages/shared-skills/scripts/frontend-refs-manifest.mjs";

fn manifest_project() -> BuiltInlineTestProject {
    InlineTestProject::new()
        .file(MANIFEST_PATH, MANIFEST_MJS)
        .build()
}

fn outline_lines(project: &BuiltInlineTestProject) -> Vec<String> {
    let listed = call_tool(
        project,
        "list_symbols",
        &serde_json::json!({ "file_patterns": [MANIFEST_PATH] }).to_string(),
    );
    listed["files"][0]["lines"]
        .as_array()
        .unwrap_or_else(|| panic!("list_symbols returned no outline: {listed}"))
        .iter()
        .map(|line| line.as_str().expect("outline line").to_string())
        .collect()
}

/// Every selector shape the reply offers as a next step, from whichever
/// payload the tool answered through.
fn offered_selectors(result: &Value) -> Vec<String> {
    let mut offered = Vec::new();
    for source in result["sources"].as_array().into_iter().flatten() {
        offered.push(source["label"].as_str().expect("source label").to_string());
    }
    for item in result["ambiguous"].as_array().into_iter().flatten() {
        for candidate in item["matches"].as_array().into_iter().flatten() {
            offered.push(candidate.as_str().expect("match").to_string());
        }
    }
    offered
}

/// The outline names the declaration by its whole recorded segment. Before
/// #2111 it named it `csv`, the tail of a dot-split of one segment, and an
/// agent copying that name out of the outline could not address anything.
#[test]
fn outline_names_a_path_like_terminal_segment_in_full() {
    let project = manifest_project();
    let lines = outline_lines(&project);
    assert!(
        lines
            .iter()
            .any(|line| line.trim() == "- data/web-interface.csv"),
        "outline must name the whole recorded segment: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.trim() == "- csv"),
        "outline must not offer a dot-split fragment of a segment: {lines:?}"
    );
}

/// The faithful `path#terminal` spelling of such a declaration resolves. The
/// plain sibling in the same object literal is the control: the two spellings
/// must reach the same payload shape, because the only difference between the
/// declarations is that one terminal segment happens to contain `/` and `.`.
#[test]
fn path_like_terminal_segment_is_addressable_by_its_anchored_spelling() {
    let project = manifest_project();

    let path_like = symbol_sources(&project, &format!("{MANIFEST_PATH}#data/web-interface.csv"));
    let plain = symbol_sources(&project, &format!("{MANIFEST_PATH}#plain"));

    assert!(
        path_like["not_found"].as_array().is_some_and(Vec::is_empty),
        "the faithful anchored spelling must resolve: {path_like:#}"
    );
    let offered = offered_selectors(&path_like);
    assert!(
        offered
            .iter()
            .all(|selector| selector.ends_with("uiUxDbFileRenames.data/web-interface.csv")),
        "every selector offered must name the csv declaration: {offered:?}"
    );
    assert_eq!(
        offered.len(),
        offered_selectors(&plain).len(),
        "a slash-bearing terminal must resolve exactly as its plain sibling does: \
         {path_like:#} vs {plain:#}"
    );
}

/// The whole display fq stays addressable too, so the two spellings an agent
/// picks between agree.
#[test]
fn path_like_terminal_segment_is_addressable_by_its_display_fq() {
    let project = manifest_project();
    let result = symbol_sources(
        &project,
        "frontend-refs-manifest.mjs.uiUxDbFileRenames.data/web-interface.csv",
    );
    assert!(
        result["not_found"].as_array().is_some_and(Vec::is_empty),
        "the display fq must resolve: {result:#}"
    );
    assert_eq!(
        vec!["frontend-refs-manifest.mjs.uiUxDbFileRenames.data/web-interface.csv".to_string()],
        offered_selectors(&result),
        "{result:#}"
    );
}

/// The dot-split fragment must not resolve to anything: it never named a
/// declaration, and answering it would be a wrong answer rather than a
/// missing one. It has to fail with a usable hint instead.
#[test]
fn a_dot_split_fragment_of_a_segment_fails_with_a_hint() {
    let project = manifest_project();
    let result = symbol_sources(&project, &format!("{MANIFEST_PATH}#csv"));
    let not_found = result["not_found"].as_array().expect("not_found array");
    assert_eq!(1, not_found.len(), "{result:#}");
    let note = not_found[0]["note"].as_str().unwrap_or_default();
    assert!(!note.trim().is_empty(), "{result:#}");
}
