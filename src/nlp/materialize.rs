//! Blob materialization: turn a (parsed) working-tree file into cached chunks,
//! summaries, and vectors keyed by its git blob OID.
//!
//! Mirrors the per-file extraction the old `index_file_group` did, but writes the
//! content-addressed schema: embeddings are skipped for component texts already
//! cached (by content hash), and a blob whose OID is already present is never
//! re-materialized. A group is materialized together so embedding batches well.

use std::collections::BTreeSet;

use crate::analyzer::{IAnalyzer, ProjectFile};

use super::bm25::fts_text;
use super::chunker::extract_file_chunks;
use super::engine::Embedder;
use super::keys::{Key, component_key, compose, composed_key};
use super::metrics;
use super::store::{BlobChunkIn, SemanticStore};

/// A working-tree file paired with the blob OID it currently resolves to.
pub struct BlobTarget {
    pub file: ProjectFile,
    pub oid: String,
    pub language: Option<String>,
}

struct PendingChunk {
    chunk_ord: i64,
    kind: &'static str,
    symbol: Option<String>,
    start_line: Option<i64>,
    end_line: Option<i64>,
    fts_tokens: String,
    hash: Key,
    parent_summary_hash: Option<Key>,
    composed_hash: Key,
}

struct PendingBlob {
    oid: String,
    language: Option<String>,
    chunks: Vec<PendingChunk>,
}

/// Phase 1 of materialization (CPU only): tree-sitter extraction + content hashing.
/// Carries no store/embedder handles, so it can run on a producer thread ahead of
/// the GPU embed (see the indexer pipeline). `count_tokens` uses the embedder's
/// tokenizer (cheap, CPU) to size chunks.
pub struct ExtractedGroup {
    pending_blobs: Vec<PendingBlob>,
    component_texts: Vec<(Key, String)>,
}

/// Materialize a group of blobs: extract + embed only what the cache is missing,
/// then persist each blob's chunks. Caller should pre-filter to blobs whose OID
/// is not already present (see `SemanticStore::missing_blobs`).
pub fn materialize_blobs(
    store: &SemanticStore,
    embedder: &dyn Embedder,
    analyzer: &dyn IAnalyzer,
    group: &[BlobTarget],
) -> Result<(), String> {
    finish_group(store, embedder, extract_group(embedder, analyzer, group))
}

