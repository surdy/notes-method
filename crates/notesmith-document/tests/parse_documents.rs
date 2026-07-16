//! End-to-end parsing tests against real generated PDF/EPUB fixtures.
//!
//! Fixtures are generated in-memory (no checked-in binaries) so the tests are
//! hermetic: `printpdf` builds a text PDF, and a hand-assembled zip builds a
//! minimal valid EPUB.

use std::io::Write;

use notesmith_document::{ChunkOptions, DocumentError, parse_document};

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

#[test]
fn parses_pdf_into_text_and_chunks() {
    let pdf = sample_pdf(&[
        "Hello from a PDF document.",
        "A second line of extractable text.",
    ]);
    let parsed = parse_document("report.pdf", &pdf, &ChunkOptions::default()).unwrap();

    assert_eq!(parsed.meta.source_type, "pdf");
    assert_eq!(parsed.meta.unit_label, "page");
    assert!(parsed.meta.unit_count >= 1);
    assert!(parsed.text.contains("Hello from a PDF document."));
    assert!(parsed.text.contains("second line of extractable text."));
    assert!(!parsed.chunks.is_empty());
    assert_eq!(parsed.chunks[0].char_start, 0);
}

#[test]
fn parses_epub_into_text_chunks_and_metadata() {
    let epub = sample_epub(
        "Spike Book",
        "Test Author",
        "<h1>Chapter One</h1><p>Hello from an EPUB paragraph.</p>",
    );
    let parsed = parse_document("book.epub", &epub, &ChunkOptions::default()).unwrap();

    assert_eq!(parsed.meta.source_type, "epub");
    assert_eq!(parsed.meta.title.as_deref(), Some("Spike Book"));
    assert_eq!(parsed.meta.author.as_deref(), Some("Test Author"));
    assert_eq!(parsed.meta.unit_label, "chapter");
    assert!(parsed.text.contains("Chapter One"));
    assert!(parsed.text.contains("Hello from an EPUB paragraph."));
    assert!(!parsed.chunks.is_empty());
}

#[test]
fn empty_pdf_reports_no_extractable_text() {
    let pdf = sample_pdf(&[]);
    let err = parse_document("blank.pdf", &pdf, &ChunkOptions::default()).unwrap_err();
    assert!(matches!(err, DocumentError::Empty), "got {err:?}");
}

#[test]
fn truncated_epub_zip_degrades_without_panic() {
    let mut epub = sample_epub("T", "A", "<p>hi</p>");
    epub.truncate(epub.len() / 2);
    let err = parse_document("broken.epub", &epub, &ChunkOptions::default()).unwrap_err();
    assert!(
        matches!(
            err,
            DocumentError::Parse { kind: "epub", .. } | DocumentError::Empty
        ),
        "got {err:?}"
    );
}
