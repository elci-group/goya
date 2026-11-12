//! Bounded PDF page rendering. Call blocking renderer operations on a worker.
use std::ffi::OsString;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_SIDE: u32 = 4096;
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = MAX_PIXELS * 3 + 4096;
pub const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TEXT_WORDS: usize = 50_000;
const MAX_FIND_MATCHES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    InvalidRequest,
    InvalidImage,
    LimitExceeded,
    InputUnavailable,
    BackendUnavailable,
    BackendFailed,
    TimedOut,
    Cancelled,
    WorkerFailed,
}
impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "document.{self:?}")
    }
}
impl std::error::Error for DocumentError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    pub text: String,
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleTextPage {
    pub page: u32,
    pub width: f32,
    pub height: f32,
    pub words: Vec<TextWord>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRect {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl AccessibleTextPage {
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn select(&self, rect: TextRect) -> Result<String, DocumentError> {
        if ![rect.x_min, rect.y_min, rect.x_max, rect.y_max]
            .iter()
            .all(|v| v.is_finite())
            || rect.x_min > rect.x_max
            || rect.y_min > rect.y_max
        {
            return Err(DocumentError::InvalidRequest);
        }
        Ok(self
            .words
            .iter()
            .filter(|word| {
                word.x_max >= rect.x_min
                    && word.x_min <= rect.x_max
                    && word.y_max >= rect.y_min
                    && word.y_min <= rect.y_max
            })
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" "))
    }

    pub fn find(&self, query: &str, limit: usize) -> Vec<TextMatch> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let limit = limit.min(MAX_FIND_MATCHES);
        let needle = query.to_lowercase();
        let mut matches = Vec::new();
        for start in 0..self.words.len() {
            if matches.len() == limit {
                break;
            }
            let mut folded = String::new();
            let mut text = String::new();
            for end in start..self.words.len() {
                if end > start {
                    folded.push(' ');
                    text.push(' ');
                }
                folded.push_str(&self.words[end].text.to_lowercase());
                text.push_str(&self.words[end].text);
                if folded == needle {
                    matches.push(TextMatch { start, end, text });
                    break;
                }
                if folded.len() >= needle.len() {
                    break;
                }
            }
        }
        matches
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, DocumentError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(DocumentError::LimitExceeded);
    }
    let count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(DocumentError::LimitExceeded)?;
    if count > MAX_PIXELS {
        return Err(DocumentError::LimitExceeded);
    }
    Ok(count)
}

fn token<'a>(bytes: &'a [u8], offset: &mut usize) -> Result<&'a [u8], DocumentError> {
    loop {
        while bytes.get(*offset).is_some_and(u8::is_ascii_whitespace) {
            *offset += 1;
        }
        if bytes.get(*offset) != Some(&b'#') {
            break;
        }
        while bytes.get(*offset).is_some_and(|byte| *byte != b'\n') {
            *offset += 1;
        }
    }
    let start = *offset;
    while bytes
        .get(*offset)
        .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'#')
    {
        *offset += 1;
    }
    if start == *offset {
        return Err(DocumentError::InvalidImage);
    }
    Ok(&bytes[start..*offset])
}

pub fn decode_ppm(bytes: &[u8]) -> Result<PixelPage, DocumentError> {
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(DocumentError::LimitExceeded);
    }
    let mut offset = 0;
    if token(bytes, &mut offset)? != b"P6" {
        return Err(DocumentError::InvalidImage);
    }
    let mut number = || -> Result<u32, DocumentError> {
        std::str::from_utf8(token(bytes, &mut offset)?)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or(DocumentError::InvalidImage)
    };
    let width = number()?;
    let height = number()?;
    if number()? != 255 {
        return Err(DocumentError::InvalidImage);
    }
    let count = pixel_count(width, height)?;
    if !bytes.get(offset).is_some_and(u8::is_ascii_whitespace) {
        return Err(DocumentError::InvalidImage);
    }
    // 二进制首像素可以是空白或 #；只能消费头部终止符，不能继续跳过像素。
    offset += if bytes.get(offset..offset + 2) == Some(b"\r\n") {
        2
    } else {
        1
    };
    if bytes.len().checked_sub(offset) != Some(count * 3) {
        return Err(DocumentError::InvalidImage);
    }
    let mut rgba = Vec::with_capacity(count * 4);
    for rgb in bytes[offset..].chunks_exact(3) {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    Ok(PixelPage {
        width,
        height,
        rgba,
    })
}

