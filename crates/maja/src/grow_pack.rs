//! Copy an existing experimental static directory into a Grow static package.
//! No scene is compiled, Grow is not invoked, and no browser logic is added.

use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    io::{Read, Write},
    path::{Component, Path},
};

#[derive(Debug)]
pub(crate) enum Failure {
    Input(String),
    Reject(String),
    Output(String),
}

struct Copied {
    index: Vec<u8>,
    css: Vec<u8>,
    manifest: Vec<u8>,
}

const INDEX_HTML: &str = "index.html";
const APP_CSS: &str = "app.css";
const MANIFEST_JSON: &str = "manifest.json";
const MAX_INPUT_FILE_BYTES: u64 = 1024 * 1024;

/// Package `input` into a new directory at `output`.
///
/// `name` and `version` are the explicit CLI strings. They are not read from
/// the static manifest.
pub(crate) fn package(
    input: &Path,
    output: &Path,
    name: &str,
    version: &str,
) -> Result<(), Failure> {
    validate_identity(name, version)?;
    let files = read_input(input)?;
    write_output(output, &files, name, version)
}

fn validate_identity(name: &str, version: &str) -> Result<(), Failure> {
    validate_part("name", name, true)?;
    validate_part("version", version, false)
}

fn validate_part(label: &str, value: &str, limit_name: bool) -> Result<(), Failure> {
    if value.bytes().any(forbidden_identity_byte) {
        return Err(Failure::Reject(format!(
            "{label} contains a newline or quote"
        )));
    }
    if value.trim().is_empty() {
        return Err(Failure::Reject(format!("{label} is empty")));
    }
    if limit_name && value.len() > 128 {
        return Err(Failure::Reject(format!("{label} exceeds 128 bytes")));
    }
    Ok(())
}

/// Characters that would break the quoted TOML strings this package writes.
fn forbidden_identity_byte(byte: u8) -> bool {
    byte == b'"' || byte == b'\'' || byte == b'\\' || byte == 0x7f || (byte < 0x20 && byte != b'\t')
}

fn read_input(input: &Path) -> Result<Copied, Failure> {
    let metadata = fs::symlink_metadata(input)
        .map_err(|error| Failure::Input(format!("cannot read input: {error}")))?;
    if metadata.file_type().is_symlink() {
        return Err(Failure::Reject("symlink escapes are rejected".into()));
    }
    if !metadata.is_dir() {
        return Err(Failure::Input(
            "cannot read input: input is not a directory".into(),
        ));
    }

    let mut saw_index = false;
    let mut saw_css = false;
    let mut saw_manifest = false;
    for entry in fs::read_dir(input)
        .map_err(|error| Failure::Input(format!("cannot read input: {error}")))?
    {
        let entry = entry.map_err(|error| Failure::Input(format!("cannot read input: {error}")))?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            return Err(Failure::Reject("unexpected packaged input file".into()));
        };
        if let Err(message) = contained_name(name) {
            return Err(Failure::Reject(message.into()));
        }
        match name {
            INDEX_HTML => saw_index = true,
            APP_CSS => saw_css = true,
            MANIFEST_JSON => saw_manifest = true,
            _ => return Err(Failure::Reject("unexpected packaged input file".into())),
        }
    }
    if !saw_index {
        return Err(Failure::Input(
            "cannot read input: missing index.html".into(),
        ));
    }
    if !saw_css {
        return Err(Failure::Input("cannot read input: missing app.css".into()));
    }
    if !saw_manifest {
        return Err(Failure::Input(
            "cannot read input: missing manifest.json".into(),
        ));
    }

    let index = read_regular(input, INDEX_HTML, MAX_INPUT_FILE_BYTES)?;
    let css = read_regular(input, APP_CSS, MAX_INPUT_FILE_BYTES)?;
    let manifest = read_regular(input, MANIFEST_JSON, MAX_INPUT_FILE_BYTES)?;
    admit_manifest(&manifest)?;
    Ok(Copied {
        index,
        css,
        manifest,
    })
}

fn contained_name(name: &str) -> Result<(), &'static str> {
    let path = Path::new(name);
    if path.is_absolute() {
        return Err("absolute paths are rejected");
    }
    let mut parts = path.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(part)), None) if part.to_str() == Some(name) => Ok(()),
        (Some(Component::ParentDir), _) => Err("parent paths are rejected"),
        _ => Err("path escapes package root"),
    }
}

