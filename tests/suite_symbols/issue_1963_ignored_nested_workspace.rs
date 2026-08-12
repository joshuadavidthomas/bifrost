use brokk_bifrost::SearchToolsService;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use std::fs;

#[test]
fn ignored_nested_workspace_registers_analyzer_and_supports_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().canonicalize().unwrap();
    let repo = Repository::init(&repo_root).unwrap();
    fs::write(repo_root.join(".gitignore"), "/target/\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all([".gitignore"], IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Bifrost Test", "test@example.com").unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "ignore build output",
        &tree,
        &[],
    )
    .unwrap();

    let workspace_root = repo_root.join("target/extracted");
    fs::create_dir_all(&workspace_root).unwrap();
    fs::write(
        workspace_root.join("main.go"),
        "package main\n\nfunc usagebenchTarget() {}\n",
    )
    .unwrap();

    let service = SearchToolsService::new_without_semantic_index(workspace_root).unwrap();
    let search = service
        .call_tool_json("search_symbols", r#"{"patterns":["usagebenchTarget"]}"#)
        .unwrap();
    let search_value: Value = serde_json::from_str(&search).unwrap();
    assert!(
        search_value["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        "payload: {search}"
    );
    let exact_symbol = search_value["files"][0]["functions"][0]["symbol"]
        .as_str()
        .expect("search result exact symbol");

    let locations = service
        .call_tool_value(
            "get_symbol_locations",
            serde_json::json!({"symbols": [exact_symbol]}),
        )
        .unwrap();
    assert_eq!(locations["locations"][0]["path"], "main.go");
}
