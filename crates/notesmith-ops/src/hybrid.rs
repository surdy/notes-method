//! Hybrid lexical + semantic search via Reciprocal Rank Fusion (RRF).
//!
//! ADR 0018 §8 settled on **RRF** (over a tunable weighted blend) to combine
//! the Tantivy/BM25 lexical ranking with vector-similarity ranking: it needs no
//! score calibration between two incomparable scales, is robust to outliers,
//! and has a single, well-understood constant `k` (default 60).
//!
//! `rrf_fuse` is a pure function over the two rankers' *ordered* result lists so
//! it can be unit-tested exhaustively. [`HybridSearch`] wires it to the live
//! [`SearchIndex`] and [`EmbeddingSearch`], collapsing per-chunk semantic hits
//! to one entry per note and enriching semantic-only notes with a snippet read
//! from the chunk span on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use notesmith_embed::{EmbeddingSearch, MetaFilter, ScoredChunk};
use notesmith_index::{SearchIndex, SearchResult};
use serde::Serialize;

/// Default RRF smoothing constant (ADR 0018 §8).
pub const DEFAULT_RRF_K: usize = 60;

/// A fused search hit, grounded by a path + snippet for agent citation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HybridHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
    /// Combined RRF score (higher is better).
    pub score: f32,
    /// 1-based rank in the lexical ranker, if it appeared.
    pub lexical_rank: Option<usize>,
    /// 1-based rank in the semantic ranker (after chunk→note dedup), if any.
    pub semantic_rank: Option<usize>,
    /// Char offsets of the best matching chunk, for precise citation.
    pub char_start: Option<i64>,
    pub char_end: Option<i64>,
}

/// Collapse per-chunk semantic hits to the best (nearest) chunk per note,
/// preserving nearest-first order. Returns `(path, best_chunk)` in rank order.
fn dedup_semantic(semantic: &[ScoredChunk]) -> Vec<&ScoredChunk> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for hit in semantic {
        if seen.insert(hit.chunk.path.clone()) {
            out.push(hit);
        }
    }
    out
}

/// Pure Reciprocal Rank Fusion of a lexical and a semantic result list.
///
/// Each list is assumed to be in descending relevance order. A document's score
/// is `sum over rankers of 1 / (k + rank)` with 1-based ranks. Semantic hits are
/// collapsed to one entry per note (best chunk) before ranking.
pub fn rrf_fuse(
    lexical: &[SearchResult],
    semantic: &[ScoredChunk],
    k: usize,
    limit: usize,
) -> Vec<HybridHit> {
    let mut acc: HashMap<String, HybridHit> = HashMap::new();

    for (idx, res) in lexical.iter().enumerate() {
        let rank = idx + 1;
        let entry = acc.entry(res.path.clone()).or_insert_with(|| HybridHit {
            path: res.path.clone(),
            title: res.title.clone(),
            snippet: res.snippet.clone(),
            score: 0.0,
            lexical_rank: None,
            semantic_rank: None,
            char_start: None,
            char_end: None,
        });
        entry.score += 1.0 / (k as f32 + rank as f32);
        entry.lexical_rank = Some(rank);
        if entry.title.is_empty() {
            entry.title = res.title.clone();
        }
        if entry.snippet.is_empty() {
            entry.snippet = res.snippet.clone();
        }
    }

    for (idx, hit) in dedup_semantic(semantic).into_iter().enumerate() {
        let rank = idx + 1;
        let entry = acc
            .entry(hit.chunk.path.clone())
            .or_insert_with(|| HybridHit {
                path: hit.chunk.path.clone(),
                title: String::new(),
                snippet: String::new(),
                score: 0.0,
                lexical_rank: None,
                semantic_rank: None,
                char_start: None,
                char_end: None,
            });
        entry.score += 1.0 / (k as f32 + rank as f32);
        entry.semantic_rank = Some(rank);
        entry.char_start = Some(hit.chunk.char_start);
        entry.char_end = Some(hit.chunk.char_end);
    }

    let mut hits: Vec<HybridHit> = acc.into_values().collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    hits.truncate(limit);
    hits
}

/// Live hybrid search over a vault: fuses [`SearchIndex`] (lexical) and
/// [`EmbeddingSearch`] (semantic) via [`rrf_fuse`].
pub struct HybridSearch {
    search_index: Arc<SearchIndex>,
    embedding: Arc<EmbeddingSearch>,
    vault_root: PathBuf,
    k: usize,
}