fn read_regular(root: &Path, name: &str, max_bytes: u64) -> Result<Vec<u8>, Failure> {
    contained_name(name).map_err(|message| Failure::Reject(message.into()))?;
    let path = root.join(name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| Failure::Input(format!("cannot read input: {name}: {error}")))?;
    if metadata.file_type().is_symlink() {
        return Err(Failure::Reject("symlink escapes are rejected".into()));
    }
    if !metadata.is_file() {
        return Err(Failure::Input(format!(
            "cannot read input: {name} is not a regular file"
        )));
    }
    if metadata.len() > max_bytes {
        return Err(Failure::Reject(format!(
            "{name} exceeds the 1 MiB packaged input limit"
        )));
    }
    let file = open_regular(&path).map_err(|error| {
        if is_symlink_open_error(&error) {
            Failure::Reject("symlink escapes are rejected".into())
        } else {
            Failure::Input(format!("cannot read input: {name}: {error}"))
        }
    })?;
    // Bound the actual read as well as the earlier metadata check: a file may
    // grow after metadata is inspected.
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Failure::Input(format!("cannot read input: {name}: {error}")))?;
    if bytes.len() as u64 > max_bytes {
        return Err(Failure::Reject(format!(
            "{name} exceeds the 1 MiB packaged input limit"
        )));
    }
    ensure_inside(root, &path)?;
    Ok(bytes)
}

fn open_regular(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux O_NOFOLLOW: do not follow a symlink that replaces the file.
        options.custom_flags(0o400000);
    }
    options.open(path)
}

fn is_symlink_open_error(error: &io::Error) -> bool {
    // Linux ELOOP. `ErrorKind::FilesystemLoop` is still unstable here.
    error.raw_os_error() == Some(40)
}

fn ensure_inside(root: &Path, file: &Path) -> Result<(), Failure> {
    let root = root
        .canonicalize()
        .map_err(|error| Failure::Input(format!("cannot read input: {error}")))?;
    let file = file
        .canonicalize()
        .map_err(|error| Failure::Input(format!("cannot read input: {error}")))?;
    if file.starts_with(&root) && file != root {
        Ok(())
    } else {
        Err(Failure::Reject("symlink escapes are rejected".into()))
    }
}

fn admit_manifest(bytes: &[u8]) -> Result<(), Failure> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Failure::Input("cannot read input: manifest.json is invalid".into()))?;
    let fields = parse_top_level(text)
        .map_err(|_| Failure::Input("cannot read input: manifest.json is invalid".into()))?;
    match fields.get("profile") {
        Some(Some(value)) if value == "maja.web.local.v1" => {}
        _ => return Err(Failure::Reject("profile is not maja.web.local.v1".into())),
    }
    match fields.get("qualification") {
        Some(Some(value)) if value == "experimental" => Ok(()),
        _ => Err(Failure::Reject("qualification is not experimental".into())),
    }
}

