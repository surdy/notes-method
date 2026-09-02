//! Deterministic, byte-preserving updates to **managed sections**.
//!
//! A managed section is a machine-owned region inside a human-owned note,
//! delimited by a pair of HTML-comment markers (see `docs/managed-sections.md`
//! and ADR 0025 Decision 5):
//!
//! ```markdown
//! <!-- notesmith:section:begin briefing/meetings -->
//! - 09:30 standup
//! <!-- notesmith:section:end briefing/meetings -->
//! ```
//!
//! The convention used to be enforced only by prompt guidance: an agent read
//! the whole note, spliced new content between the markers, and wrote the whole
//! note back. Real-machine verification showed that cannot guarantee byte
//! preservation — a compliant agent still stripped trailing spaces from human
//! text outside the markers. [`update_managed_section`] replaces that with a
//! deterministic transform: it locates the marker pair, replaces **only** the
//! interior byte range, and copies every other byte through untouched.
//!
//! This module is pure string surgery — no IO, no frontmatter parsing, no
//! normalization. Callers must **not** run the save pipeline over the result:
//! doing so would restamp `updated:` and trim trailing whitespace, which is
//! exactly what the contract forbids.

/// Opening delimiter of a begin marker, before the section id.
pub const BEGIN_MARKER_PREFIX: &str = "<!-- notesmith:section:begin ";
/// Opening delimiter of an end marker, before the section id.
pub const END_MARKER_PREFIX: &str = "<!-- notesmith:section:end ";
/// Closing delimiter shared by both markers.
pub const MARKER_SUFFIX: &str = " -->";

/// Render the begin marker line for `section_id` (without a line terminator).
pub fn begin_marker(section_id: &str) -> String {
    format!("{BEGIN_MARKER_PREFIX}{section_id}{MARKER_SUFFIX}")
}

/// Render the end marker line for `section_id` (without a line terminator).
pub fn end_marker(section_id: &str) -> String {
    format!("{END_MARKER_PREFIX}{section_id}{MARKER_SUFFIX}")
}

/// Why a managed-section update could not be performed.
///
/// Every variant is a refusal: the note is never partially rewritten, and the
/// caller is expected to surface the failure rather than fall back to a
/// whole-note write.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManagedSectionError {
    /// The section id is empty or would not round-trip through a marker line.
    #[error("invalid managed-section id {section_id:?}: {reason}")]
    InvalidSectionId {
        section_id: String,
        reason: &'static str,
    },
    /// Neither marker for the section is present and `append_if_missing` was false.
    #[error("no managed section `{section_id}` in the note")]
    SectionNotFound { section_id: String },
    /// The begin marker appears more than once.
    #[error(
        "managed section `{section_id}` has duplicate begin markers (lines {first} and {second})"
    )]
    DuplicateBeginMarker {
        section_id: String,
        first: usize,
        second: usize,
    },
    /// The end marker appears more than once.
    #[error(
        "managed section `{section_id}` has duplicate end markers (lines {first} and {second})"
    )]
    DuplicateEndMarker {
        section_id: String,
        first: usize,
        second: usize,
    },
    /// The end marker precedes the begin marker.
    #[error(
        "managed section `{section_id}` is inverted: end marker on line {end_line} precedes begin marker on line {begin_line}"
    )]
    InvertedMarkers {
        section_id: String,
        begin_line: usize,
        end_line: usize,
    },
    /// A begin marker with no matching end marker.
    #[error("managed section `{section_id}` has a begin marker on line {line} with no end marker")]
    MissingEndMarker { section_id: String, line: usize },
    /// An end marker with no matching begin marker.
    #[error("managed section `{section_id}` has an end marker on line {line} with no begin marker")]
    MissingBeginMarker { section_id: String, line: usize },
    /// The replacement content itself contains a marker line. Writing it would
    /// corrupt the section: a same-id marker makes the next update fail as a
    /// duplicate, and a different-id marker plants a phantom section that
    /// another automation could claim.
    #[error(
        "managed section `{section_id}`: content line {line} is a section marker; content must not contain marker lines"
    )]
    ContentContainsMarker { section_id: String, line: usize },
}