impl HybridSearch {
    pub fn new(
        search_index: Arc<SearchIndex>,
        embedding: Arc<EmbeddingSearch>,
        vault_root: PathBuf,
    ) -> Self {
        Self {
            search_index,
            embedding,
            vault_root,
            k: DEFAULT_RRF_K,
        }
    }

    /// Run both rankers and fuse. `limit` bounds the final result count; each
    /// underlying ranker is queried a little deeper so fusion has candidates.
    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<HybridHit>> {
        let depth = (limit * 4).max(20);
        let lexical = self.search_index.search(query, depth)?;
        let semantic = self
            .embedding
            .search(query, depth, &MetaFilter::default())
            .map_err(|e| anyhow::anyhow!("semantic search failed: {e}"))?;

        let mut hits = rrf_fuse(&lexical, &semantic, self.k, limit);
        for hit in &mut hits {
            if hit.snippet.is_empty() {
                hit.snippet = self.snippet_from_span(hit);
            }
        }
        Ok(hits)
    }

    /// Read a snippet from the note's chunk span for a semantic-only hit.
    fn snippet_from_span(&self, hit: &HybridHit) -> String {
        let (Some(start), Some(end)) = (hit.char_start, hit.char_end) else {
            return String::new();
        };
        let path = self.vault_root.join(&hit.path);
        read_span(&path, start as usize, end as usize)
    }
}

/// Read `[start, end)` chars from a UTF-8 file, best-effort and panic-free.
fn read_span(path: &Path, start: usize, end: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let snippet: String = content
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();
    let trimmed = snippet.trim();
    const MAX: usize = 280;
    if trimmed.chars().count() > MAX {
        trimmed.chars().take(MAX).collect::<String>() + "…"
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_embed::ChunkRef;

    fn lex(path: &str, title: &str, snippet: &str) -> SearchResult {
        SearchResult {
            vault_name: "v".into(),
            path: path.into(),
            title: title.into(),
            note_type: "note".into(),
            score: 1.0,
            snippet: snippet.into(),
        }
    }

    fn sem(path: &str, chunk_id: i64, distance: f32) -> ScoredChunk {
        ScoredChunk {
            chunk: ChunkRef {
                vault_name: "v".into(),
                path: path.into(),
                chunk_id,
                char_start: 0,
                char_end: 10,
                media_ts_start: None,
                media_ts_end: None,
                content_hash: "h".into(),
            },
            distance,
        }
    }

    #[test]
    fn fuses_overlapping_and_disjoint_hits() {
        let lexical = vec![lex("a.md", "A", "alpha"), lex("b.md", "B", "beta")];
        let semantic = vec![sem("b.md", 0, 0.1), sem("c.md", 0, 0.2)];

        let hits = rrf_fuse(&lexical, &semantic, DEFAULT_RRF_K, 10);
        // b.md appears in both rankers → should rank first.
        assert_eq!(hits[0].path, "b.md");
        assert!(hits[0].lexical_rank.is_some() && hits[0].semantic_rank.is_some());
        // a.md and c.md each appear once.
        let paths: Vec<_> = hits.iter().map(|h| h.path.as_str()).collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"c.md"));
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn rrf_score_matches_formula() {
        let lexical = vec![lex("x.md", "X", "s")];
        let semantic = vec![sem("x.md", 0, 0.0)];
        let hits = rrf_fuse(&lexical, &semantic, 60, 10);
        // rank 1 in both rankers: 1/61 + 1/61.
        let expected = 1.0 / 61.0 + 1.0 / 61.0;
        assert!((hits[0].score - expected).abs() < 1e-6);
    }

    #[test]
    fn collapses_chunks_to_best_per_note() {
        // Two chunks of the same note; nearer one (first) wins the span.
        let semantic = vec![sem("n.md", 0, 0.05), sem("n.md", 1, 0.5)];
        let hits = rrf_fuse(&[], &semantic, 60, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].semantic_rank, Some(1));
    }

    #[test]
    fn respects_limit_and_stable_order() {
        let lexical = vec![lex("a.md", "A", "s"), lex("b.md", "B", "s")];
        let hits = rrf_fuse(&lexical, &[], 60, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
    }

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(rrf_fuse(&[], &[], 60, 10).is_empty());
    }
}