/// Aspect-preserving nearest-neighbor presentation with a white page surround.
pub fn fit_page(page: &PixelPage, width: u32, height: u32) -> Result<PixelPage, DocumentError> {
    let count = pixel_count(width, height)?;
    let source_count = pixel_count(page.width, page.height)?;
    if page.rgba.len() != source_count * 4 {
        return Err(DocumentError::InvalidImage);
    }
    let scale = (width as f64 / page.width as f64).min(height as f64 / page.height as f64);
    let draw_width = (page.width as f64 * scale).floor().max(1.0) as u32;
    let draw_height = (page.height as f64 * scale).floor().max(1.0) as u32;
    let (left, top) = ((width - draw_width) / 2, (height - draw_height) / 2);
    let mut rgba = vec![255; count * 4];
    for y in 0..draw_height {
        for x in 0..draw_width {
            let sx = (u64::from(x) * u64::from(page.width) / u64::from(draw_width)) as usize;
            let sy = (u64::from(y) * u64::from(page.height) / u64::from(draw_height)) as usize;
            let source = (sy * page.width as usize + sx) * 4;
            let dest = (((y + top) * width + x + left) as usize) * 4;
            rgba[dest..dest + 4].copy_from_slice(&page.rgba[source..source + 4]);
        }
    }
    Ok(PixelPage {
        width,
        height,
        rgba,
    })
}

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub path: PathBuf,
    pub page: u32,
    pub max_side: u32,
    pub timeout: Duration,
}