impl ManagedSectionError {
    /// A stable machine-readable code for this failure, for API/tool clients
    /// that need to branch on the kind of malformed layout.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidSectionId { .. } => "invalid_section_id",
            Self::SectionNotFound { .. } => "section_not_found",
            Self::DuplicateBeginMarker { .. } => "duplicate_begin_marker",
            Self::DuplicateEndMarker { .. } => "duplicate_end_marker",
            Self::InvertedMarkers { .. } => "inverted_markers",
            Self::MissingEndMarker { .. } => "missing_end_marker",
            Self::MissingBeginMarker { .. } => "missing_begin_marker",
            Self::ContentContainsMarker { .. } => "content_contains_marker",
        }
    }

    /// The section id the failure refers to.
    pub fn section_id(&self) -> &str {
        match self {
            Self::InvalidSectionId { section_id, .. }
            | Self::SectionNotFound { section_id }
            | Self::DuplicateBeginMarker { section_id, .. }
            | Self::DuplicateEndMarker { section_id, .. }
            | Self::InvertedMarkers { section_id, .. }
            | Self::MissingEndMarker { section_id, .. }
            | Self::MissingBeginMarker { section_id, .. }
            | Self::ContentContainsMarker { section_id, .. } => section_id,
        }
    }
}

/// What [`update_managed_section`] did to the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSectionUpdate {
    /// The complete new note content.
    pub content: String,
    /// True when the marker pair was absent and a whole block was appended.
    pub appended: bool,
    /// False when the update was a no-op (the new bytes equal the old bytes).
    pub changed: bool,
}

/// Replace the interior of the `section_id` managed section in `note` with
/// `content`, preserving every other byte of the note exactly.
///
/// Guarantees:
///
/// - Bytes before the begin-marker line and after the end-marker line are
///   copied verbatim — trailing whitespace, tabs, mixed CRLF/LF line endings,
///   malformed HTML comments and YAML frontmatter all survive untouched.
/// - `content` is written as-is, with a single `\n` appended when it is
///   non-empty and does not already end in a newline (so the end marker keeps
///   its own line). An empty `content` collapses the interior to nothing.
/// - Re-running with identical `content` is a byte-level no-op (idempotent).
/// - Only the one marker pair is touched, so other managed sections in the same
///   note cannot change.
///
/// When the pair is absent and `append_if_missing` is true, one complete block
/// (begin marker, content, end marker) is appended at EOF, separated from the
/// existing final content by exactly one blank line. Existing bytes are still
/// only ever appended to, never rewritten — so a note that already ends in
/// several blank lines keeps them.
pub fn update_managed_section(
    note: &str,
    section_id: &str,
    content: &str,
    append_if_missing: bool,
) -> Result<ManagedSectionUpdate, ManagedSectionError> {
    validate_section_id(section_id)?;
    validate_content(section_id, content)?;

    match locate_section(note, section_id) {
        Ok(Some(span)) => {
            let mut updated =
                String::with_capacity(note.len() + content.len().saturating_add(2));
            updated.push_str(&note[..span.interior_start]);
            updated.push_str(&interior_block(content));
            updated.push_str(&note[span.interior_end..]);
            let changed = updated != note;
            Ok(ManagedSectionUpdate {
                content: updated,
                appended: false,
                changed,
            })
        }
        Ok(None) => {
            if !append_if_missing {
                return Err(ManagedSectionError::SectionNotFound {
                    section_id: section_id.to_string(),
                });
            }
            Ok(ManagedSectionUpdate {
                content: append_block(note, section_id, content),
                appended: true,
                changed: true,
            })
        }
        Err(error) => Err(error),
    }
}

fn validate_section_id(section_id: &str) -> Result<(), ManagedSectionError> {
    let invalid = if section_id.is_empty() {
        Some("must not be empty")
    } else if section_id.trim() != section_id {
        Some("must not start or end with whitespace")
    } else if section_id.contains(['\n', '\r']) {
        Some("must not contain line breaks")
    } else if section_id.contains("--") {
        Some("must not contain `--` (it would terminate the HTML comment)")
    } else {
        None
    };

    match invalid {
        Some(reason) => Err(ManagedSectionError::InvalidSectionId {
            section_id: section_id.to_string(),
            reason,
        }),
        None => Ok(()),
    }
}

/// Reject content that carries marker lines of its own (for any section id):
/// written through, a same-id marker makes every later update fail as a
/// duplicate, and a different-id marker plants a phantom section.
fn validate_content(section_id: &str, content: &str) -> Result<(), ManagedSectionError> {
    for (index, line) in content.lines().enumerate() {
        let text = line.trim();
        if text.starts_with(BEGIN_MARKER_PREFIX) || text.starts_with(END_MARKER_PREFIX) {
            return Err(ManagedSectionError::ContentContainsMarker {
                section_id: section_id.to_string(),
                line: index + 1,
            });
        }
    }
    Ok(())
}