/// The distinct component texts a group of files would embed (chunk bodies +
/// parent summaries), in extraction order. Diagnostic helper for embed-stage
/// profiling; uses placeholder OIDs since only the texts are needed.
pub fn extract_group_texts(
    embedder: &dyn Embedder,
    analyzer: &dyn IAnalyzer,
    files: &[ProjectFile],
) -> Vec<String> {
    let targets: Vec<BlobTarget> = files
        .iter()
        .map(|file| BlobTarget {
            file: file.clone(),
            oid: String::new(),
            language: None,
        })
        .collect();
    extract_group(embedder, analyzer, &targets)
        .component_texts
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// Phase 1 (CPU): extract chunks and the distinct component texts to embed.
pub fn extract_group(
    embedder: &dyn Embedder,
    analyzer: &dyn IAnalyzer,
    group: &[BlobTarget],
) -> ExtractedGroup {
    let count_tokens = |text: &str| embedder.count_tokens(text);

    let mut pending_blobs: Vec<PendingBlob> = Vec::with_capacity(group.len());
    let mut component_texts: Vec<(Key, String)> = Vec::new();
    let mut seen_components: BTreeSet<Key> = BTreeSet::new();

    for target in group {
        metrics::trace(format_args!("extract file {}", target.file.rel_path().display()));
        let extracted = extract_file_chunks(analyzer, &target.file, &count_tokens);
        metrics::trace(format_args!(
            "extract done {} ({} chunks)",
            target.file.rel_path().display(),
            extracted.chunks.len()
        ));
        let mut chunks = Vec::with_capacity(extracted.chunks.len());
        for chunk in extracted.chunks {
            let hash = component_key(&chunk.text);
            if seen_components.insert(hash) {
                component_texts.push((hash, chunk.text.clone()));
            }
            let parent_hash = chunk.parent_text.as_deref().map(component_key);
            if let (Some(key), Some(text)) = (parent_hash, chunk.parent_text.as_deref())
                && seen_components.insert(key)
            {
                component_texts.push((key, text.to_string()));
            }
            let composed_hash = match parent_hash {
                Some(parent) => composed_key(&hash, &parent),
                None => hash,
            };
            chunks.push(PendingChunk {
                chunk_ord: chunk.ord,
                kind: chunk.kind.as_str(),
                symbol: chunk.symbol,
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                fts_tokens: fts_text(&chunk.text),
                hash,
                parent_summary_hash: parent_hash,
                composed_hash,
            });
        }
        metrics::trace(format_args!(
            "fts/hash done {}",
            target.file.rel_path().display()
        ));
        pending_blobs.push(PendingBlob {
            oid: target.oid.clone(),
            language: target.language.clone(),
            chunks,
        });
    }

    ExtractedGroup {
        pending_blobs,
        component_texts,
    }
}

/// Phases 2-4 (GPU embed + DB writes): embed the missing components, compose the
/// missing chunk vectors, and persist chunk metadata.
pub fn finish_group(
    store: &SemanticStore,
    embedder: &dyn Embedder,
    extracted: ExtractedGroup,
) -> Result<(), String> {
    let ExtractedGroup {
        pending_blobs,
        component_texts,
    } = extracted;

    // 2. Embed component texts the store has never seen.
    let all_component_keys: Vec<Key> = component_texts.iter().map(|(key, _)| *key).collect();
    let missing: BTreeSet<Key> = metrics::time(&metrics::SQLITE_NS, || {
        store.missing_component_hashes(&all_component_keys)
    })
    .map_err(|e| e.to_string())?
    .into_iter()
    .collect();
    let to_embed: Vec<&(Key, String)> = component_texts
        .iter()
        .filter(|(key, _)| missing.contains(key))
        .collect();
    if !to_embed.is_empty() {
        let texts: Vec<&str> = to_embed.iter().map(|(_, text)| text.as_str()).collect();
        let max_bytes = texts.iter().map(|t| t.len()).max().unwrap_or(0);
        let vectors = metrics::traced(
            &metrics::EMBED_NS,
            format_args!("embed {} texts (max_bytes={max_bytes})", texts.len()),
            || embedder.embed_passages(&texts),
        )?;
        let items: Vec<(Key, Vec<f32>)> =
            to_embed.iter().map(|(key, _)| *key).zip(vectors).collect();
        metrics::trace(format_args!("upsert_component {} vectors", items.len()));
        store
            .upsert_component_vectors(&items)
            .map_err(|e| e.to_string())?;
    }

    // 3. Compose missing chunk vectors from their (now cached) components.
    let composed_keys: Vec<Key> = pending_blobs
        .iter()
        .flat_map(|blob| blob.chunks.iter().map(|chunk| chunk.composed_hash))
        .collect();
    let missing_composed: BTreeSet<Key> = metrics::time(&metrics::SQLITE_NS, || {
        store.missing_composed_hashes(&composed_keys)
    })
    .map_err(|e| e.to_string())?
    .into_iter()
    .collect();
    if !missing_composed.is_empty() {
        let mut needed: BTreeSet<Key> = BTreeSet::new();
        for blob in &pending_blobs {
            for chunk in &blob.chunks {
                if missing_composed.contains(&chunk.composed_hash) {
                    needed.insert(chunk.hash);
                    if let Some(parent) = chunk.parent_summary_hash {
                        needed.insert(parent);
                    }
                }
            }
        }
        let component_vectors = metrics::time(&metrics::SQLITE_NS, || {
            store.component_vectors(&needed.iter().copied().collect::<Vec<_>>())
        })
        .map_err(|e| e.to_string())?;
        let mut composed_items: Vec<(Key, Vec<f32>)> = Vec::new();
        let mut emitted: BTreeSet<Key> = BTreeSet::new();
        metrics::trace(format_args!("compose {} vectors", missing_composed.len()));
        metrics::time(&metrics::COMPOSE_NS, || -> Result<(), String> {
            for blob in &pending_blobs {
                for chunk in &blob.chunks {
                    if !missing_composed.contains(&chunk.composed_hash)
                        || !emitted.insert(chunk.composed_hash)
                    {
                        continue;
                    }
                    let child = component_vectors
                        .get(&chunk.hash)
                        .ok_or_else(|| "component vector missing after embed".to_string())?;
                    let vector = match chunk.parent_summary_hash {
                        Some(parent) => {
                            let parent_vec = component_vectors
                                .get(&parent)
                                .ok_or_else(|| "parent vector missing after embed".to_string())?;
                            compose(child, parent_vec)
                        }
                        None => child.clone(),
                    };
                    composed_items.push((chunk.composed_hash, vector));
                }
            }
            Ok(())
        })?;
        metrics::trace(format_args!("upsert_composed {} vectors", composed_items.len()));
        store
            .upsert_composed_vectors(&composed_items)
            .map_err(|e| e.to_string())?;
    }

    // 4. Persist each blob's chunk metadata.
    metrics::trace(format_args!("put_blobs ({} blobs)", pending_blobs.len()));
    for blob in &pending_blobs {
        let rows: Vec<BlobChunkIn> = blob
            .chunks
            .iter()
            .map(|chunk| BlobChunkIn {
                chunk_ord: chunk.chunk_ord,
                kind: chunk.kind,
                symbol: chunk.symbol.as_deref(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                fts_tokens: &chunk.fts_tokens,
                hash: chunk.hash,
                parent_summary_hash: chunk.parent_summary_hash,
                composed_hash: chunk.composed_hash,
            })
            .collect();
        metrics::time(&metrics::SQLITE_NS, || {
            store.put_blob(&blob.oid, blob.language.as_deref(), &rows)
        })
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}
