//! Semantic code search: parent-averaged function embeddings + grounded-strings
//! BM25 + git co-edit relevance, returned as independent retrieval signals.
//! Reranking happens downstream, outside this crate.
//!
//! The design (and every tuned constant below) is ported from the brokkbench
//! localizer prototype; see `analysis/{bm25,coedit-reranker}/REPORT.md`
//! there for the sweeps that selected these values.

pub mod bm25;
pub mod chunker;
pub mod engine;
pub mod indexer;
pub mod keys;
pub mod query;
pub mod store;

/// Weight of the chunk vector when averaging with its parent context vector.
pub const PARENT_ALPHA: f64 = 0.5;

/// Token budget for any single embedded text (chunk, summary, or symbols list).
pub const MAX_SEQ_TOKENS: usize = 8192;

/// Reciprocal-rank smoothing constant for the positional co-edit score.
pub const RRF_K: f64 = 30.0;

/// Recency half-life (commits) passed to most_relevant_files.
pub const COEDIT_HALF_LIFE: f64 = 250.0;

/// Cap on distinct BM25 query tokens.
pub const MAX_QUERY_TOKENS: usize = 256;

/// Asymmetric query/passage prefixes. Applied exactly once, only inside the
/// `Embedder` impls, so indexed text never carries a prefix. These match the
/// granite localizer fine-tune's training prefixes and are part of the
/// embedding fingerprint: changing them invalidates cached vectors.
pub const QUERY_PREFIX: &str =
    "Given a GitHub issue, retrieve code that must be changed to fix it.\nQuery: ";
pub const PASSAGE_PREFIX: &str = "Passage: Code chunk from repository.\n";

/// Versioned contracts shared with the prototype's vector cache key recipe.
pub const COMPONENT_CONTRACT_VERSION: &str = "component_v1";
pub const REPRESENTATION_KIND: &str = "parent_avg_v1";

/// Bump when the BM25 tokenizer changes; stored in the index meta table.
pub const BM25_TOKENIZER_VERSION: &str = "code-subtoken-v1";

/// Bump when chunk extraction or parent-text derivation changes.
pub const CHUNKER_VERSION: &str = "chunker_v1";
