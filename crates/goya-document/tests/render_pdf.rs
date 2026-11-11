//! Explicit native backend acceptance; never silently passes without the backend.
use goya_document::{extract_pdf_text, render_pdf, RenderRequest, TextRect};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[test]
#[ignore = "requires Linux bwrap/prlimit/Poppler and an allowed user namespace"]
fn synthetic_pdf_renders_real_pixels_inside_the_sandbox() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/documents/one-page.pdf");
    let page = render_pdf(
        &RenderRequest {
            path,
            page: 1,
            max_side: 600,
            timeout: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
    )
    .expect("sandboxed Poppler must render the synthetic PDF");
    assert_eq!((page.width, page.height), (600, 540));
    assert_eq!(page.rgba.len(), 600 * 540 * 4);
    assert!(page.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
    assert!(page
        .rgba
        .chunks_exact(4)
        .any(|pixel| u16::from(pixel[2]) > u16::from(pixel[0]) + 40));
    println!("sandboxed synthetic PDF PASS: 600x540 actual opaque page pixels");
}

#[test]
#[ignore = "requires Linux bwrap/prlimit/Poppler and an allowed user namespace"]
fn synthetic_pdf_text_is_selectable_and_accessible_inside_the_sandbox() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/documents/one-page.pdf");
    let page = extract_pdf_text(
        &RenderRequest {
            path,
            page: 1,
            max_side: 600,
            timeout: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
    )
    .expect("sandboxed pdftotext extraction");
    assert_eq!(page.page, 1);
    assert_eq!(page.text(), "Goya synthetic document");
    assert_eq!(
        page.select(TextRect {
            x_min: 20.0,
            y_min: 20.0,
            x_max: 120.0,
            y_max: 50.0
        })
        .unwrap(),
        "Goya synthetic"
    );
    println!(
        "sandboxed synthetic PDF PASS: {} accessible words, selected page text",
        page.words.len()
    );
}
