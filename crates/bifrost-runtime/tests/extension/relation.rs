use brokk_bifrost_runtime::extension::{
    NormalizedRelativePath, SemanticDirection, SemanticEvidence, SemanticNodeOccurrence,
    SemanticProof, SemanticRelationCompleteness, SemanticRelationEdge, SemanticRelationKind,
    SemanticRelationLimits, SemanticRelationRequest, SemanticRelationScope,
    SemanticRelationSnapshot, SemanticRelationStatus, SemanticSeed, SourceSpan, StableDigest,
    WorkspaceGeneration, decode_relation_request_json, decode_relation_snapshot_json,
    encode_relation_request_json, encode_relation_snapshot_json, read_relation_snapshot_jsonl,
    write_relation_snapshot_jsonl,
};
use std::io::Cursor;

fn digest(byte: char) -> StableDigest {
    StableDigest::parse(byte.to_string().repeat(64)).unwrap()
}
fn generation() -> WorkspaceGeneration {
    serde_json::from_str(&format!("\"{}\"", "a".repeat(64))).unwrap()
}
fn span(start: u64, end: u64) -> SourceSpan {
    SourceSpan {
        path: NormalizedRelativePath::new("src/app.ts").unwrap(),
        start_utf8_byte: start,
        end_utf8_byte: end,
    }
}
fn limits() -> SemanticRelationLimits {
    SemanticRelationLimits {
        max_seed_matches: 4,
        max_call_depth: 2,
        max_nodes: 20,
        max_edges: 40,
        max_boundaries: 4,
        max_diagnostics: 4,
        max_output_bytes: 1_000_000,
        max_materialized_files: 4,
        max_traversal_steps: 1000,
        max_source_bytes: 100_000,
    }
}

#[test]
fn request_json_is_canonical_and_strict() {
    let request = SemanticRelationRequest::new(
        generation(),
        vec![SemanticSeed::Source { span: span(0, 1) }],
        SemanticRelationScope::Procedure,
        vec![SemanticRelationKind::ControlFlow],
        SemanticDirection::Both,
        limits(),
    )
    .unwrap();
    let encoded = encode_relation_request_json(&request).unwrap();
    assert!(encoded.ends_with(b"\n"));
    assert_eq!(decode_relation_request_json(&encoded).unwrap(), request);
    let with_unknown = String::from_utf8(encoded)
        .unwrap()
        .replacen('{', "{\"unknown\":true,", 1);
    assert!(decode_relation_request_json(with_unknown.as_bytes()).is_err());
}

#[test]
fn snapshot_json_and_jsonl_are_equivalent() {
    let evidence = SemanticEvidence {
        kind: "control_edge".into(),
        mappings: vec![span(0, 1)].into_boxed_slice(),
        proof: SemanticProof::Proven,
        completeness: SemanticRelationCompleteness::Complete,
    };
    let nodes = vec![
        SemanticNodeOccurrence {
            local_id: 99,
            stable_id: digest('b'),
            call_context: Box::new([]),
            span: span(2, 3),
            role: "program_point".into(),
        },
        SemanticNodeOccurrence {
            local_id: 98,
            stable_id: digest('a'),
            call_context: Box::new([]),
            span: span(0, 1),
            role: "program_point".into(),
        },
    ];
    let edges = vec![SemanticRelationEdge {
        source: 99,
        target: 98,
        kind: SemanticRelationKind::ControlFlow,
        subtype: Some("normal".into()),
        proof: SemanticProof::Proven,
        completeness: SemanticRelationCompleteness::Complete,
        evidence: vec![evidence].into_boxed_slice(),
    }];
    let snapshot = SemanticRelationSnapshot::try_new(
        generation(),
        digest('c'),
        SemanticRelationStatus::Complete,
        nodes,
        edges,
        Vec::new(),
    )
    .unwrap();
    let json = encode_relation_snapshot_json(&snapshot).unwrap();
    assert_eq!(decode_relation_snapshot_json(&json).unwrap(), snapshot);
    let mut jsonl = Vec::new();
    write_relation_snapshot_jsonl(&snapshot, &mut jsonl).unwrap();
    assert_eq!(
        read_relation_snapshot_jsonl(Cursor::new(jsonl)).unwrap(),
        snapshot
    );
}

#[test]
fn complete_and_incomplete_empty_snapshots_are_distinct() {
    let complete = SemanticRelationSnapshot::try_new(
        generation(),
        digest('d'),
        SemanticRelationStatus::Complete,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert!(complete.authoritative_absence());
    let mut truncated_json = encode_relation_snapshot_json(&complete).unwrap();
    let text = String::from_utf8(truncated_json.clone())
        .unwrap()
        .replace("\"complete\"", "\"partial\"");
    truncated_json = text.into_bytes();
    assert!(
        decode_relation_snapshot_json(&truncated_json).is_err(),
        "digest binds completion"
    );
}
