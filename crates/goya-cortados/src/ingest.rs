use std::path::Path;

use crate::{Diagnostic, Limits};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluded {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub files: Vec<SourceFile>,
    pub excluded: Vec<Excluded>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn ingest(input: &Path, limits: &Limits) -> Result<Material, Vec<Diagnostic>> {
    #[cfg(target_os = "linux")]
    {
        linux::ingest(input, limits).map_err(|error| vec![*error])
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (input, limits);
        let mut diagnostic = Diagnostic::new(
            "COR-E500",
            ".",
            None,
            "secure source ingestion requires Linux descriptor-relative filesystem access",
        );
        diagnostic.remediation = "Run Cortados on a supported Linux host.".into();
        Err(vec![diagnostic])
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::BTreeSet;
    use std::ffi::{CStr, CString};
    use std::fs::File;
    use std::io::{self, Read};
    use std::mem::MaybeUninit;
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;
    use std::ptr::NonNull;
    use std::time::{Duration, Instant};

    use unicode_normalization::UnicodeNormalization;

    use super::*;

    type Result<T> = std::result::Result<T, Box<Diagnostic>>;
    const PIN: libc::c_int = libc::O_PATH | libc::O_NOFOLLOW;
    const ROOT: &str = ".";

    pub(super) fn ingest(input: &Path, limits: &Limits) -> Result<Material> {
        let root = open_input(input)?;
        let mut reader = Reader {
            limits,
            material: Material {
                files: Vec::new(),
                excluded: Vec::new(),
                diagnostics: Vec::new(),
            },
            entries: 0,
            total_bytes: 0,
            proc_fds: None,
        };
        match root.kind() {
            libc::S_IFDIR => {
                if let Some(name) = input.file_name() {
                    let name = portable_name(name.as_bytes(), ROOT)?;
                    if excluded_directory(&collision_key(name)) {
                        return Err(source_error("extract source into a project directory outside dependency and build trees"));
                    }
                }
                reader.walk(root)?;
            }
            libc::S_IFREG => {
                let name = input.file_name().expect("a regular input has a final name");
                let path = reader.relative_path(ROOT, name.as_bytes(), 1)?;
                let extension = input.extension().and_then(|ext| ext.to_str()).unwrap_or("");
                match extension.to_ascii_lowercase().as_str() {
                    "jar" | "class" | "war" | "aar" | "dex" | "zip" | "apk" | "tar" | "gz"
                    | "xz" | "bz2" | "7z" | "jmod" | "tgz" => {
                        return Err(source_error(
                            "archive and bytecode inputs are deferred by the source profile",
                        ));
                    }
                    _ if extension != "java" => {
                        return Err(source_error(
                            "a single input file must have the .java extension",
                        ));
                    }
                    _ => {}
                }
                reader.count_entry(ROOT)?;
                reader.read_file(path, root)?;
            }
            _ => return Err(security(ROOT, "input is a special file")),
        }

        if reader.material.files.is_empty() {
            return Err(source_error(
                "the supplied project contains no eligible regular files",
            ));
        }
        reader
            .material
            .files
            .sort_unstable_by(|a, b| a.path.cmp(&b.path));
        reader
            .material
            .excluded
            .sort_unstable_by(|a, b| a.path.cmp(&b.path));
        Ok(reader.material)
    }

    struct Reader<'a> {
        limits: &'a Limits,
        material: Material,
        entries: usize,
        total_bytes: usize,
        proc_fds: Option<OwnedFd>,
    }

    impl Reader<'_> {
        fn count_entry(&mut self, directory: &str) -> Result<()> {
            self.entries = self
                .entries
                .checked_add(1)
                .filter(|total| *total <= self.limits.max_entries)
                .ok_or_else(|| exhausted(directory, "max_entries"))?;
            Ok(())
        }

        fn relative_path(&self, parent: &str, name: &[u8], depth: usize) -> Result<String> {
            if depth > self.limits.max_path_depth {
                return Err(exhausted(parent, "max_path_depth"));
            }
            let path_bytes = if parent == ROOT {
                Some(name.len())
            } else {
                parent
                    .len()
                    .checked_add(1)
                    .and_then(|len| len.checked_add(name.len()))
            };
            if path_bytes.is_none_or(|len| len > self.limits.max_path_bytes) {
                return Err(exhausted(parent, "max_path_bytes"));
            }
            let name = portable_name(name, parent)?;
            Ok(if parent == ROOT {
                name.to_owned()
            } else {
                format!("{parent}/{name}")
            })
        }

        fn read_directory(&mut self, path: String, pinned: Pinned) -> Result<Directory> {
            let descriptor = open_at(
                pinned.fd(),
                c".",
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
            .map_err(|error| io_error(&path, "open directory", error))?;
            pinned.verify(&path)?;
            let mut dir = DirStream::new(descriptor)
                .map_err(|error| io_error(&path, "open directory stream", error))?;
            let mut names = Vec::new();
            while let Some(name) = dir
                .next()
                .map_err(|error| io_error(&path, "list directory", error))?
            {
                if name == c"." || name == c".." {
                    continue;
                }
                self.count_entry(&path)?;
                names.push(name.to_owned());
            }
            names.sort_unstable();
            pinned.verify(&path)?;
            Ok(Directory {
                path,
                pinned,
                names: names.into_iter(),
                keys: BTreeSet::new(),
            })
        }

        fn walk(&mut self, root: Pinned) -> Result<()> {
            let device = root.info.st_dev;
            let mut pending = vec![self.read_directory(ROOT.into(), root)?];
            while !pending.is_empty() {
                let depth = pending.len();
                let directory = pending.last_mut().expect("a directory is pending");
                let Some(name) = directory.names.next() else {
                    directory.pinned.verify(&directory.path)?;
                    pending.pop();
                    continue;
                };
                let path = self.relative_path(&directory.path, name.to_bytes(), depth)?;
                let key = collision_key(name.to_str().expect("the entry name was validated"));
                if !directory.keys.insert(key.clone()) {
                    return Err(security(
                        &path,
                        "case-normalized or Unicode-normalized input names collide",
                    ));
                }
                let entry = pin_at(directory.pinned.fd(), &name, &path, same_snapshot)?;
                let reason = match entry.kind() {
                    libc::S_IFLNK => "symbolic link",
                    _ if entry.info.st_dev != device => "entry on a different filesystem",
                    libc::S_IFDIR if excluded_directory(&key) => {
                        "dependency or generated directory"
                    }
                    libc::S_IFDIR => {
                        pending.push(self.read_directory(path, entry)?);
                        continue;
                    }
                    libc::S_IFREG => {
                        self.read_file(path, entry)?;
                        continue;
                    }
                    _ => "special file",
                };
                self.material.excluded.push(Excluded {
                    path,
                    reason: reason.into(),
                });
            }
            Ok(())
        }

        fn read_file(&mut self, path: String, pinned: Pinned) -> Result<()> {
            let started = Instant::now();
            let timeout = Duration::from_secs(self.limits.max_seconds_per_file);
            let expected = usize::try_from(pinned.info.st_size)
                .map_err(|_| exhausted(&path, "max_file_bytes"))?;
            if expected > self.limits.max_file_bytes {
                return Err(exhausted(&path, "max_file_bytes"));
            }
            let remaining = self.limits.max_total_bytes - self.total_bytes;
            if expected > remaining {
                return Err(exhausted(&path, "max_total_bytes"));
            }
            check_time(&path, started, timeout)?;

            if self.proc_fds.is_none() {
                self.proc_fds = Some(open_proc_fds(&path)?);
            }
            let proc_fds = self.proc_fds.as_ref().expect("the proc descriptor is open");
            let descriptor_name =
                CString::new(pinned.fd().to_string()).expect("file descriptor is numeric");
            // The kernel's descriptor link reopens the already-pinned regular inode. The
            // untrusted entry name is never opened for I/O, even after a concurrent rename.
            let descriptor = open_at(
                proc_fds.as_raw_fd(),
                &descriptor_name,
                libc::O_RDONLY | libc::O_NONBLOCK,
            )
            .map_err(|error| io_error(&path, "reopen verified regular file", error))?;
            let current = stat_fd(descriptor.as_raw_fd())
                .map_err(|error| io_error(&path, "inspect open file", error))?;
            if !same_snapshot(&pinned.info, &current) {
                return Err(changed(&path));
            }

            let mut file = File::from(descriptor);
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                check_time(&path, started, timeout)?;
                let request = (expected - bytes.len()).saturating_add(1).min(buffer.len());
                let read = match file.read(&mut buffer[..request]) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(io_error(&path, "read file", error)),
                };
                let length = bytes
                    .len()
                    .checked_add(read)
                    .ok_or_else(|| exhausted(&path, "max_file_bytes"))?;
                if length > self.limits.max_file_bytes {
                    return Err(exhausted(&path, "max_file_bytes"));
                }
                if length > remaining {
                    return Err(exhausted(&path, "max_total_bytes"));
                }
                if length > expected {
                    return Err(changed(&path));
                }
                if length > bytes.capacity() {
                    let capacity = bytes.capacity().saturating_mul(2).max(length).min(expected);
                    bytes
                        .try_reserve_exact(capacity - bytes.len())
                        .map_err(|_| exhausted(&path, "max_memory_bytes"))?;
                }
                bytes.extend_from_slice(&buffer[..read]);
            }
            if bytes.len() != expected {
                return Err(changed(&path));
            }
            pinned.verify(&path)?;
            check_time(&path, started, timeout)?;
            self.total_bytes = self
                .total_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| exhausted(&path, "max_total_bytes"))?;
            self.material.files.push(SourceFile { path, bytes });
            Ok(())
        }
    }

    struct Directory {
        path: String,
        pinned: Pinned,
        names: std::vec::IntoIter<CString>,
        keys: BTreeSet<String>,
    }

    struct Pinned {
        descriptor: OwnedFd,
        info: libc::stat,
    }

    impl Pinned {
        fn fd(&self) -> RawFd {
            self.descriptor.as_raw_fd()
        }

        fn kind(&self) -> libc::mode_t {
            self.info.st_mode & libc::S_IFMT
        }

        fn verify(&self, path: &str) -> Result<()> {
            let current =
                stat_fd(self.fd()).map_err(|error| io_error(path, "recheck input", error))?;
            if !same_snapshot(&self.info, &current) {
                return Err(changed(path));
            }
            Ok(())
        }
    }

    fn open_input(input: &Path) -> Result<Pinned> {
        if input.as_os_str().is_empty() {
            return Err(source_error("the input path is empty"));
        }
        let mut parts = input.components().peekable();
        let anchor = if parts.peek() == Some(&Component::RootDir) {
            parts.next();
            c"/"
        } else {
            c"."
        };
        let descriptor = open_at(libc::AT_FDCWD, anchor, PIN | libc::O_DIRECTORY)
            .map_err(|error| io_error(ROOT, "open input anchor", error))?;
        let info = stat_fd(descriptor.as_raw_fd())
            .map_err(|error| io_error(ROOT, "inspect input anchor", error))?;
        let mut pinned = Pinned { descriptor, info };
        while let Some(part) = parts.next() {
            let name = match part {
                Component::CurDir => continue,
                Component::Normal(name) => name,
                _ => {
                    return Err(security(
                        ROOT,
                        "input path contains a parent traversal or unsupported prefix",
                    ))
                }
            };
            portable_name(name.as_bytes(), ROOT)?;
            if pinned.kind() != libc::S_IFDIR {
                return Err(security(ROOT, "an input path component is not a directory"));
            }
            let name = CString::new(name.as_bytes()).expect("the input name was validated");
            // Ancestors only locate the input. Their unrelated entries can change
            // concurrently, while the selected root needs a stable snapshot.
            let compare = if parts.peek().is_some() {
                same_identity
            } else {
                same_snapshot
            };
            pinned = pin_at(pinned.fd(), &name, ROOT, compare)?;
            if pinned.kind() == libc::S_IFLNK {
                return Err(security(ROOT, "input path contains a symbolic link"));
            }
        }
        Ok(pinned)
    }

    fn pin_at(
        parent: RawFd,
        name: &CStr,
        path: &str,
        compare: fn(&libc::stat, &libc::stat) -> bool,
    ) -> Result<Pinned> {
        let observed = stat_at(parent, name)
            .map_err(|error| io_error(path, "inspect directory entry", error))?;
        let descriptor = open_at(parent, name, PIN)
            .map_err(|error| io_error(path, "pin directory entry", error))?;
        let info = stat_fd(descriptor.as_raw_fd())
            .map_err(|error| io_error(path, "inspect pinned entry", error))?;
        if !compare(&observed, &info) {
            return Err(changed(path));
        }
        Ok(Pinned { descriptor, info })
    }

    fn open_proc_fds(path: &str) -> Result<OwnedFd> {
        let descriptor = open_at(libc::AT_FDCWD, c"/proc/self/fd", PIN | libc::O_DIRECTORY)
            .map_err(|error| io_error(path, "open kernel descriptor directory", error))?;
        let mut info = MaybeUninit::<libc::statfs>::uninit();
        // fstatfs writes a statfs value when it succeeds, and borrows the open descriptor.
        if unsafe { libc::fstatfs(descriptor.as_raw_fd(), info.as_mut_ptr()) } != 0 {
            return Err(io_error(
                path,
                "inspect kernel descriptor directory",
                io::Error::last_os_error(),
            ));
        }
        // Linux's PROC_SUPER_MAGIC identifies the filesystem hosting the descriptor links.
        if unsafe { info.assume_init() }.f_type != 0x9fa0 {
            return Err(security(
                path,
                "secure file reopening requires procfs at /proc/self/fd",
            ));
        }
        Ok(descriptor)
    }

    fn open_at(parent: RawFd, name: &CStr, flags: libc::c_int) -> io::Result<OwnedFd> {
        // name is NUL-terminated and these flags never require a mode argument.
        let fd = unsafe { libc::openat(parent, name.as_ptr(), flags | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // The successful open transferred ownership of fd.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn stat_at(parent: RawFd, name: &CStr) -> io::Result<libc::stat> {
        let mut info = MaybeUninit::<libc::stat>::uninit();
        // fstatat writes a stat value on success and inspects the entry itself.
        if unsafe {
            libc::fstatat(
                parent,
                name.as_ptr(),
                info.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() })
    }

    fn stat_fd(descriptor: RawFd) -> io::Result<libc::stat> {
        let mut info = MaybeUninit::<libc::stat>::uninit();
        // fstat writes a stat value on success and borrows the descriptor.
        if unsafe { libc::fstat(descriptor, info.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() })
    }

    fn same_identity(a: &libc::stat, b: &libc::stat) -> bool {
        a.st_dev == b.st_dev
            && a.st_ino == b.st_ino
            && a.st_mode & libc::S_IFMT == b.st_mode & libc::S_IFMT
    }

    fn same_snapshot(a: &libc::stat, b: &libc::stat) -> bool {
        same_identity(a, b)
            && a.st_mode == b.st_mode
            && a.st_nlink == b.st_nlink
            && a.st_size == b.st_size
            && a.st_mtime == b.st_mtime
            && a.st_mtime_nsec == b.st_mtime_nsec
            && a.st_ctime == b.st_ctime
            && a.st_ctime_nsec == b.st_ctime_nsec
    }

    struct DirStream(NonNull<libc::DIR>);

    impl DirStream {
        fn new(descriptor: OwnedFd) -> io::Result<Self> {
            // fdopendir takes ownership only on success.
            let stream = unsafe { libc::fdopendir(descriptor.as_raw_fd()) };
            let stream = NonNull::new(stream).ok_or_else(io::Error::last_os_error)?;
            let _ = descriptor.into_raw_fd();
            Ok(Self(stream))
        }

        fn next(&mut self) -> io::Result<Option<&CStr>> {
            // A null result can mean either EOF or failure, distinguished by errno.
            let entry = unsafe {
                *libc::__errno_location() = 0;
                libc::readdir(self.0.as_ptr())
            };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(0) {
                    Ok(None)
                } else {
                    Err(error)
                };
            }
            // readdir supplies a NUL-terminated name on the open directory stream.
            Ok(Some(unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }))
        }
    }

    impl Drop for DirStream {
        fn drop(&mut self) {
            // This is the sole owner of the DIR pointer and its underlying descriptor.
            unsafe { libc::closedir(self.0.as_ptr()) };
        }
    }

    fn portable_name<'a>(bytes: &'a [u8], parent: &str) -> Result<&'a str> {
        let name = std::str::from_utf8(bytes)
            .map_err(|_| security(parent, "an input name is not valid UTF-8"))?;
        let stem = name
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        if name.is_empty()
            || name.ends_with([' ', '.'])
            || reserved
            || name.chars().any(|c| {
                c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            })
        {
            return Err(security(
                parent,
                "an input name contains a nonportable character or reserved path component",
            ));
        }
        Ok(name)
    }

    fn collision_key(name: &str) -> String {
        // Compare NFC names after Unicode upper/lower case mapping without rewriting origin names.
        name.nfc()
            .flat_map(char::to_uppercase)
            .flat_map(char::to_lowercase)
            .nfc()
            .collect()
    }

    fn excluded_directory(key: &str) -> bool {
        matches!(
            key,
            ".git" | "node_modules" | "target" | "build" | ".gradle"
        )
    }

    fn check_time(path: &str, started: Instant, timeout: Duration) -> Result<()> {
        if started.elapsed() >= timeout {
            return Err(exhausted(path, "max_seconds_per_file"));
        }
        Ok(())
    }

    fn exhausted(path: &str, limit: &str) -> Box<Diagnostic> {
        let mut diagnostic = security(path, format!("ingestion limit exceeded: {limit}"));
        diagnostic.remediation =
            format!("Reduce the supplied material or adjust {limit} for this input.");
        diagnostic
    }

    fn changed(path: &str) -> Box<Diagnostic> {
        security(path, "the filesystem entry changed during ingestion")
    }

    fn io_error(path: &str, action: &str, error: io::Error) -> Box<Diagnostic> {
        security(path, format!("could not {action}: {error}"))
    }

    fn security(path: &str, message: impl Into<String>) -> Box<Diagnostic> {
        let mut diagnostic = Diagnostic::new("COR-E500", path, None, message);
        diagnostic.remediation =
            "Supply a stable directory of portable, regular files or a single .java file.".into();
        Box::new(diagnostic)
    }

    fn source_error(message: &str) -> Box<Diagnostic> {
        let mut diagnostic = Diagnostic::new("COR-E100", ROOT, None, message);
        diagnostic.remediation = "Supply a Java source directory or a single .java file.".into();
        Box::new(diagnostic)
    }

    #[cfg(test)]
    mod tests {
        use std::ffi::OsString;
        use std::fs;
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::{symlink, PermissionsExt};
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use super::*;

        struct Fixture(PathBuf);

        impl Fixture {
            fn new() -> Self {
                static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
                let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
                loop {
                    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                    let path = temp.join(format!("cortados-ingest-{}-{id}", std::process::id()));
                    match fs::create_dir(&path) {
                        Ok(()) => return Self(path),
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                        Err(error) => panic!("fixture setup failed: {error}"),
                    }
                }
            }

            fn put(&self, path: impl AsRef<Path>, bytes: &[u8]) -> PathBuf {
                let path = self.0.join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, bytes).unwrap();
                path
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn inventories_exact_bytes_in_byte_order_without_running_project_scripts() {
            let project = Fixture::new();
            project.put("z/Z.java", b"class Z {}\r\n");
            project.put("src/A.java", b"class A {}\n");
            project.put("src-extra", &[0, 255, 17]);
            project.put("lib/opaque.jar", b"opaque archive");
            let script = project.put("gradlew", b"#!/bin/sh\n: > \"${0%/*}/executed\"\n");
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
            for dir in [".git", "node_modules", "target", "build", ".gradle"] {
                project.put(format!("{dir}/Forbidden.java"), b"not material");
            }
            project.put("pom.xml", b"<project/>");

            let material = crate::ingest::ingest(&project.0, &Limits::default()).unwrap();
            let paths: Vec<_> = material
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect();
            assert_eq!(
                paths,
                [
                    "gradlew",
                    "lib/opaque.jar",
                    "pom.xml",
                    "src-extra",
                    "src/A.java",
                    "z/Z.java"
                ]
            );
            assert_eq!(material.files[3].bytes, [0, 255, 17]);
            assert_eq!(material.files[5].bytes, b"class Z {}\r\n");
            let excluded: Vec<_> = material
                .excluded
                .iter()
                .map(|item| item.path.as_str())
                .collect();
            assert_eq!(
                excluded,
                [".git", ".gradle", "build", "node_modules", "target"]
            );
            assert!(material
                .excluded
                .iter()
                .all(|item| item.reason == "dependency or generated directory"));
            assert!(!project.0.join("executed").exists());
        }

        #[test]
        fn excludes_symlinks_and_fifos_and_rejects_symlink_inputs() {
            let project = Fixture::new();
            project.put("Real.java", b"class Real {}");
            let outside = Fixture::new();
            let secret = outside.put("Outside.java", b"class Outside {}");
            symlink(secret, project.0.join("FileLink.java")).unwrap();
            symlink(&outside.0, project.0.join("directory-link")).unwrap();
            let fifo = project.0.join("pipe.java");
            let fifo_name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
            // fifo_name is the NUL-terminated path under the test's private fixture.
            assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);

            let material = crate::ingest::ingest(&project.0, &Limits::default()).unwrap();
            assert_eq!(material.files.len(), 1);
            assert_eq!(material.files[0].path, "Real.java");
            let excluded: Vec<_> = material
                .excluded
                .iter()
                .map(|item| (item.path.as_str(), item.reason.as_str()))
                .collect();
            assert_eq!(
                excluded,
                [
                    ("FileLink.java", "symbolic link"),
                    ("directory-link", "symbolic link"),
                    ("pipe.java", "special file"),
                ]
            );

            for name in [
                "FileLink.java",
                "directory-link",
                "directory-link/Outside.java",
                "pipe.java",
            ] {
                let diagnostics =
                    crate::ingest::ingest(&project.0.join(name), &Limits::default()).unwrap_err();
                assert_eq!(diagnostics[0].code, "COR-E500", "{name}");
            }
        }

        #[test]
        fn rejects_archive_inputs_and_empty_directories() {
            let project = Fixture::new();
            let empty = crate::ingest::ingest(&project.0, &Limits::default()).unwrap_err();
            assert_eq!(empty[0].code, "COR-E100");
            assert!(!empty[0].remediation.is_empty());
            for name in [
                "example.jar",
                "example.class",
                "example.war",
                "example.aar",
                "example.dex",
            ] {
                let archive = project.put(name, b"opaque archive");
                let error = crate::ingest::ingest(&archive, &Limits::default()).unwrap_err();
                assert_eq!(error[0].code, "COR-E100");
                assert!(error[0].message.contains("deferred"));
            }
            let java = project.put("Only.java", b"class Only {}");
            let result = crate::ingest::ingest(&java, &Limits::default()).unwrap();
            assert_eq!(
                result.files,
                [SourceFile {
                    path: "Only.java".into(),
                    bytes: b"class Only {}".to_vec()
                }]
            );
        }

        #[test]
        fn enforces_entry_path_file_total_and_time_limits() {
            let project = Fixture::new();
            project.put("A.java", b"123");
            project.put("src/Z.java", b"1234");
            for (limits, name) in [
                (
                    Limits {
                        max_entries: 2,
                        ..Limits::default()
                    },
                    "max_entries",
                ),
                (
                    Limits {
                        max_path_depth: 1,
                        ..Limits::default()
                    },
                    "max_path_depth",
                ),
                (
                    Limits {
                        max_path_bytes: 6,
                        ..Limits::default()
                    },
                    "max_path_bytes",
                ),
                (
                    Limits {
                        max_file_bytes: 3,
                        ..Limits::default()
                    },
                    "max_file_bytes",
                ),
                (
                    Limits {
                        max_total_bytes: 6,
                        ..Limits::default()
                    },
                    "max_total_bytes",
                ),
                (
                    Limits {
                        max_seconds_per_file: 0,
                        ..Limits::default()
                    },
                    "max_seconds_per_file",
                ),
            ] {
                let diagnostics = crate::ingest::ingest(&project.0, &limits).unwrap_err();
                assert_eq!(diagnostics[0].code, "COR-E500");
                assert!(diagnostics[0].message.contains(name), "{diagnostics:?}");
            }
            let limits = Limits {
                max_entries: 3,
                max_path_depth: 2,
                max_path_bytes: 10,
                max_file_bytes: 4,
                max_total_bytes: 7,
                ..Limits::default()
            };
            assert_eq!(
                crate::ingest::ingest(&project.0, &limits)
                    .unwrap()
                    .files
                    .len(),
                2
            );
        }

        #[test]
        fn rejects_nonportable_names_and_normalized_collisions() {
            for name in [
                OsString::from_vec(vec![255]),
                "C:drive".into(),
                "a\\b".into(),
                "NUL.txt".into(),
                "trail. ".into(),
            ] {
                let project = Fixture::new();
                project.put(name, b"data");
                let error = crate::ingest::ingest(&project.0, &Limits::default()).unwrap_err();
                assert_eq!(error[0].code, "COR-E500");
                assert_eq!(error[0].path, ".");
            }
            for (first, second) in [("A.java", "a.java"), ("Café.java", "cafe\u{301}.java")] {
                let project = Fixture::new();
                project.put(first, b"data");
                project.put(second, b"data");
                let error = crate::ingest::ingest(&project.0, &Limits::default()).unwrap_err();
                assert_eq!(error[0].code, "COR-E500");
                assert!(error[0].message.contains("collide"));
            }
            let project = Fixture::new();
            project.put("src/Class.java", b"class Class {}");
            let input = project.0.join("src/../src");
            let diagnostics = crate::ingest::ingest(&input, &Limits::default()).unwrap_err();
            assert_eq!(diagnostics[0].code, "COR-E500");
            assert!(diagnostics[0].message.contains("parent traversal"));
        }

        #[test]
        fn rejects_file_and_directory_changes_after_pinning() {
            let project = Fixture::new();
            let first = project.put("A.java", b"original");
            let pinned = open_input(&first).ok().unwrap();
            let second = project.put("B.java", b"replacement");
            fs::remove_file(&first).unwrap();
            symlink(second, &first).unwrap();
            let error = pinned.verify("A.java").unwrap_err();
            assert!(error.message.contains("changed"));

            let pinned = open_input(&project.0).ok().unwrap();
            project.put("another.java", b"new");
            let error = pinned.verify(".").unwrap_err();
            assert!(error.message.contains("changed"));
        }

        #[test]
        fn ignores_unrelated_changes_in_input_ancestors() {
            let ancestor = Fixture::new();
            let source = ancestor.put("project/A.java", b"class A {}");
            let sibling = ancestor.0.join("unrelated");
            let start = std::sync::Barrier::new(2);
            std::thread::scope(|threads| {
                threads.spawn(|| {
                    start.wait();
                    for _ in 0..256 {
                        fs::create_dir(&sibling).unwrap();
                        fs::remove_dir(&sibling).unwrap();
                    }
                });
                start.wait();
                for _ in 0..128 {
                    let material = crate::ingest::ingest(&source, &Limits::default()).unwrap();
                    assert_eq!(material.files[0].path, "A.java");
                }
            });
        }
    }
}
