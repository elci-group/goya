//! Native PDF raster proof. Real parser work runs outside the Wayland loop.
use goya_document::{
    extract_pdf_text, fit_page, render_pdf, AccessibleTextPage, RenderRequest, TextRect,
};
use goya_wayland::{GoyaWindow, KeySpecial, PlatformEvent, Shortcut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

fn main() {
    if let Err(error) = run() {
        eprintln!("goya-document: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut path = None;
    let mut page = 1;
    let mut max_side = 1600;
    let mut duration = 30;
    let mut live = false;
    let mut cursor = 0;
    while cursor < args.len() {
        if args[cursor] == "--live" {
            live = true;
            cursor += 1;
            continue;
        }
        let value = args.get(cursor + 1).ok_or("missing argument value")?;
        match args[cursor].as_str() {
            "--file" => path = Some(PathBuf::from(value)),
            "--page" => page = value.parse()?,
            "--max-side" => max_side = value.parse()?,
            "--duration" => duration = value.parse()?,
            _ => return Err("unknown argument".into()),
        }
        cursor += 2;
    }
    if duration == 0 || duration > 3600 {
        return Err("duration must be 1..3600 seconds".into());
    }
    let request = RenderRequest {
        path: path.ok_or("--file is required")?,
        page,
        max_side,
        timeout: Duration::from_secs(15),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    if !live {
        let page = render_pdf(&request, &cancelled)?;
        println!(
            "document render PASS: {}x{} RGBA bytes={}",
            page.width,
            page.height,
            page.rgba.len()
        );
        return Ok(());
    }
    let mut window = GoyaWindow::create("Goya document", 960, 720)?;
    let (tx, rx) = mpsc::sync_channel(1);
    let worker_cancelled = Arc::clone(&cancelled);
    let worker = std::thread::spawn(move || {
        let result = render_pdf(&request, &worker_cancelled).and_then(|pixels| {
            let text = extract_pdf_text(&request, &worker_cancelled)?;
            Ok((pixels, text))
        });
        let _ = tx.send(result);
    });
    let started = Instant::now();
    let mut source = None;
    let mut text_page: Option<AccessibleTextPage> = None;
    let mut fitted = None;
    let mut pointer = (0.0_f64, 0.0_f64);
    let mut anchor: Option<(f64, f64)> = None;
    let mut selection = String::new();
    let mut presented = 0_u64;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        while !window.close_requested() && started.elapsed() < Duration::from_secs(duration) {
            for event in window.poll() {
                match event {
                    PlatformEvent::KeySpecial(KeySpecial::Escape)
                    | PlatformEvent::Shortcut(Shortcut::Quit) => return Ok(()),
                    PlatformEvent::PointerMove(x, y) => {
                        pointer = (x, y);
                        if anchor.is_some() {
                            selection = select_text(
                                text_page.as_ref(),
                                anchor.unwrap(),
                                pointer,
                                (window.size().0 as f64, window.size().1 as f64),
                            );
                        }
                    }
                    PlatformEvent::PointerPress(x, y) => {
                        pointer = (x, y);
                        anchor = Some(pointer);
                        selection.clear();
                    }
                    PlatformEvent::PointerRelease(x, y) => {
                        pointer = (x, y);
                        if let Some(start) = anchor.take() {
                            selection = select_text(
                                text_page.as_ref(),
                                start,
                                pointer,
                                (window.size().0 as f64, window.size().1 as f64),
                            );
                        }
                    }
                    PlatformEvent::Shortcut(Shortcut::Copy) => {
                        window.set_clipboard_text(&selection)
                    }
                    _ => {}
                }
            }
            if let Ok(result) = rx.try_recv() {
                let (pixels, accessible) = result?;
                source = Some(pixels);
                text_page = Some(accessible);
            }
            let (width, height) = window.size();
            // 最小化/零尺寸 configure 不是解析失败；等待可绘制尺寸再恢复页面。
            if width <= 0 || height <= 0 {
                std::thread::sleep(Duration::from_millis(16));
                continue;
            }
            if let Some(page) = &source {
                let width = u32::try_from(width)?;
                let height = u32::try_from(height)?;
                if fitted
                    .as_ref()
                    .is_none_or(|previous: &goya_document::PixelPage| {
                        previous.width != width || previous.height != height
                    })
                {
                    fitted = Some(fit_page(page, width, height)?);
                }
                if let Some(frame) = &fitted {
                    let display = highlight_selection(frame, text_page.as_ref(), &selection);
                    window.present_rgba(&display.rgba, display.width, display.height)?;
                    presented += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    })();
    cancelled.store(true, Ordering::Release);
    worker.join().map_err(|_| "document worker panicked")?;
    outcome?;
    if presented == 0 {
        return Err("native window closed before document presentation".into());
    }
    println!("native document presentation PASS: {presented} frames");
    Ok(())
}

fn select_text(
    page: Option<&AccessibleTextPage>,
    start: (f64, f64),
    end: (f64, f64),
    viewport: (f64, f64),
) -> String {
    let Some(page) = page else {
        return String::new();
    };
    if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return String::new();
    }
    let scale = (viewport.0 as f32 / page.width).min(viewport.1 as f32 / page.height);
    let drawn_w = page.width * scale;
    let drawn_h = page.height * scale;
    let left = (viewport.0 as f32 - drawn_w) / 2.0;
    let top = (viewport.1 as f32 - drawn_h) / 2.0;
    let (x1, y1, x2, y2) = (
        start.0.min(end.0),
        start.1.min(end.1),
        start.0.max(end.0),
        start.1.max(end.1),
    );
    page.select(TextRect {
        x_min: ((x1 as f32 - left) / scale).max(0.0),
        y_min: ((y1 as f32 - top) / scale).max(0.0),
        x_max: ((x2 as f32 - left) / scale).max(0.0),
        y_max: ((y2 as f32 - top) / scale).max(0.0),
    })
    .unwrap_or_default()
}

fn highlight_selection(
    frame: &goya_document::PixelPage,
    page: Option<&AccessibleTextPage>,
    selected: &str,
) -> goya_document::PixelPage {
    let Some(page) = page else {
        return frame.clone();
    };
    if selected.is_empty() {
        return frame.clone();
    }
    let selected_words: Vec<&str> = selected.split_whitespace().collect();
    let mut output = frame.clone();
    let scale = (frame.width as f32 / page.width).min(frame.height as f32 / page.height);
    let left = (frame.width as f32 - page.width * scale) / 2.0;
    let top = (frame.height as f32 - page.height * scale) / 2.0;
    for word in &page.words {
        if !selected_words.contains(&word.text.as_str()) {
            continue;
        }
        let x0 = (left + word.x_min * scale).floor().max(0.0) as u32;
        let y0 = (top + word.y_min * scale).floor().max(0.0) as u32;
        let x1 = (left + word.x_max * scale).ceil().min(frame.width as f32) as u32;
        let y1 = (top + word.y_max * scale).ceil().min(frame.height as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = ((y * frame.width + x) * 4) as usize;
                output.rgba[offset] = ((u16::from(output.rgba[offset]) + 255) / 2) as u8;
                output.rgba[offset + 1] = ((u16::from(output.rgba[offset + 1]) + 220) / 2) as u8;
                output.rgba[offset + 2] = (u16::from(output.rgba[offset + 2]) / 2) as u8;
            }
        }
    }
    output
}