/// Byte range strictly between the marker lines: from the first byte after the
/// begin-marker line's terminator to the first byte of the end-marker line.
struct SectionSpan {
    interior_start: usize,
    interior_end: usize,
}

fn locate_section(
    note: &str,
    section_id: &str,
) -> Result<Option<SectionSpan>, ManagedSectionError> {
    let begin = begin_marker(section_id);
    let end = end_marker(section_id);

    let mut begins: Vec<usize> = Vec::new();
    let mut ends: Vec<usize> = Vec::new();
    let lines = split_lines(note);

    for (index, line) in lines.iter().enumerate() {
        // A marker owns its whole line; surrounding whitespace (including a
        // lone `\r` from a CRLF note) is tolerated but never rewritten.
        let text = line.text.trim();
        if text == begin {
            begins.push(index);
        } else if text == end {
            ends.push(index);
        }
    }

    if begins.len() > 1 {
        return Err(ManagedSectionError::DuplicateBeginMarker {
            section_id: section_id.to_string(),
            first: begins[0] + 1,
            second: begins[1] + 1,
        });
    }
    if ends.len() > 1 {
        return Err(ManagedSectionError::DuplicateEndMarker {
            section_id: section_id.to_string(),
            first: ends[0] + 1,
            second: ends[1] + 1,
        });
    }

    match (begins.first(), ends.first()) {
        (None, None) => Ok(None),
        (Some(begin_index), None) => Err(ManagedSectionError::MissingEndMarker {
            section_id: section_id.to_string(),
            line: begin_index + 1,
        }),
        (None, Some(end_index)) => Err(ManagedSectionError::MissingBeginMarker {
            section_id: section_id.to_string(),
            line: end_index + 1,
        }),
        (Some(begin_index), Some(end_index)) if end_index < begin_index => {
            Err(ManagedSectionError::InvertedMarkers {
                section_id: section_id.to_string(),
                begin_line: begin_index + 1,
                end_line: end_index + 1,
            })
        }
        (Some(begin_index), Some(end_index)) => Ok(Some(SectionSpan {
            interior_start: lines[*begin_index].next_start,
            interior_end: lines[*end_index].start,
        })),
    }
}

struct LineSpan<'a> {
    /// Byte offset of the first character of the line.
    start: usize,
    /// Byte offset of the first character of the following line (or EOF).
    next_start: usize,
    /// The line without its `\n` / `\r\n` terminator.
    text: &'a str,
}

fn split_lines(content: &str) -> Vec<LineSpan<'_>> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let bytes = content.as_bytes();

    while start < content.len() {
        match content[start..].find('\n') {
            Some(offset) => {
                let newline = start + offset;
                let mut text_end = newline;
                if text_end > start && bytes[text_end - 1] == b'\r' {
                    text_end -= 1;
                }
                lines.push(LineSpan {
                    start,
                    next_start: newline + 1,
                    text: &content[start..text_end],
                });
                start = newline + 1;
            }
            None => {
                lines.push(LineSpan {
                    start,
                    next_start: content.len(),
                    text: &content[start..],
                });
                break;
            }
        }
    }

    lines
}

