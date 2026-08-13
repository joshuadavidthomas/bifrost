use brokk_bifrost_runtime::extension::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

fn digest(bytes: &[u8]) -> StableDigest {
    StableDigest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

fn identity(id: &str) -> ObservationIdentity {
    ObservationIdentity {
        namespace: "test".into(),
        id: id.into(),
        category: "conformance".into(),
        outcome: "caller_owned".into(),
        attributes: BTreeMap::new(),
    }
}

fn document(
    workspace: &ExtensionWorkspace,
    path: &str,
    source: &str,
    start: usize,
    end: usize,
) -> ObservationDocument {
    ObservationDocument {
        schema_version: "1.0".into(),
        compatibility: ExtensionCompatibility::default(),
        expected_generation: workspace.generation().clone(),
        repository: ObservationRepository {
            kind: "fixture".into(),
            identity: "two-language".into(),
            revision: "r1".into(),
            mount: "workspace".into(),
            dirty_overlay: None,
        },
        subject: identity("subject"),
        run: identity("run"),
        producer: ObservationProducer {
            format_name: "generic-ranges".into(),
            format_version: "1".into(),
            tool_name: "fixture".into(),
            tool_version: "1".into(),
            adapter_name: "fixture".into(),
            adapter_version: "1".into(),
            input_content_sha256: digest(source.as_bytes()),
        },
        configuration_hash: digest(b"configuration"),
        limits: ObservationMappingLimits {
            max_records: 8,
            max_source_bytes: 1_000_000,
            max_candidate_nodes: 256,
            max_mapped_nodes_per_record: 128,
            max_total_mapped_nodes: 256,
            max_diagnostics: 16,
            max_output_bytes: 1_000_000,
            max_materialized_files: 8,
            max_traversal_steps: 10_000,
        },
        include_synthetic: false,
        include_generated: false,
        records: vec![ObservationRecord {
            record_id: "record-1".into(),
            source: ObservationSourceIdentity {
                mount: "workspace".into(),
                span: SourceSpan {
                    path: NormalizedRelativePath::new(path).unwrap(),
                    start_utf8_byte: start as u64,
                    end_utf8_byte: end as u64,
                },
                content_sha256: digest(source.as_bytes()),
                language: None,
                generated_provenance: None,
            },
            kind: ObservationRecordKind::Range,
            category: "sample".into(),
            outcome: "observed".into(),
            attributes: BTreeMap::new(),
        }]
        .into_boxed_slice(),
    }
}

fn assert_language(path: &str, source: &str, needle: &str) {
    let project = super::inline_project::InlineTestProject::new()
        .file(path, source)
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let start = source.find(needle).unwrap();
    let doc = document(&workspace, path, source, start, start + needle.len());
    let result = workspace
        .map_observations(&doc, &ExtensionCancellation::new())
        .unwrap();
    assert_eq!(result.outcomes.len(), 1);
    assert!(
        matches!(&result.outcomes[0], ObservationMappingOutcome::Exact { nodes, .. } if !nodes.is_empty())
    );

    let json = encode_observation_document_json(&doc).unwrap();
    assert_eq!(decode_observation_document_json(&json).unwrap(), doc);
    let result_json = encode_observation_result_json(&result).unwrap();
    assert_eq!(
        decode_observation_result_json(&result_json).unwrap(),
        result
    );
    let mut jsonl = Vec::new();
    write_observation_mapping_jsonl(&result, &mut jsonl).unwrap();
    assert_eq!(
        read_observation_mapping_jsonl(Cursor::new(jsonl)).unwrap(),
        result
    );
    let request = ExtensionRequest::Observations(Box::new(doc.clone()));
    let decoded = decode_request_json(&encode_request_json(&request).unwrap()).unwrap();
    let ExtensionResponse::Observations(serialized) = workspace
        .execute(decoded, &ExtensionCancellation::new())
        .unwrap()
    else {
        panic!("observation response")
    };
    assert_eq!(serialized, result);
}

#[test]
fn java_and_python_ranges_map_to_stable_semantic_nodes() {
    assert_language(
        "Sample.java",
        "class Sample { void f() { int x = 1; x++; } }",
        "x++",
    );
    assert_language("sample.py", "def f():\n    x = 1\n    x += 1\n", "x += 1");
}

#[test]
fn content_identity_prevents_cross_path_or_stale_mapping() {
    let source = "def f():\n    return 1\n";
    let project = super::inline_project::InlineTestProject::new()
        .file("a.py", source)
        .file("b.py", source)
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let mut doc = document(&workspace, "a.py", source, 13, 21);
    doc.records[0].source.content_sha256 = digest(b"different");
    let result = workspace
        .map_observations(&doc, &ExtensionCancellation::new())
        .unwrap();
    assert!(matches!(
        result.outcomes[0],
        ObservationMappingOutcome::Stale { .. }
    ));
}

#[test]
fn malformed_and_interrupted_jsonl_are_rejected() {
    let source = "def f():\n    return 1\n";
    let project = super::inline_project::InlineTestProject::new()
        .file("a.py", source)
        .build();
    let workspace =
        ExtensionWorkspace::open(ExtensionWorkspaceOptions::new(project.root())).unwrap();
    let doc = document(&workspace, "a.py", source, 13, 21);
    let mut json = encode_observation_document_json(&doc).unwrap();
    let position = json.iter().position(|byte| *byte == b'{').unwrap() + 1;
    json.splice(position..position, b"\"unknown\":true,".iter().copied());
    assert!(decode_observation_document_json(&json).is_err());

    let result = workspace
        .map_observations(&doc, &ExtensionCancellation::new())
        .unwrap();
    let mut jsonl = Vec::new();
    write_observation_mapping_jsonl(&result, &mut jsonl).unwrap();
    jsonl.truncate(jsonl.iter().rposition(|byte| *byte == b'\n').unwrap());
    let prior = jsonl.iter().rposition(|byte| *byte == b'\n').unwrap() + 1;
    jsonl.truncate(prior);
    assert!(read_observation_mapping_jsonl(Cursor::new(jsonl)).is_err());
}