fn sandbox_args(path: &Path, page: u32, side: u32) -> Vec<OsString> {
    [
        "--unshare-all",
        "--new-session",
        "--die-with-parent",
        "--clearenv",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind",
        "/lib64",
        "/lib64",
        "--ro-bind",
        "/etc/fonts",
        "/etc/fonts",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--setenv",
        "HOME",
        "/tmp",
        "--setenv",
        "LANG",
        "C.UTF-8",
        "--chdir",
        "/tmp",
        "--ro-bind",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([path.as_os_str().to_owned()])
    .chain(
        [
            "/input.pdf",
            "--",
            "/usr/bin/prlimit",
            "--as=1073741824",
            "--cpu=10",
            "--",
            "/usr/bin/pdftoppm",
            "-f",
        ]
        .into_iter()
        .map(OsString::from),
    )
    .chain([
        page.to_string().into(),
        "-singlefile".into(),
        "-scale-to".into(),
        side.to_string().into(),
        "/input.pdf".into(),
    ])
    .collect()
}

fn text_sandbox_args(path: &Path, page: u32) -> Vec<OsString> {
    let mut args = sandbox_args(path, page, 1);
    let start = args
        .iter()
        .position(|arg| arg == "/usr/bin/prlimit")
        .unwrap();
    args.truncate(start);
    args.extend(
        [
            "/usr/bin/prlimit",
            "--as=1073741824",
            "--cpu=10",
            "--",
            "/usr/bin/pdftotext",
            "-f",
        ]
        .into_iter()
        .map(OsString::from),
    );
    args.extend([
        page.to_string().into(),
        "-l".into(),
        page.to_string().into(),
        "-bbox".into(),
        "/input.pdf".into(),
        "-".into(),
    ]);
    args
}

fn html_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

pub fn parse_bbox_text(page: u32, html: &str) -> Result<AccessibleTextPage, DocumentError> {
    if page == 0 || html.len() > MAX_TEXT_BYTES {
        return Err(DocumentError::LimitExceeded);
    }
    let page_tag = html.find("<page ").ok_or(DocumentError::InvalidImage)?;
    let tag_end = html[page_tag..]
        .find('>')
        .ok_or(DocumentError::InvalidImage)?
        + page_tag;
    let attrs = &html[page_tag..tag_end];
    let attr = |name: &str| -> Result<f32, DocumentError> {
        let key = format!("{name}=\"");
        let start = attrs.find(&key).ok_or(DocumentError::InvalidImage)? + key.len();
        let end = attrs[start..]
            .find('"')
            .ok_or(DocumentError::InvalidImage)?
            + start;
        attrs[start..end]
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or(DocumentError::InvalidImage)
    };
    let width = attr(" width")?;
    let height = attr(" height")?;
    let mut words = Vec::new();
    let mut rest = &html[tag_end..];
    while let Some(start) = rest.find("<word ") {
        rest = &rest[start + 6..];
        let end = rest.find('>').ok_or(DocumentError::InvalidImage)?;
        let attrs = &rest[..end];
        rest = &rest[end + 1..];
        let close = rest.find("</word>").ok_or(DocumentError::InvalidImage)?;
        let text = html_unescape(rest[..close].trim());
        rest = &rest[close + 7..];
        if words.len() == MAX_TEXT_WORDS {
            return Err(DocumentError::LimitExceeded);
        }
        let coord = |name: &str| -> Result<f32, DocumentError> {
            let key = format!("{name}=\"");
            let start = attrs.find(&key).ok_or(DocumentError::InvalidImage)? + key.len();
            let end = attrs[start..]
                .find('"')
                .ok_or(DocumentError::InvalidImage)?
                + start;
            attrs[start..end]
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or(DocumentError::InvalidImage)
        };
        let word = TextWord {
            text,
            x_min: coord("xMin")?,
            y_min: coord("yMin")?,
            x_max: coord("xMax")?,
            y_max: coord("yMax")?,
        };
        if word.x_min > word.x_max
            || word.y_min > word.y_max
            || word.x_max > width
            || word.y_max > height
        {
            return Err(DocumentError::InvalidImage);
        }
        words.push(word);
    }
    Ok(AccessibleTextPage {
        page,
        width,
        height,
        words,
    })
}

/// Execute on a dedicated worker. Missing/denied sandbox is a hard error.
pub fn render_pdf(
    request: &RenderRequest,
    cancelled: &AtomicBool,
) -> Result<PixelPage, DocumentError> {
    if request.page == 0
        || request.max_side == 0
        || request.max_side > MAX_SIDE
        || request.timeout.is_zero()
        || request.timeout > Duration::from_secs(30)
    {
        return Err(DocumentError::InvalidRequest);
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(DocumentError::Cancelled);
    }
    let path = request
        .path
        .canonicalize()
        .map_err(|_| DocumentError::InputUnavailable)?;
    let metadata = path
        .metadata()
        .map_err(|_| DocumentError::InputUnavailable)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(DocumentError::LimitExceeded);
    }
    let mut child = Command::new("/usr/bin/bwrap")
        .args(sandbox_args(&path, request.page, request.max_side))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| DocumentError::BackendUnavailable)?;
    let stdout = child.stdout.take().ok_or(DocumentError::WorkerFailed)?;
    let stderr = child.stderr.take().ok_or(DocumentError::WorkerFailed)?;
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let diagnostics = std::thread::spawn(move || {
        // Drain diagnostics without retaining untrusted document text or paths.
        std::io::copy(&mut stderr.take(64 * 1024), &mut std::io::sink())
    });
    let started = Instant::now();
    let outcome = loop {
        if cancelled.load(Ordering::Acquire) {
            break Err(DocumentError::Cancelled);
        }
        if started.elapsed() >= request.timeout {
            break Err(DocumentError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(DocumentError::BackendFailed)
                }
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break Err(DocumentError::WorkerFailed),
        }
    };
    // 取消、超时和错误都必须回收解析进程，不能把后台解析留给窗口生命周期之外。
    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let bytes = output
        .join()
        .map_err(|_| DocumentError::WorkerFailed)?
        .map_err(|_| DocumentError::WorkerFailed)?;
    diagnostics
        .join()
        .map_err(|_| DocumentError::WorkerFailed)?
        .map_err(|_| DocumentError::WorkerFailed)?;
    outcome?;
    decode_ppm(&bytes)
}

