//! Heading/paragraph-aware note chunker (ADR 0018 §4/§8).
//!
//! Splits a note body into overlapping chunks of ~256–512 tokens, aligned to
//! paragraph and heading boundaries, and emits byte offsets so results can be
//! cited back to the exact span. "Tokens" are approximated by whitespace words
//! (the real model tokenizer differs slightly, but chunk sizing only needs to
//! be roughly right; the store records exact offsets regardless).
//!
//! Per ADR 0009 the chunker never panics on untrusted content: pathological
//! input (unclosed fences, one giant paragraph, empty body) yields a
//! degraded-but-valid chunk set in bounded time. Offsets always index valid
//! UTF-8 char boundaries of the input.

/// A chunk of a note body with its byte offsets and extracted text.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkSpan {
    pub char_start: usize,
    pub char_end: usize,
    pub text: String,
}

/// Tuning for [`chunk_note`]. Sizes are in approximate tokens (whitespace words).
#[derive(Debug, Clone)]
pub struct ChunkerOptions {
    pub target_tokens: usize,
    pub max_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkerOptions {
    fn default() -> Self {
        Self {
            target_tokens: 350,
            max_tokens: 512,
            overlap_tokens: 52, // ~15% of target
        }
    }
}

/// A paragraph/heading unit within the body: a byte range plus a word count and
/// whether it begins a heading (so chunks can align to sections).
#[derive(Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    words: usize,
    heading: bool,
}

/// Chunk a note body into overlapping [`ChunkSpan`]s.
pub fn chunk_note(body: &str, opts: &ChunkerOptions) -> Vec<ChunkSpan> {
    let max_tokens = opts.max_tokens.max(1);
    let target_tokens = opts.target_tokens.clamp(1, max_tokens);
    let overlap_tokens = opts.overlap_tokens.min(target_tokens.saturating_sub(1));

    let mut units = split_units(body);
    units = split_oversized(body, units, max_tokens, target_tokens, overlap_tokens);
    if units.is_empty() {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let mut j = i;
        let mut words = 0;
        while j < units.len() {
            let w = units[j].words;
            if words > 0 && words + w > max_tokens {
                break;
            }
            // Heading-aware: a heading that isn't the first unit starts a new chunk.
            if words > 0 && units[j].heading {
                break;
            }
            words += w;
            j += 1;
            if words >= target_tokens {
                break;
            }
        }
        let start = units[i].start;
        let end = units[j - 1].end;
        spans.push(ChunkSpan {
            char_start: start,
            char_end: end,
            text: body[start..end].to_string(),
        });
        if j >= units.len() {
            break;
        }
        // Overlap: back up so the next chunk re-includes ~overlap_tokens of tail.
        let mut k = j;
        let mut ow = 0;
        while k > i {
            let w = units[k - 1].words;
            if ow + w > overlap_tokens {
                break;
            }
            ow += w;
            k -= 1;
        }
        i = if k > i { k } else { i + 1 };
    }
    spans
}

/// Split the body into paragraph/heading units by blank-line and heading
/// boundaries, recording each unit's trimmed byte range and word count.
fn split_units(body: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut open: Option<(usize, usize)> = None; // (start, content_end)

    let mut line_start = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        let content_end = line_start + line.trim_end().len();
        let is_blank = trimmed.is_empty();
        let is_heading = trimmed.starts_with('#');

        if is_blank {
            if let Some((s, e)) = open.take() {
                push_unit(body, &mut units, s, e);
            }
        } else if is_heading {
            if let Some((s, e)) = open.take() {
                push_unit(body, &mut units, s, e);
            }
            open = Some((line_start, content_end));
        } else if let Some((_, e)) = open.as_mut() {
            *e = content_end;
        } else {
            open = Some((line_start, content_end));
        }
        line_start += line.len();
    }
    if let Some((s, e)) = open.take() {
        push_unit(body, &mut units, s, e);
    }
    units
}

fn push_unit(body: &str, units: &mut Vec<Unit>, start: usize, end: usize) {
    if end <= start {
        return;
    }
    let text = &body[start..end];
    let words = text.split_whitespace().count();
    if words == 0 {
        return;
    }
    let heading = text.trim_start().starts_with('#');
    units.push(Unit {
        start,
        end,
        words,
        heading,
    });
}