fn write_output(output: &Path, files: &Copied, name: &str, version: &str) -> Result<(), Failure> {
    if fs::symlink_metadata(output).is_ok() {
        return Err(Failure::Output(
            "output failed: output already exists".into(),
        ));
    }
    create_private_dir(output).map_err(map_create)?;
    let result = write_members(output, files, name, version);
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

fn map_create(error: io::Error) -> Failure {
    if error.kind() == io::ErrorKind::AlreadyExists {
        Failure::Output("output failed: output already exists".into())
    } else if error.kind() == io::ErrorKind::NotFound {
        Failure::Output("output failed: output parent does not exist".into())
    } else {
        Failure::Output(format!("output failed: {error}"))
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn write_members(output: &Path, files: &Copied, name: &str, version: &str) -> Result<(), Failure> {
    // Grow serves repository-root index.html. The copied files stay at the root.
    write_new(&output.join(INDEX_HTML), &files.index)?;
    write_new(&output.join(APP_CSS), &files.css)?;
    write_new(&output.join(MANIFEST_JSON), &files.manifest)?;
    create_private_dir(&output.join(".grow")).map_err(map_create)?;
    write_new(
        &output.join(".grow").join("manifest.toml"),
        manifest_toml(name, version).as_bytes(),
    )?;
    create_private_dir(&output.join("metadata")).map_err(map_create)?;
    write_new(
        &output.join("metadata").join("goya-build.json"),
        build_record(files).as_bytes(),
    )?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| Failure::Output(format!("output failed: {error}")))?;
    file.write_all(bytes)
        .map_err(|error| Failure::Output(format!("output failed: {error}")))
}

fn manifest_toml(name: &str, version: &str) -> String {
    format!("[grow]\nname = \"{name}\"\nversion = \"{version}\"\n[network]\nenabled = false\n")
}

fn build_record(files: &Copied) -> String {
    let html = hex(&Sha256::digest(&files.index));
    let css = hex(&Sha256::digest(&files.css));
    let manifest = hex(&Sha256::digest(&files.manifest));
    format!(
        "{{\n  \"profile\": \"maja.grow.local.v1\",\n  \"qualification\": \"experimental\",\n  \"browser_logic\": \"this package adds no browser logic\",\n  \"artifacts_sha256\": {{\n    \"index.html\": \"{html}\",\n    \"app.css\": \"{css}\",\n    \"manifest.json\": \"{manifest}\"\n  }}\n}}\n"
    )
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}

fn parse_top_level(text: &str) -> Result<BTreeMap<String, Option<String>>, ()> {
    let mut index = 0;
    skip_ws(text, &mut index);
    let fields = parse_object(text, &mut index)?;
    skip_ws(text, &mut index);
    if index != text.len() {
        return Err(());
    }
    Ok(fields)
}

fn parse_object(text: &str, index: &mut usize) -> Result<BTreeMap<String, Option<String>>, ()> {
    expect_byte(text, index, b'{')?;
    let mut fields = BTreeMap::new();
    skip_ws(text, index);
    if peek(text, *index) == Some(b'}') {
        *index += 1;
        return Ok(fields);
    }
    loop {
        skip_ws(text, index);
        let key = parse_string(text, index)?;
        skip_ws(text, index);
        expect_byte(text, index, b':')?;
        let value = parse_value(text, index)?;
        if fields.insert(key, value).is_some() {
            return Err(());
        }
        skip_ws(text, index);
        match peek(text, *index) {
            Some(b',') => {
                *index += 1;
            }
            Some(b'}') => {
                *index += 1;
                return Ok(fields);
            }
            _ => return Err(()),
        }
    }
}

fn parse_array(text: &str, index: &mut usize) -> Result<(), ()> {
    expect_byte(text, index, b'[')?;
    skip_ws(text, index);
    if peek(text, *index) == Some(b']') {
        *index += 1;
        return Ok(());
    }
    loop {
        parse_value(text, index)?;
        skip_ws(text, index);
        match peek(text, *index) {
            Some(b',') => *index += 1,
            Some(b']') => {
                *index += 1;
                return Ok(());
            }
            _ => return Err(()),
        }
    }
}

fn parse_value(text: &str, index: &mut usize) -> Result<Option<String>, ()> {
    skip_ws(text, index);
    match peek(text, *index) {
        Some(b'"') => Ok(Some(parse_string(text, index)?)),
        Some(b'{') => {
            parse_object(text, index)?;
            Ok(None)
        }
        Some(b'[') => {
            parse_array(text, index)?;
            Ok(None)
        }
        Some(b't') => {
            expect_literal(text, index, "true")?;
            Ok(None)
        }
        Some(b'f') => {
            expect_literal(text, index, "false")?;
            Ok(None)
        }
        Some(b'n') => {
            expect_literal(text, index, "null")?;
            Ok(None)
        }
        Some(b'-') | Some(b'0'..=b'9') => {
            skip_number(text, index)?;
            Ok(None)
        }
        _ => Err(()),
    }
}

fn parse_string(text: &str, index: &mut usize) -> Result<String, ()> {
    expect_byte(text, index, b'"')?;
    let bytes = text.as_bytes();
    let mut out = String::new();
    while *index < bytes.len() {
        let byte = bytes[*index];
        if byte == b'"' {
            *index += 1;
            return Ok(out);
        }
        if byte == b'\\' {
            *index += 1;
            let escaped = bytes.get(*index).copied().ok_or(())?;
            match escaped {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{0008}'),
                b'f' => out.push('\u{000c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    *index += 1;
                    push_unicode_escape(text, index, &mut out)?;
                    continue;
                }
                _ => return Err(()),
            }
            *index += 1;
            continue;
        }
        if byte < 0x20 {
            return Err(());
        }
        let ch = text[*index..].chars().next().ok_or(())?;
        out.push(ch);
        *index += ch.len_utf8();
    }
    Err(())
}

fn push_unicode_escape(text: &str, index: &mut usize, out: &mut String) -> Result<(), ()> {
    let unit = hex4(text, index)?;
    if (0xD800..=0xDBFF).contains(&unit) {
        if text.as_bytes().get(*index) != Some(&b'\\')
            || text.as_bytes().get(*index + 1) != Some(&b'u')
        {
            return Err(());
        }
        *index += 2;
        let low = hex4(text, index)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Err(());
        }
        let code = 0x10000 + (((unit - 0xD800) << 10) | (low - 0xDC00));
        out.push(char::from_u32(code).ok_or(())?);
        return Ok(());
    }
    if (0xDC00..=0xDFFF).contains(&unit) {
        return Err(());
    }
    out.push(char::from_u32(unit).ok_or(())?);
    Ok(())
}

fn hex4(text: &str, index: &mut usize) -> Result<u32, ()> {
    let end = index.checked_add(4).ok_or(())?;
    let digits = text.get(*index..end).ok_or(())?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(());
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| ())?;
    *index = end;
    Ok(value)
}