/// Extract one page's accessible words and PDF-space bounding boxes with the
/// same isolated worker policy as raster rendering.
pub fn extract_pdf_text(
    request: &RenderRequest,
    cancelled: &AtomicBool,
) -> Result<AccessibleTextPage, DocumentError> {
    if request.page == 0 || request.timeout.is_zero() || request.timeout > Duration::from_secs(30) {
        return Err(DocumentError::InvalidRequest);
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(DocumentError::Cancelled);
    }
    let path = request
        .path
        .canonicalize()
        .map_err(|_| DocumentError::InputUnavailable)?;
    let metadata = path
        .metadata()
        .map_err(|_| DocumentError::InputUnavailable)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(DocumentError::LimitExceeded);
    }
    let mut child = Command::new("/usr/bin/bwrap")
        .args(text_sandbox_args(&path, request.page))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| DocumentError::BackendUnavailable)?;
    let stdout = child.stdout.take().ok_or(DocumentError::WorkerFailed)?;
    let stderr = child.stderr.take().ok_or(DocumentError::WorkerFailed)?;
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_TEXT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let diagnostics = std::thread::spawn(move || {
        std::io::copy(&mut stderr.take(64 * 1024), &mut std::io::sink())
    });
    let started = Instant::now();
    let outcome = loop {
        if cancelled.load(Ordering::Acquire) {
            break Err(DocumentError::Cancelled);
        }
        if started.elapsed() >= request.timeout {
            break Err(DocumentError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(DocumentError::BackendFailed)
                }
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break Err(DocumentError::WorkerFailed),
        }
    };
    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let bytes = output
        .join()
        .map_err(|_| DocumentError::WorkerFailed)?
        .map_err(|_| DocumentError::WorkerFailed)?;
    diagnostics
        .join()
        .map_err(|_| DocumentError::WorkerFailed)?
        .map_err(|_| DocumentError::WorkerFailed)?;
    outcome?;
    let html = std::str::from_utf8(&bytes).map_err(|_| DocumentError::InvalidImage)?;
    parse_bbox_text(request.page, html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_whitespace_pixels_are_not_consumed_as_header() {
        let page = decode_ppm(b"P6\n# comment\n1 1\n255\n \n#").unwrap();
        assert_eq!(page.rgba, [32, 10, 35, 255]);
    }
    #[test]
    fn rejects_corruption_overflow_truncation_and_extra_pixels() {
        for bytes in [
            b"P3\n1 1\n255\nabc".as_slice(),
            b"P6\n0 1\n255\n",
            b"P6\n99999999999 1\n255\n",
            b"P6\n4097 1\n255\n",
            b"P6\n1 1\n65535\nabc",
            b"P6\n1 1\n255\nab",
            b"P6\n1 1\n255\nabcd",
        ] {
            assert!(decode_ppm(bytes).is_err(), "accepted corrupt PPM");
        }
    }
    #[test]
    fn fit_preserves_aspect_and_rejects_inconsistent_source() {
        let page = PixelPage {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 255, 255, 0, 0, 255],
        };
        let fitted = fit_page(&page, 4, 4).unwrap();
        assert_eq!(&fitted.rgba[..16], &[255; 16]);
        assert_eq!(&fitted.rgba[16..20], &[0, 0, 0, 255]);
        assert_eq!(&fitted.rgba[24..28], &[255, 0, 0, 255]);
        assert!(fit_page(
            &PixelPage {
                width: 2,
                height: 2,
                rgba: vec![]
            },
            4,
            4
        )
        .is_err());
        assert!(fit_page(&page, 0, 4).is_err());
    }
    #[test]
    fn renderer_has_no_shell_or_unsandboxed_fallback() {
        let args = sandbox_args(Path::new("/synthetic/file with ; spaces.pdf"), 2, 1200);
        assert!(args.contains(&OsString::from("--unshare-all")));
        assert!(args.contains(&OsString::from("/synthetic/file with ; spaces.pdf")));
        assert!(args.contains(&OsString::from("/usr/bin/pdftoppm")));
        assert!(!args.contains(&OsString::from("--share-net")));
    }
    #[test]
    fn cancellation_and_invalid_request_do_not_spawn_parser() {
        let mut request = RenderRequest {
            path: "/unavailable.pdf".into(),
            page: 1,
            max_side: 100,
            timeout: Duration::from_secs(1),
        };
        assert_eq!(
            render_pdf(&request, &AtomicBool::new(true)),
            Err(DocumentError::Cancelled)
        );
        request.page = 0;
        assert_eq!(
            render_pdf(&request, &AtomicBool::new(false)),
            Err(DocumentError::InvalidRequest)
        );
    }

    #[test]
    fn bbox_parser_builds_page_scoped_selection_and_rejects_bad_bounds() {
        let html = r#"<page width="200" height="100"><word xMin="1" yMin="2" xMax="10" yMax="12">Goya &amp; Docs</word><word xMin="20" yMin="2" xMax="30" yMax="12">PDF</word></page>"#;
        let page = parse_bbox_text(3, html).unwrap();
        assert_eq!(page.page, 3);
        assert_eq!(page.text(), "Goya & Docs PDF");
        assert_eq!(
            page.select(TextRect {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 15.0,
                y_max: 20.0
            })
            .unwrap(),
            "Goya & Docs"
        );
        assert!(page
            .select(TextRect {
                x_min: f32::NAN,
                y_min: 0.0,
                x_max: 1.0,
                y_max: 1.0
            })
            .is_err());
        assert!(parse_bbox_text(1, &"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
        assert!(parse_bbox_text(1, "<page width=\"10\" height=\"10\"><word xMin=\"9\" yMin=\"1\" xMax=\"11\" yMax=\"2\">x</word></page>").is_err());
    }

    fn page_with(words: &[&str]) -> AccessibleTextPage {
        AccessibleTextPage {
            page: 1,
            width: 100.0,
            height: 100.0,
            words: words
                .iter()
                .map(|text| TextWord {
                    text: (*text).to_string(),
                    x_min: 0.0,
                    y_min: 0.0,
                    x_max: 1.0,
                    y_max: 1.0,
                })
                .collect(),
        }
    }

    #[test]
    fn find_empty_query_returns_no_matches() {
        let page = page_with(&["Hello", "World"]);
        assert!(page.find("", 8).is_empty());
        assert!(page.find("   ", 8).is_empty());
        assert_eq!(page.words.len(), 2);
    }

    #[test]
    fn find_folds_unicode_scalar_case() {
        let page = page_with(&["Éclair", "İ"]);
        assert_eq!(
            page.find("  éclair ", 4),
            vec![TextMatch {
                start: 0,
                end: 0,
                text: "Éclair".to_string(),
            }]
        );
        assert_eq!(
            page.find("i\u{0307}", 4),
            vec![TextMatch {
                start: 1,
                end: 1,
                text: "İ".to_string(),
            }]
        );
        assert!(page.find("i", 4).is_empty());
    }

    #[test]
    fn find_matches_adjacent_two_word_span() {
        let page = page_with(&["Hello", "World"]);
        assert_eq!(
            page.find("hello", 4),
            vec![TextMatch {
                start: 0,
                end: 0,
                text: "Hello".to_string(),
            }]
        );
        assert_eq!(
            page.find("hello world", 4),
            vec![TextMatch {
                start: 0,
                end: 1,
                text: "Hello World".to_string(),
            }]
        );
        let repeated = page_with(&["x", "x", "x"]);
        assert_eq!(
            repeated.find("x x", 8),
            vec![
                TextMatch {
                    start: 0,
                    end: 1,
                    text: "x x".to_string(),
                },
                TextMatch {
                    start: 1,
                    end: 2,
                    text: "x x".to_string(),
                },
            ]
        );
    }

    #[test]
    fn find_stops_at_requested_limit_and_caps_at_32() {
        let words: Vec<&str> = vec!["Hit"; 40];
        let page = page_with(&words);
        assert!(page.find("hit", 0).is_empty());
        let limited = page.find("hit", 2);
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].start, 0);
        assert_eq!(limited[1].end, 1);
        let capped = page.find("hit", 100);
        assert_eq!(capped.len(), 32);
        assert_eq!(capped[31].start, 31);
        assert_eq!(capped[31].end, 31);
        assert_eq!(capped[31].text, "Hit");
        assert_eq!(page.words.len(), 40);
    }

    #[test]
    fn find_returns_empty_when_nothing_matches() {
        let page = page_with(&["Hello", "World"]);
        assert!(page.find("goodbye", 8).is_empty());
        assert!(page.find("hell", 8).is_empty());
        assert!(page.find("hello  world", 8).is_empty());
        assert_eq!(page.text(), "Hello World");
    }
}
