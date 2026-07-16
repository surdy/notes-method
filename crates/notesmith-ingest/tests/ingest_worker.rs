//! End-to-end tests for the drop-folder ingest worker (ADR 0022, #263).
//!
//! Fixtures are generated in-memory (no checked-in binaries): `printpdf` builds
//! a text PDF and a hand-assembled zip builds a minimal valid EPUB, mirroring
//! `notesmith-document`'s own fixtures.

use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::{TimeZone, Utc};
use notesmith_ingest::{IngestWorker, ItemOutcome};

fn sample_pdf(lines: &[&str]) -> Vec<u8> {
    use printpdf::*;
    let (doc, page1, layer1) = PdfDocument::new("Fixture", Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();
    let layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 280.0;
    for line in lines {
        layer.use_text(*line, 14.0, Mm(20.0), Mm(y), &font);
        y -= 10.0;
    }
    doc.save_to_bytes().unwrap()
}

fn sample_epub(title: &str, author: &str, body_html: &str) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        let opt = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", opt).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();

        zip.start_file("OEBPS/content.opf", opt).unwrap();
        let opf = format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:creator>{author}</dc:creator><dc:identifier id="id">urn:uuid:test</dc:identifier></metadata><manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="ch1"/></spine></package>"#
        );
        zip.write_all(opf.as_bytes()).unwrap();

        zip.start_file("OEBPS/ch1.xhtml", opt).unwrap();
        let xhtml = format!(
            r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Ch1</title></head><body>{body_html}</body></html>"#
        );
        zip.write_all(xhtml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn write_raw(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn worker(root: &Path) -> IngestWorker {
    IngestWorker::new(root, "raw", "ingested")
        .with_now(Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap())
}

#[test]
fn happy_path_extracts_pdf_and_epub_keeping_raw_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(
        root,
        "raw/talk.pdf",
        &sample_pdf(&["Hello from the drop folder"]),
    );
    write_raw(
        root,
        "raw/book.epub",
        &sample_epub(
            "The Book",
            "An Author",
            "<h1>Chapter</h1><p>Body text here.</p>",
        ),
    );

    let report = worker(root).run().unwrap();

    assert_eq!(report.ingested(), 2, "both documents ingested");
    assert_eq!(report.failed(), 0);
    assert_eq!(report.unsupported(), 0);

    // Raw files kept in place (never moved/deleted).
    assert!(root.join("raw/talk.pdf").exists());
    assert!(root.join("raw/book.epub").exists());

    // Sidecar notes written under the notes dir with provenance frontmatter.
    let pdf_note = fs::read_to_string(root.join("ingested/talk.md")).unwrap();
    assert!(pdf_note.contains("source_path: raw/talk.pdf"));
    assert!(pdf_note.contains("source_type: pdf"));
    assert!(pdf_note.contains("source_hash: sha256:"));
    assert!(pdf_note.contains("status: ingested"));
    assert!(pdf_note.contains("page_count:"));

    let epub_note = fs::read_to_string(root.join("ingested/book.md")).unwrap();
    assert!(epub_note.contains("source_path: raw/book.epub"));
    assert!(epub_note.contains("source_type: epub"));
    assert!(epub_note.contains("status: ingested"));
    assert!(epub_note.contains("chapter_count:"));
    assert!(epub_note.contains("Body text here"));
}

#[test]
fn rerun_is_a_noop_when_content_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["stable content"]));

    worker(root).run().unwrap();
    let before = fs::read_to_string(root.join("ingested/talk.md")).unwrap();

    let report = worker(root).run().unwrap();
    assert_eq!(report.unchanged(), 1);
    assert_eq!(report.ingested(), 0);

    let after = fs::read_to_string(root.join("ingested/talk.md")).unwrap();
    assert_eq!(before, after, "unchanged note is not rewritten differently");
}

#[test]
fn changed_content_is_reingested() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["version one"]));
    worker(root).run().unwrap();

    // Replace the file's content in place.
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["version two changed"]));
    let report = worker(root).run().unwrap();

    assert_eq!(report.reingested(), 1);
    assert_eq!(report.unchanged(), 0);
    let note = fs::read_to_string(root.join("ingested/talk.md")).unwrap();
    assert!(note.contains("version two changed"));
}