fn skip_number(text: &str, index: &mut usize) -> Result<(), ()> {
    let bytes = text.as_bytes();
    if bytes.get(*index) == Some(&b'-') {
        *index += 1;
    }
    if bytes.get(*index) == Some(&b'0') {
        *index += 1;
    } else if bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
        while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
            *index += 1;
        }
    } else {
        return Err(());
    }
    if bytes.get(*index) == Some(&b'.') {
        *index += 1;
        let start = *index;
        while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
            *index += 1;
        }
        if *index == start {
            return Err(());
        }
    }
    if bytes
        .get(*index)
        .is_some_and(|byte| *byte == b'e' || *byte == b'E')
    {
        *index += 1;
        if bytes
            .get(*index)
            .is_some_and(|byte| *byte == b'+' || *byte == b'-')
        {
            *index += 1;
        }
        let start = *index;
        while bytes.get(*index).is_some_and(|byte| byte.is_ascii_digit()) {
            *index += 1;
        }
        if *index == start {
            return Err(());
        }
    }
    Ok(())
}

fn expect_literal(text: &str, index: &mut usize, literal: &str) -> Result<(), ()> {
    let end = index.checked_add(literal.len()).ok_or(())?;
    if text.get(*index..end) != Some(literal) {
        return Err(());
    }
    *index = end;
    Ok(())
}

fn expect_byte(text: &str, index: &mut usize, byte: u8) -> Result<(), ()> {
    if peek(text, *index) != Some(byte) {
        return Err(());
    }
    *index += 1;
    Ok(())
}

fn peek(text: &str, index: usize) -> Option<u8> {
    text.as_bytes().get(index).copied()
}

