//! Note-to-note relatedness scoring for the Relevant Notes panel (issue #201).
//!
//! Relatedness blends two orthogonal signals:
//!
//! * **Embedding similarity** — cosine similarity between the active note's and
//!   a candidate's mean chunk vector (centroid). Only available when the vault
//!   has embeddings; absent otherwise.
//! * **Link-graph proximity** — a direct link in either direction, plus
//!   *shared neighbours* (co-citation: notes cited by the same note as the
//!   active one; and bibliographic coupling: notes that cite the same targets
//!   as the active one).
//!
//! [`rank_related`] is a pure function over pre-computed [`CandidateSignals`] so
//! the ranking/blending is unit-testable without a database or an embedder. The
//! data-gathering half lives in `LocalOps::related_notes`.

use std::cmp::Ordering;

/// Weight of embedding similarity in the blended score (when embeddings exist).
const EMBED_WEIGHT: f32 = 0.65;
/// Weight of the normalized link-graph score in the blended score.
const GRAPH_WEIGHT: f32 = 0.35;
/// Raw graph points awarded for a direct link (either direction).
const DIRECT_LINK_POINTS: f32 = 1.0;
/// Raw graph points awarded per shared neighbour.
const SHARED_NEIGHBOR_POINTS: f32 = 0.5;

/// Pre-computed relatedness signals for one candidate note.
#[derive(Debug, Clone)]
pub struct CandidateSignals {
    pub path: String,
    pub title: String,
    /// Cosine similarity to the active note, or `None` when embeddings are
    /// unavailable or the candidate has no stored vector.
    pub embedding_similarity: Option<f32>,
    /// Whether the active note and this candidate link to each other directly.
    pub directly_linked: bool,
    /// Count of shared neighbours (co-citation + bibliographic coupling).
    pub shared_neighbors: u32,
}

/// A ranked related note with its blended score and contributing signals.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedNote {
    pub path: String,
    pub title: String,
    pub score: f32,
    pub embedding_similarity: Option<f32>,
    pub directly_linked: bool,
    pub shared_neighbors: u32,
}

/// Cosine similarity between two equal-length vectors. Returns `0.0` for
/// mismatched or zero-magnitude vectors rather than `NaN`.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Raw (un-normalized) link-graph score for a candidate.
fn graph_raw(signals: &CandidateSignals) -> f32 {
    let direct = if signals.directly_linked {
        DIRECT_LINK_POINTS
    } else {
        0.0
    };
    direct + SHARED_NEIGHBOR_POINTS * signals.shared_neighbors as f32
}

/// Blend the signals into a ranked list of related notes.
///
/// When `embeddings_used` is false the graph score alone drives ranking
/// (graph-only degradation). The graph score is min-max normalized against the
/// strongest candidate so it shares the `[0, 1]` range with embedding
/// similarity before weighting. Candidates with no positive signal are dropped.
pub fn rank_related(
    candidates: Vec<CandidateSignals>,
    embeddings_used: bool,
    limit: usize,
) -> Vec<RelatedNote> {
    let max_graph = candidates.iter().map(graph_raw).fold(0.0f32, f32::max);

    let mut scored: Vec<RelatedNote> = candidates
        .into_iter()
        .map(|c| {
            let g_norm = if max_graph > 0.0 {
                graph_raw(&c) / max_graph
            } else {
                0.0
            };
            let embed = c.embedding_similarity.map(|s| s.clamp(0.0, 1.0));
            let score = if embeddings_used {
                EMBED_WEIGHT * embed.unwrap_or(0.0) + GRAPH_WEIGHT * g_norm
            } else {
                g_norm
            };
            RelatedNote {
                path: c.path,
                title: c.title,
                score,
                embedding_similarity: embed,
                directly_linked: c.directly_linked,
                shared_neighbors: c.shared_neighbors,
            }
        })
        .filter(|r| r.score > 0.0)
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                b.embedding_similarity
                    .unwrap_or(0.0)
                    .partial_cmp(&a.embedding_similarity.unwrap_or(0.0))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(path: &str, embed: Option<f32>, direct: bool, shared: u32) -> CandidateSignals {
        CandidateSignals {
            path: path.to_string(),
            title: path.to_string(),
            embedding_similarity: embed,
            directly_linked: direct,
            shared_neighbors: shared,
        }
    }

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
    }

    #[test]
    fn cosine_handles_mismatched_and_zero_vectors() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn embedding_dominates_but_graph_breaks_close_scores() {
        // A: high embedding, no links. B: lower embedding but directly linked.
        let ranked = rank_related(
            vec![
                signals("A.md", Some(0.9), false, 0),
                signals("B.md", Some(0.2), true, 2),
            ],
            true,
            10,
        );
        assert_eq!(ranked.len(), 2);
        // A's embedding (0.65*0.9=0.585) still beats B (0.65*0.2+0.35*1.0=0.48).
        assert_eq!(ranked[0].path, "A.md");
        assert_eq!(ranked[1].path, "B.md");
    }

    #[test]
    fn graph_only_when_embeddings_absent() {
        let ranked = rank_related(
            vec![
                signals("A.md", None, false, 1),
                signals("B.md", None, true, 0),
            ],
            false,
            10,
        );
        // Direct link (raw 1.0) outranks a single shared neighbour (raw 0.5).
        assert_eq!(ranked[0].path, "B.md");
        assert_eq!(ranked[1].path, "A.md");
        assert!((ranked[0].score - 1.0).abs() < 1e-6);
        assert!((ranked[1].score - 0.5).abs() < 1e-6);
    }

    #[test]
    fn drops_candidates_with_no_signal() {
        let ranked = rank_related(
            vec![
                signals("A.md", Some(0.0), false, 0),
                signals("B.md", Some(0.5), false, 0),
            ],
            true,
            10,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].path, "B.md");
    }

    #[test]
    fn respects_limit() {
        let ranked = rank_related(
            vec![
                signals("A.md", Some(0.9), false, 0),
                signals("B.md", Some(0.8), false, 0),
                signals("C.md", Some(0.7), false, 0),
            ],
            true,
            2,
        );
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].path, "A.md");
        assert_eq!(ranked[1].path, "B.md");
    }
}