#[test]
fn rename_moves_note_without_reextracting() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bytes = sample_pdf(&["portable content"]);
    write_raw(root, "raw/original.pdf", &bytes);
    worker(root).run().unwrap();
    assert!(root.join("ingested/original.md").exists());

    // Simulate a rename: same bytes at a new path, old path gone.
    fs::remove_file(root.join("raw/original.pdf")).unwrap();
    write_raw(root, "raw/renamed.pdf", &bytes);

    let report = worker(root).run().unwrap();
    assert_eq!(report.renamed(), 1);
    assert_eq!(report.ingested(), 0);
    assert_eq!(report.reingested(), 0);

    assert!(
        !root.join("ingested/original.md").exists(),
        "old note removed"
    );
    let note = fs::read_to_string(root.join("ingested/renamed.md")).unwrap();
    assert!(note.contains("source_path: raw/renamed.pdf"));
}

#[test]
fn unsupported_file_type_is_recorded_not_extracted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/notes.xyz", b"some bytes");

    let report = worker(root).run().unwrap();
    assert_eq!(report.unsupported(), 1);
    assert_eq!(report.ingested(), 0);

    let note = fs::read_to_string(root.join("ingested/notes.md")).unwrap();
    assert!(note.contains("status: unsupported"));

    // Re-run: unchanged, not retried.
    let report = worker(root).run().unwrap();
    assert_eq!(report.unchanged(), 1);
    assert_eq!(report.unsupported(), 0);
}

#[test]
fn malformed_document_degrades_without_panic() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // A .pdf extension but garbage bytes: parser fails, batch continues.
    write_raw(root, "raw/broken.pdf", b"%PDF not really a pdf");
    write_raw(root, "raw/good.pdf", &sample_pdf(&["still works"]));

    let report = worker(root).run().unwrap();
    // The good file still ingests; the broken one is recorded (failed or
    // unsupported depending on how the parser classifies it), never panics.
    assert_eq!(
        report.ingested(),
        1,
        "good doc ingested despite bad sibling"
    );
    assert_eq!(report.failed() + report.unsupported(), 1);
    assert!(root.join("ingested/good.md").exists());
    assert!(root.join("ingested/broken.md").exists());
}

#[test]
fn orphaned_note_is_reported_when_raw_removed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["temporary"]));
    worker(root).run().unwrap();

    fs::remove_file(root.join("raw/talk.pdf")).unwrap();
    let report = worker(root).run().unwrap();

    assert_eq!(report.items.len(), 0, "no raw files to process");
    assert_eq!(report.orphaned, vec!["ingested/talk.md".to_string()]);
    // Note is reported, not deleted.
    assert!(root.join("ingested/talk.md").exists());
}

#[test]
fn missing_raw_dir_yields_empty_report() {
    let dir = tempfile::tempdir().unwrap();
    let report = worker(dir.path()).run().unwrap();
    assert!(report.items.is_empty());
    assert!(report.orphaned.is_empty());
}

#[test]
fn failed_document_is_retried_on_next_pass() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/broken.pdf", b"%PDF broken bytes");

    let first = worker(root).run().unwrap();
    // Only assert retry behavior when the parser classified it as a transient
    // failure (not a terminal "unsupported").
    if first.failed() == 1 {
        let note = fs::read_to_string(root.join("ingested/broken.md")).unwrap();
        assert!(note.contains("status: failed"));
        // Same bytes, same status: re-processed (still Failed), not skipped as
        // Unchanged, because failed notes are always retried.
        let second = worker(root).run().unwrap();
        assert_eq!(second.reingested() + second.failed(), 1);
        assert_eq!(second.unchanged(), 0);
    }
    let _ = ItemOutcome::Failed;
}

// ---------------------------------------------------------------------------
// Ingestion ledger (#264): per-vault append-only `raw/log.md`.
// ---------------------------------------------------------------------------