/// Break any unit longer than `max_tokens` into `target`-sized word windows so
/// no single chunk blows past the budget (e.g. a wall-of-text paragraph). The
/// windows overlap by `overlap` words (stride = `target - overlap`) so the
/// overlap contract holds even inside one giant paragraph.
fn split_oversized(
    body: &str,
    units: Vec<Unit>,
    max_tokens: usize,
    target: usize,
    overlap: usize,
) -> Vec<Unit> {
    let mut out = Vec::new();
    let stride = target.saturating_sub(overlap).max(1);
    for unit in units {
        if unit.words <= max_tokens {
            out.push(unit);
            continue;
        }
        let text = &body[unit.start..unit.end];
        // Byte offsets (relative to unit.start) of each word's start and end.
        let mut word_bounds: Vec<(usize, usize)> = Vec::new();
        let mut word_start: Option<usize> = None;
        for (idx, ch) in text.char_indices() {
            if ch.is_whitespace() {
                if let Some(s) = word_start.take() {
                    word_bounds.push((s, idx));
                }
            } else if word_start.is_none() {
                word_start = Some(idx);
            }
        }
        if let Some(s) = word_start.take() {
            word_bounds.push((s, text.len()));
        }
        let n = word_bounds.len();
        let mut w = 0;
        while w < n {
            let end_word = (w + target).min(n);
            let s = unit.start + word_bounds[w].0;
            let e = unit.start + word_bounds[end_word - 1].1;
            out.push(Unit {
                start: s,
                end: e,
                words: end_word - w,
                heading: false,
            });
            if end_word >= n {
                break;
            }
            w += stride;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_opts() -> ChunkerOptions {
        ChunkerOptions {
            target_tokens: 6,
            max_tokens: 10,
            overlap_tokens: 2,
        }
    }

    #[test]
    fn empty_body_yields_no_chunks() {
        assert!(chunk_note("", &ChunkerOptions::default()).is_empty());
        assert!(chunk_note("   \n\n  \n", &ChunkerOptions::default()).is_empty());
    }

    #[test]
    fn offsets_index_back_to_body() {
        let body = "# Title\n\nAlpha beta gamma delta epsilon zeta.\n\nSecond paragraph here now.";
        let spans = chunk_note(body, &small_opts());
        assert!(!spans.is_empty());
        for span in &spans {
            assert_eq!(&body[span.char_start..span.char_end], span.text);
        }
    }

    #[test]
    fn headings_start_new_chunks() {
        let body = "# One\n\nsome words in one\n\n# Two\n\nother words in two";
        let spans = chunk_note(body, &small_opts());
        // The second heading should begin a fresh chunk.
        assert!(spans.iter().any(|s| s.text.contains("# Two")));
        let two_chunk = spans.iter().find(|s| s.text.contains("# Two")).unwrap();
        assert!(two_chunk.text.trim_start().starts_with("# Two"));
    }

    #[test]
    fn short_body_is_a_single_chunk() {
        let body = "just a few words";
        let spans = chunk_note(body, &ChunkerOptions::default());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, body);
    }

    #[test]
    fn overlap_repeats_some_tail_between_chunks() {
        let body = "one two three four five six seven eight nine ten eleven twelve";
        let opts = ChunkerOptions {
            target_tokens: 4,
            max_tokens: 6,
            overlap_tokens: 2,
        };
        let spans = chunk_note(body, &opts);
        assert!(spans.len() >= 2, "expected multiple chunks");
        // Consecutive chunks should share at least one word due to overlap.
        for pair in spans.windows(2) {
            let a: Vec<&str> = pair[0].text.split_whitespace().collect();
            let b: Vec<&str> = pair[1].text.split_whitespace().collect();
            assert!(
                a.iter().rev().take(2).any(|w| b.contains(w)),
                "chunks should overlap: {a:?} / {b:?}"
            );
        }
    }

    #[test]
    fn giant_paragraph_is_split_under_max() {
        let body = (0..100)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let opts = ChunkerOptions {
            target_tokens: 20,
            max_tokens: 25,
            overlap_tokens: 3,
        };
        let spans = chunk_note(&body, &opts);
        assert!(spans.len() > 1);
        for span in &spans {
            let words = span.text.split_whitespace().count();
            assert!(words <= 25, "chunk exceeded max: {words}");
            assert_eq!(&body[span.char_start..span.char_end], span.text);
        }
    }

    #[test]
    fn unclosed_fence_does_not_panic() {
        let body = "# Note\n\n```rust\nfn main() {\n    // never closed\n\nmore text {{ nested";
        let spans = chunk_note(body, &ChunkerOptions::default());
        for span in &spans {
            assert_eq!(&body[span.char_start..span.char_end], span.text);
        }
    }
}