/// The bytes that go between the marker lines: `content` verbatim, terminated
/// so the end marker keeps its own line.
fn interior_block(content: &str) -> String {
    if content.is_empty() {
        String::new()
    } else if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn append_block(note: &str, section_id: &str, content: &str) -> String {
    let mut out = String::from(note);

    if !out.is_empty() {
        // Exactly one blank line between the existing final content and the
        // begin marker. Only ever *adds* newlines: existing bytes are never
        // rewritten, so a note already ending in blank lines keeps them.
        let trailing_newlines = out.chars().rev().take_while(|ch| *ch == '\n').count();
        for _ in trailing_newlines..2 {
            out.push('\n');
        }
    }

    out.push_str(&begin_marker(section_id));
    out.push('\n');
    out.push_str(&interior_block(content));
    out.push_str(&end_marker(section_id));
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_with_two_sections() -> String {
        concat!(
            "---\n",
            "title: Daily\n",
            "updated: 2026-09-01 08:00\n",
            "---\n",
            "\n",
            "## Focus\n",
            "Human focus text.\n",
            "\n",
            "<!-- notesmith:section:begin briefing/meetings -->\n",
            "- old meetings\n",
            "<!-- notesmith:section:end briefing/meetings -->\n",
            "\n",
            "<!-- notesmith:section:begin briefing/tasks -->\n",
            "- old tasks\n",
            "<!-- notesmith:section:end briefing/tasks -->\n",
        )
        .to_string()
    }

    #[test]
    fn replaces_only_the_interior() {
        let note = note_with_two_sections();
        let result =
            update_managed_section(&note, "briefing/meetings", "- new meetings", true).unwrap();

        assert!(!result.appended);
        assert!(result.changed);
        assert!(result.content.contains("- new meetings\n"));
        assert!(!result.content.contains("- old meetings"));
        // Everything else is verbatim.
        assert!(result.content.contains("updated: 2026-09-01 08:00\n"));
        assert!(result.content.contains("- old tasks\n"));
    }

    #[test]
    fn preserves_hostile_bytes_outside_the_markers() {
        // Trailing spaces, a tab, CRLF/LF mixing and an unterminated HTML
        // comment — every one of which a whole-note rewrite would mangle.
        let note = "---\r\ntitle: Ugly\t\r\n---\n\
                    Human line with trailing spaces.   \n\
                    \tTabbed line with a trailing tab.\t\n\
                    <!-- an incomplete comment\n\
                    <!-- notesmith:section:begin s -->\n\
                    old\n\
                    <!-- notesmith:section:end s -->\r\n\
                    Tail with trailing space. \n";

        let result = update_managed_section(note, "s", "new", true).unwrap();

        let prefix_end = note.find("<!-- notesmith:section:begin s -->").unwrap();
        let suffix_start = note.find("<!-- notesmith:section:end s -->").unwrap();
        assert_eq!(&result.content[..prefix_end], &note[..prefix_end]);
        assert_eq!(
            &result.content[result.content.len() - (note.len() - suffix_start)..],
            &note[suffix_start..]
        );
        assert!(result.content.contains("\nnew\n<!-- notesmith:section:end s -->\r\n"));
    }

    #[test]
    fn identical_content_is_idempotent() {
        let note = note_with_two_sections();
        let first = update_managed_section(&note, "briefing/tasks", "- a\n- b\n", true).unwrap();
        let second =
            update_managed_section(&first.content, "briefing/tasks", "- a\n- b\n", true).unwrap();

        assert_eq!(first.content, second.content);
        assert!(!second.changed);
    }

    #[test]
    fn content_without_trailing_newline_gets_exactly_one() {
        let note = "<!-- notesmith:section:begin s -->\nold\n<!-- notesmith:section:end s -->\n";
        let result = update_managed_section(note, "s", "line", true).unwrap();
        assert_eq!(
            result.content,
            "<!-- notesmith:section:begin s -->\nline\n<!-- notesmith:section:end s -->\n"
        );
    }

    #[test]
    fn empty_content_empties_the_interior() {
        let note = "<!-- notesmith:section:begin s -->\nold\nlines\n<!-- notesmith:section:end s -->\n";
        let result = update_managed_section(note, "s", "", true).unwrap();
        assert_eq!(
            result.content,
            "<!-- notesmith:section:begin s -->\n<!-- notesmith:section:end s -->\n"
        );
    }

    #[test]
    fn updating_one_section_cannot_touch_another() {
        let note = note_with_two_sections();
        let result = update_managed_section(&note, "briefing/meetings", "- x", true).unwrap();

        let others = |text: &str| {
            let start = text.find("<!-- notesmith:section:begin briefing/tasks -->").unwrap();
            text[start..].to_string()
        };
        assert_eq!(others(&result.content), others(&note));
    }

    #[test]
    fn appends_one_block_with_one_blank_separator() {
        let note = "---\ntitle: T\n---\nBody line.\n";
        let result = update_managed_section(note, "briefing/new", "- fresh", true).unwrap();

        assert!(result.appended);
        assert_eq!(
            result.content,
            "---\ntitle: T\n---\nBody line.\n\n\
             <!-- notesmith:section:begin briefing/new -->\n\
             - fresh\n\
             <!-- notesmith:section:end briefing/new -->\n"
        );
    }

    #[test]
    fn appends_terminator_when_the_note_has_no_trailing_newline() {
        let result = update_managed_section("Body line.", "s", "x", true).unwrap();
        assert_eq!(
            result.content,
            "Body line.\n\n<!-- notesmith:section:begin s -->\nx\n<!-- notesmith:section:end s -->\n"
        );
    }

    #[test]
    fn appends_to_an_empty_note_without_a_leading_blank_line() {
        let result = update_managed_section("", "s", "x", true).unwrap();
        assert_eq!(
            result.content,
            "<!-- notesmith:section:begin s -->\nx\n<!-- notesmith:section:end s -->\n"
        );
    }

    #[test]
    fn appending_then_updating_converges() {
        let note = "Body.\n";
        let appended = update_managed_section(note, "s", "one", true).unwrap();
        let updated = update_managed_section(&appended.content, "s", "one", true).unwrap();
        assert_eq!(appended.content, updated.content);
        assert!(!updated.appended);
    }

    #[test]
    fn missing_pair_without_append_is_a_structured_error() {
        let error = update_managed_section("Body.\n", "s", "x", false).unwrap_err();
        assert_eq!(error.code(), "section_not_found");
        assert_eq!(error.section_id(), "s");
    }

    #[test]
    fn duplicate_begin_marker_is_rejected() {
        let note = "<!-- notesmith:section:begin s -->\na\n<!-- notesmith:section:begin s -->\nb\n<!-- notesmith:section:end s -->\n";
        let error = update_managed_section(note, "s", "x", true).unwrap_err();
        assert_eq!(error.code(), "duplicate_begin_marker");
        assert!(matches!(
            error,
            ManagedSectionError::DuplicateBeginMarker {
                first: 1,
                second: 3,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_end_marker_is_rejected() {
        let note = "<!-- notesmith:section:begin s -->\na\n<!-- notesmith:section:end s -->\nb\n<!-- notesmith:section:end s -->\n";
        let error = update_managed_section(note, "s", "x", true).unwrap_err();
        assert_eq!(error.code(), "duplicate_end_marker");
    }

    #[test]
    fn inverted_pair_is_rejected() {
        let note = "<!-- notesmith:section:end s -->\na\n<!-- notesmith:section:begin s -->\n";
        let error = update_managed_section(note, "s", "x", true).unwrap_err();
        assert_eq!(error.code(), "inverted_markers");
    }

    #[test]
    fn begin_without_end_is_rejected() {
        let note = "<!-- notesmith:section:begin s -->\na\n";
        let error = update_managed_section(note, "s", "x", true).unwrap_err();
        assert_eq!(error.code(), "missing_end_marker");
    }

    #[test]
    fn end_without_begin_is_rejected() {
        let note = "a\n<!-- notesmith:section:end s -->\n";
        let error = update_managed_section(note, "s", "x", true).unwrap_err();
        assert_eq!(error.code(), "missing_begin_marker");
    }

    #[test]
    fn markers_for_other_ids_are_ignored() {
        let note = "<!-- notesmith:section:begin other -->\na\n<!-- notesmith:section:end other -->\n";
        let error = update_managed_section(note, "s", "x", false).unwrap_err();
        assert_eq!(error.code(), "section_not_found");
    }

    #[test]
    fn content_containing_marker_lines_is_rejected() {
        let note = note_with_two_sections();
        for content in [
            "- ok\n<!-- notesmith:section:begin briefing/meetings -->\n",
            "<!-- notesmith:section:end briefing/meetings -->",
            "  <!-- notesmith:section:begin some/other-id -->  ",
        ] {
            let error =
                update_managed_section(&note, "briefing/meetings", content, true).unwrap_err();
            assert_eq!(error.code(), "content_contains_marker", "content {content:?}");
        }
        // The note is untouched on refusal by construction (no write happened),
        // and ordinary HTML comments in content are still fine.
        let ok = update_managed_section(&note, "briefing/meetings", "<!-- note -->", true);
        assert!(ok.is_ok());
    }

    #[test]
    fn invalid_section_ids_are_rejected() {
        for id in ["", " s ", "a\nb", "a--b"] {
            let error = update_managed_section("body\n", id, "x", true).unwrap_err();
            assert_eq!(error.code(), "invalid_section_id", "id {id:?}");
        }
    }

    #[test]
    fn marker_lines_may_carry_a_carriage_return() {
        let note = "<!-- notesmith:section:begin s -->\r\nold\r\n<!-- notesmith:section:end s -->\r\n";
        let result = update_managed_section(note, "s", "new", true).unwrap();
        assert_eq!(
            result.content,
            "<!-- notesmith:section:begin s -->\r\nnew\n<!-- notesmith:section:end s -->\r\n"
        );
    }
}