fn skip_ws(text: &str, index: &mut usize) {
    while matches!(peek(text, *index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        *index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    const STATIC_MANIFEST: &str = "{\n  \"schema\": \"MAJA-STATIC/1-draft\",\n  \"compiler\": \"0.0.1\",\n  \"profile\": \"maja.web.local.v1\",\n  \"qualification\": \"experimental\",\n  \"source_sha256\": \"abc\",\n  \"text_source_bytes\": [ 0, 4 ],\n  \"artifacts_sha256\": { \"index.html\": \"aa\", \"app.css\": \"bb\" }\n}\n";

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("maja-grow-pack-{}-{n}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_input(dir: &Path, manifest: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let html = b"<!doctype html>\n<main>static</main>\n".to_vec();
        let css = b"main{overflow:hidden}\n".to_vec();
        let manifest = manifest.as_bytes().to_vec();
        fs::write(dir.join(INDEX_HTML), &html).unwrap();
        fs::write(dir.join(APP_CSS), &css).unwrap();
        fs::write(dir.join(MANIFEST_JSON), &manifest).unwrap();
        (html, css, manifest)
    }

    fn run_package(
        input: &Path,
        output: &Path,
        name: &str,
        version: &str,
    ) -> (i32, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = crate::run(
            [
                "maja",
                "package",
                "grow",
                "--input",
                &input.display().to_string(),
                "--out",
                &output.display().to_string(),
                "--name",
                name,
                "--version",
                version,
            ]
            .into_iter()
            .map(OsString::from),
            &mut stdout,
            &mut stderr,
        );
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    fn relative_files(root: &Path) -> Vec<String> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        files.sort();
        files
    }

    #[test]
    fn happy_path_copies_bytes_and_records_experimental_grow_metadata() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        let (html, css, manifest) = write_input(&input, STATIC_MANIFEST);

        let (code, stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stdout, "packaged: experimental static directory\n");
        assert!(stderr.is_empty(), "{stderr}");
        assert_eq!(fs::read(output.join(INDEX_HTML)).unwrap(), html);
        assert_eq!(fs::read(output.join(APP_CSS)).unwrap(), css);
        assert_eq!(fs::read(output.join(MANIFEST_JSON)).unwrap(), manifest);
        assert_eq!(
            relative_files(&output),
            vec![
                ".grow/manifest.toml".to_string(),
                "app.css".to_string(),
                "index.html".to_string(),
                "manifest.json".to_string(),
                "metadata/goya-build.json".to_string(),
            ]
        );
        let record = fs::read_to_string(output.join("metadata/goya-build.json")).unwrap();
        assert!(record.contains("\"profile\": \"maja.grow.local.v1\""));
        assert!(record.contains("\"qualification\": \"experimental\""));
        assert!(record.contains("this package adds no browser logic"));
        assert!(record.contains(&hex(&Sha256::digest(&html))));
        assert!(record.contains(&hex(&Sha256::digest(&css))));
        assert!(record.contains(&hex(&Sha256::digest(&manifest))));
        assert!(!record.contains('/'));
        assert!(!record.contains('\\'));
        assert!(!record.to_ascii_lowercase().contains("timestamp"));
        assert!(!record.contains("runtime"));
        assert!(!record.contains("wasi"));
        let lower = stdout.to_ascii_lowercase();
        assert!(!lower.contains("install"));
        assert!(!lower.contains("serve"));
        assert!(!lower.contains("production"));
        assert!(!lower.contains("mfi"));
    }

    #[test]
    fn missing_index_html_is_an_input_error() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        fs::write(input.join(APP_CSS), b"main{}\n").unwrap();
        fs::write(input.join(MANIFEST_JSON), STATIC_MANIFEST).unwrap();

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 3, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0004"), "{stderr}");
        assert!(stderr.contains("index.html"), "{stderr}");
        assert!(!output.exists());
    }

    #[test]
    fn wrong_profile_is_rejected() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        let manifest = STATIC_MANIFEST.replace(
            "\"profile\": \"maja.web.local.v1\"",
            "\"profile\": \"maja.web.v1\"",
        );
        write_input(&input, &manifest);

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 4, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0007"), "{stderr}");
        assert!(!output.exists());
    }

    #[test]
    fn written_toml_has_no_runtime() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        write_input(&input, STATIC_MANIFEST);

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 0, "{stderr}");
        let toml = fs::read_to_string(output.join(".grow/manifest.toml")).unwrap();
        assert_eq!(
            toml,
            "[grow]\nname = \"cli-name\"\nversion = \"9.9.9\"\n[network]\nenabled = false\n"
        );
        assert!(!toml.contains("[runtime]"));
        assert!(!toml.contains("[permissions]"));
        assert!(!toml.contains("wasm"));
    }

    #[test]
    fn existing_output_is_rejected() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        write_input(&input, STATIC_MANIFEST);
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep"), b"sentinel").unwrap();

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 5, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0005"), "{stderr}");
        assert_eq!(fs::read(output.join("keep")).unwrap(), b"sentinel");
        assert!(!output.join(INDEX_HTML).exists());
        assert!(!output.join(".grow").exists());
    }

    #[test]
    fn empty_name_is_rejected() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        write_input(&input, STATIC_MANIFEST);

        let (code, _stdout, stderr) = run_package(&input, &output, "", "9.9.9");

        assert_eq!(code, 4, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0007"), "{stderr}");
        assert!(!output.exists());
    }

    #[test]
    fn malformed_manifest_json_is_rejected_before_output_creation() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        let malformed = STATIC_MANIFEST.replace(
            "\"qualification\": \"experimental\"",
            "\"qualification\": \"experimental\",\n  \"profile\": \"maja.web.local.v1\"",
        );
        write_input(&input, &malformed);

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 3, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0004"), "{stderr}");
        assert!(stderr.contains("manifest.json is invalid"), "{stderr}");
        assert!(!output.exists());
    }

    #[test]
    fn malformed_unicode_escape_in_manifest_is_rejected() {
        let malformed = STATIC_MANIFEST.replace("maja.web.local.v1", "maja.web.local.\\uD800");
        let result = admit_manifest(malformed.as_bytes());
        assert!(
            matches!(result, Err(Failure::Input(message)) if message.contains("manifest.json is invalid"))
        );
    }

    #[test]
    fn oversized_static_member_is_rejected_without_output() {
        let scratch = Scratch::new();
        let input = scratch.0.join("input");
        let output = scratch.0.join("output");
        fs::create_dir(&input).unwrap();
        write_input(&input, STATIC_MANIFEST);
        fs::write(input.join(APP_CSS), vec![b'x'; 1024 * 1024 + 1]).unwrap();

        let (code, _stdout, stderr) = run_package(&input, &output, "cli-name", "9.9.9");

        assert_eq!(code, 4, "{stderr}");
        assert!(stderr.contains("MAJA-CLI-E0007"), "{stderr}");
        assert!(stderr.contains("app.css exceeds the 1 MiB"), "{stderr}");
        assert!(!output.exists());
    }
}