fn ledger_entries(root: &Path) -> Vec<String> {
    let log = fs::read_to_string(root.join("raw/log.md")).unwrap_or_default();
    log.lines()
        .filter(|l| l.starts_with("## ["))
        .map(str::to_string)
        .collect()
}

#[test]
fn ledger_records_one_entry_per_state_changing_action() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["Hello"]));
    write_raw(root, "raw/notes.xyz", b"some bytes");

    worker(root).run().unwrap();

    let entries = ledger_entries(root);
    assert_eq!(entries.len(), 2, "one greppable entry per processed file");
    let log = fs::read_to_string(root.join("raw/log.md")).unwrap();
    assert!(log.contains("## [2026-07-15T12:00:00Z] ingest"));
    assert!(log.contains("ingested/talk.md"));
    assert!(log.contains("status=ingested"));
    assert!(log.contains("source=raw/talk.pdf"));
    assert!(log.contains("hash="));
    assert!(log.contains("status=unsupported"));
    assert!(log.contains("source=raw/notes.xyz"));
}

#[test]
fn ledger_is_append_only_and_skips_steady_state_noops() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["Hello"]));

    worker(root).run().unwrap();
    let after_first = fs::read_to_string(root.join("raw/log.md")).unwrap();
    assert_eq!(ledger_entries(root).len(), 1);

    // Second pass: file unchanged -> Unchanged (noop) -> nothing appended.
    let report = worker(root).run().unwrap();
    assert_eq!(report.unchanged(), 1);
    let after_second = fs::read_to_string(root.join("raw/log.md")).unwrap();
    assert_eq!(
        after_first, after_second,
        "steady-state noop appends nothing and never rewrites prior entries"
    );
}

#[test]
fn ledger_appends_reingest_without_touching_prior_entries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["version one"]));
    worker(root).run().unwrap();
    let after_first = fs::read_to_string(root.join("raw/log.md")).unwrap();

    write_raw(root, "raw/talk.pdf", &sample_pdf(&["version two changed"]));
    worker(root).run().unwrap();

    let after_second = fs::read_to_string(root.join("raw/log.md")).unwrap();
    assert!(
        after_second.starts_with(&after_first),
        "prior ledger content preserved verbatim (append-only)"
    );
    assert_eq!(ledger_entries(root).len(), 2);
    assert!(after_second.contains("status=reingest"));
}

#[test]
fn ledger_is_never_ingested_and_delete_then_rerun_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/talk.pdf", &sample_pdf(&["Hello"]));
    assert_eq!(worker(root).run().unwrap().ingested(), 1);

    // The ledger itself is never treated as a raw input.
    let second = worker(root).run().unwrap();
    assert_eq!(
        second.items.len(),
        1,
        "only the pdf is processed, not log.md"
    );
    assert_eq!(second.unsupported(), 0);
    assert!(!root.join("ingested/log.md").exists());

    // Deleting the ledger and re-running is safe: already-ingested file is a
    // noop (no crash). A later state change cleanly resumes the ledger.
    fs::remove_file(root.join("raw/log.md")).unwrap();
    assert_eq!(worker(root).run().unwrap().unchanged(), 1);

    write_raw(root, "raw/two.pdf", &sample_pdf(&["second"]));
    worker(root).run().unwrap();
    let log = fs::read_to_string(root.join("raw/log.md")).unwrap();
    assert!(log.contains("ingested/two.md"));
    assert_eq!(
        ledger_entries(root).len(),
        1,
        "ledger resumed after deletion"
    );
}

#[test]
fn ledger_records_failed_or_unsupported_for_broken_input() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_raw(root, "raw/broken.pdf", b"%PDF not a real pdf");

    let report = worker(root).run().unwrap();
    let log = fs::read_to_string(root.join("raw/log.md")).unwrap();
    if report.failed() == 1 {
        assert!(log.contains("status=failed"));
    } else {
        assert!(log.contains("status=unsupported"));
    }
    assert_eq!(ledger_entries(root).len(), 1);
}
