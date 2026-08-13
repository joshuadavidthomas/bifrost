use super::inline_project::InlineTestProject;
use brokk_bifrost_analysis::Language;
use brokk_bifrost_runtime::extension::{
    CodeQuery, ExtensionCancellation, ExtensionCompatibility, ExtensionCompletion, ExtensionError,
    ExtensionLimitValues, ExtensionLimits, ExtensionRequest, ExtensionResponse, ExtensionWorkspace,
    ExtensionWorkspaceOptions, NormalizedRelativePath, SemanticRelationRequest, SourceSpan,
    StructuralRequest, decode_request_json, encode_request_json, encode_response_json,
};
use serde_json::json;

#[test]
fn generation_is_stable_for_unchanged_source_and_changes_after_edit() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("src/app.ts", "export function answer() { return 42; }\n")
        .build();
    let first = ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let second = ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    assert_eq!(first.generation(), second.generation());

    std::fs::write(
        project.root().join("src/app.ts"),
        "export function answer() { return 43; }\n",
    )
    .unwrap();
    assert_eq!(
        first.generation(),
        second.generation(),
        "an open workspace keeps its immutable generation"
    );
    let changed = ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    assert_ne!(first.generation(), changed.generation());
}

#[test]
fn direct_and_json_structural_requests_are_equivalent() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("src/app.ts", "export function answer() { return 42; }\n")
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let request = ExtensionRequest::Structural(StructuralRequest {
        compatibility: ExtensionCompatibility::default(),
        expected_generation: workspace.generation().clone(),
        query: CodeQuery::from_json(&json!({ "match": { "kind": "function", "name": "answer" } }))
            .unwrap(),
        limits: ExtensionLimits::default(),
    });
    let encoded = encode_request_json(&request).unwrap();
    let decoded = decode_request_json(&encoded).unwrap();
    let direct = workspace
        .execute(request, &ExtensionCancellation::new())
        .unwrap();
    let serialized = workspace
        .execute(decoded, &ExtensionCancellation::new())
        .unwrap();
    assert_eq!(
        encode_response_json(&direct).unwrap(),
        encode_response_json(&serialized).unwrap()
    );
    let ExtensionResponse::Structural(outcome) = direct else {
        panic!("structural response")
    };
    assert_eq!(outcome.completion, ExtensionCompletion::Complete);
    assert!(!outcome.value.unwrap().items.is_empty());
}

#[test]
fn semantic_control_flow_is_source_backed_and_cancellable() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file(
            "src/app.ts",
            "export function answer(flag: boolean) { if (flag) return 42; return 0; }\n",
        )
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let request = SemanticRelationRequest {
        compatibility: ExtensionCompatibility::default(),
        expected_generation: workspace.generation().clone(),
        seed: SourceSpan {
            path: NormalizedRelativePath::new("src/app.ts").unwrap(),
            start_utf8_byte: 20,
            end_utf8_byte: 20,
        },
        limits: ExtensionLimits::default(),
    };
    let outcome = workspace
        .semantic_relations(request.clone(), &ExtensionCancellation::new())
        .unwrap();
    assert_eq!(outcome.completion, ExtensionCompletion::Complete);
    let snapshot = outcome.value.unwrap();
    assert!(!snapshot.nodes.is_empty());
    assert!(!snapshot.edges.is_empty());
    assert!(
        snapshot
            .nodes
            .iter()
            .all(|node| node.span.path.as_str() == "src/app.ts")
    );

    let cancelled = ExtensionCancellation::new();
    cancelled.cancel();
    let outcome = workspace
        .semantic_relations(request.clone(), &cancelled)
        .unwrap();
    assert_eq!(outcome.completion, ExtensionCompletion::Cancelled);
    assert!(outcome.value.is_none());

    let tiny_limits = ExtensionLimits::new(ExtensionLimitValues {
        semantic_nodes: 1,
        semantic_edges: 1,
        ..ExtensionLimitValues::default()
    })
    .unwrap();
    let limited = SemanticRelationRequest {
        limits: tiny_limits,
        ..request
    };
    let outcome = workspace
        .semantic_relations(limited, &ExtensionCancellation::new())
        .unwrap();
    assert!(matches!(
        outcome.completion,
        ExtensionCompletion::Truncated { .. }
    ));
}

#[test]
fn stale_generation_is_rejected_before_execution() {
    let project = InlineTestProject::with_language(Language::TypeScript)
        .file("src/app.ts", "export function answer() { return 42; }\n")
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let request = ExtensionRequest::Structural(StructuralRequest {
        compatibility: ExtensionCompatibility::default(),
        expected_generation: workspace.generation().clone(),
        query: CodeQuery::from_json(&json!({ "match": { "kind": "function", "name": "answer" } }))
            .unwrap(),
        limits: ExtensionLimits::default(),
    });
    let mut encoded: serde_json::Value =
        serde_json::from_slice(&encode_request_json(&request).unwrap()).unwrap();
    encoded["request"][1] = json!("0".repeat(64));
    let stale = decode_request_json(&serde_json::to_vec(&encoded).unwrap()).unwrap();
    assert!(matches!(
        workspace.execute(stale, &ExtensionCancellation::new()),
        Err(ExtensionError::StaleGeneration { .. })
    ));
}

#[test]
fn normalized_paths_reject_noncanonical_spellings() {
    for invalid in ["", "/absolute", "../escape", "src\\app.ts", "src/./app.ts"] {
        assert!(NormalizedRelativePath::new(invalid).is_err(), "{invalid}");
    }
}
