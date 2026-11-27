//! CLI orchestration: only this executable's worker is spawned, never Java input.
use goya_cortados::{analysis, Diagnostic, Limits, PROFILE};
use std::collections::BTreeMap;
use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    limits: Limits,
    wall_seconds: u64,
    worker: bool,
}

fn diagnostic(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::new(code, ".", None, message)]
}

fn parse(args: &[String]) -> Result<Option<Options>, Vec<Diagnostic>> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!(
            "cortados analyse INPUT --output NEW_DIRECTORY [--profile {PROFILE}]\n\
                  Static, offline Java 17 source analysis. No native conversion.\n\
                  Limits may only be lowered: --max-entries, --max-file-bytes,\n\
                  --max-total-bytes, --max-depth, --max-tokens, --max-ast-depth,\n\
                  --file-seconds, --memory-mib, --wall-seconds (default 120).\n\
                  Exit 0: syntax analysis completed (inspect semantic completeness);\n\
                  exit 4: usage, input, syntax, policy, budget or output failure."
        );
        return Ok(None);
    }
    if args == ["--version"] {
        println!("cortados {} ({PROFILE})", env!("CARGO_PKG_VERSION"));
        return Ok(None);
    }
    if args[0] != "analyse" {
        return Err(diagnostic("COR-E200", "Only static `analyse` is implemented; conversion, verification and certification are unavailable."));
    }
    let input = args
        .get(1)
        .filter(|s| !s.starts_with('-'))
        .ok_or_else(|| diagnostic("COR-E100", "analyse requires an input path"))?;
    let mut options = Options {
        input: input.into(),
        output: PathBuf::new(),
        limits: Limits::default(),
        wall_seconds: 120,
        worker: false,
    };
    let mut i = 2;
    let mut seen = std::collections::BTreeSet::new();
    while i < args.len() {
        let key = args[i].as_str();
        if !seen.insert(key) {
            return Err(diagnostic("COR-E100", format!("duplicate option: {key}")));
        }
        if key == "--worker" {
            if std::env::var_os("CORTADOS_INTERNAL_WORKER").as_deref() != Some(OsStr::new("1")) {
                return Err(diagnostic(
                    "COR-E500",
                    "--worker is reserved for the CLI supervisor",
                ));
            }
            options.worker = true;
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| diagnostic("COR-E100", format!("{key} needs a value")))?;
        match key {
            "--output" => options.output = value.into(),
            "--profile" if value == PROFILE => {}
            "--profile" => return Err(diagnostic("COR-E200", "unknown compatibility profile")),
            _ => {
                let number = value.parse::<u64>().map_err(|_| {
                    diagnostic("COR-E100", format!("{key} requires a positive integer"))
                })?;
                if number == 0 {
                    return Err(diagnostic("COR-E500", "limits must be positive"));
                }
                let defaults = Limits::default();
                let (slot, maximum): (&mut usize, usize) = match key {
                    "--max-entries" => (&mut options.limits.max_entries, defaults.max_entries),
                    "--max-file-bytes" => {
                        (&mut options.limits.max_file_bytes, defaults.max_file_bytes)
                    }
                    "--max-total-bytes" => (
                        &mut options.limits.max_total_bytes,
                        defaults.max_total_bytes,
                    ),
                    "--max-depth" => (&mut options.limits.max_path_depth, defaults.max_path_depth),
                    "--max-tokens" => (
                        &mut options.limits.max_tokens_per_file,
                        defaults.max_tokens_per_file,
                    ),
                    "--max-ast-depth" => {
                        (&mut options.limits.max_ast_depth, defaults.max_ast_depth)
                    }
                    "--file-seconds" if number <= defaults.max_seconds_per_file => {
                        options.limits.max_seconds_per_file = number;
                        i += 2;
                        continue;
                    }
                    "--memory-mib" if number <= defaults.max_memory_bytes / (1024 * 1024) => {
                        options.limits.max_memory_bytes = number * 1024 * 1024;
                        i += 2;
                        continue;
                    }
                    "--wall-seconds" if number <= 120 => {
                        options.wall_seconds = number;
                        i += 2;
                        continue;
                    }
                    "--file-seconds" | "--memory-mib" | "--wall-seconds" => {
                        return Err(diagnostic(
                            "COR-E500",
                            "requested limit exceeds profile maximum",
                        ))
                    }
                    _ => return Err(diagnostic("COR-E100", format!("unknown option: {key}"))),
                };
                *slot = usize::try_from(number)
                    .ok()
                    .filter(|n| *n <= maximum)
                    .ok_or_else(|| {
                        diagnostic("COR-E500", "requested limit exceeds profile maximum")
                    })?;
            }
        }
        i += 2;
    }
    if options.output.as_os_str().is_empty() {
        return Err(diagnostic(
            "COR-E100",
            "--output must name a new directory whose parent exists",
        ));
    }
    Ok(Some(options))
}

#[cfg(target_os = "linux")]
fn confine(limits: &Limits, wall_seconds: u64) -> Result<(), Vec<Diagnostic>> {
    for (resource, value) in [
        (libc::RLIMIT_AS, limits.max_memory_bytes),
        (libc::RLIMIT_CPU, wall_seconds),
        (libc::RLIMIT_CORE, 0),
        (libc::RLIMIT_FSIZE, limits.max_total_bytes as u64),
    ] {
        let mut inherited = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: getrlimit writes the resource limits to a valid output pointer.
        if unsafe { libc::getrlimit(resource, inherited.as_mut_ptr()) } != 0 {
            return Err(diagnostic(
                "COR-E500",
                "cannot inspect inherited resource limit",
            ));
        }
        let inherited = unsafe { inherited.assume_init() };
        let requested = (value as libc::rlim_t).min(inherited.rlim_max);
        let limit = libc::rlimit {
            rlim_cur: requested,
            rlim_max: requested,
        };
        // SAFETY: valid rlimit pointer; this worker changes only its own limits.
        if unsafe { libc::setrlimit(resource, &limit) } != 0 {
            return Err(diagnostic(
                "COR-E500",
                format!(
                    "cannot apply process limit: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
    }
    // SAFETY: prctl receives the documented scalar arguments, no pointers.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(diagnostic(
            "COR-E500",
            "cannot prohibit privilege acquisition",
        ));
    }
    // SAFETY: this worker is single-threaded; EFBIG will permit normal cleanup.
    if unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) } == libc::SIG_ERR {
        return Err(diagnostic(
            "COR-E500",
            "cannot configure bounded output writes",
        ));
    }
    let mut event: libc::sigevent = unsafe { std::mem::zeroed() };
    event.sigev_notify = libc::SIGEV_SIGNAL;
    event.sigev_signo = libc::SIGKILL;
    let mut timer = std::mem::MaybeUninit::<libc::timer_t>::uninit();
    // SAFETY: both pointers refer to valid, writable POSIX timer structures.
    if unsafe { libc::timer_create(libc::CLOCK_MONOTONIC, &mut event, timer.as_mut_ptr()) } != 0 {
        return Err(diagnostic(
            "COR-E500",
            "cannot create worker wall-time limit",
        ));
    }
    let timeout = libc::itimerspec {
        it_interval: libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        it_value: libc::timespec {
            tv_sec: wall_seconds as libc::time_t,
            tv_nsec: 0,
        },
    };
    // SAFETY: timer_create succeeded; timeout is a valid positive duration.
    if unsafe { libc::timer_settime(timer.assume_init(), 0, &timeout, std::ptr::null_mut()) } != 0 {
        return Err(diagnostic("COR-E500", "cannot arm worker wall-time limit"));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn confine(_: &Limits, _: u64) -> Result<(), Vec<Diagnostic>> {
    Err(diagnostic(
        "COR-E500",
        "this analysis worker requires Linux process and filesystem protections",
    ))
}

#[cfg(target_os = "linux")]
fn write_bundle(output: &Path, artifacts: &BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    write_bundle_with_sync(output, artifacts, File::sync_all)
}

#[cfg(target_os = "linux")]
fn write_bundle_with_sync(
    output: &Path,
    artifacts: &BTreeMap<String, Vec<u8>>,
    sync_parent: impl FnOnce(&File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    fn string(value: &OsStr) -> std::io::Result<CString> {
        CString::new(value.as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in output path")
        })
    }
    fn open_dir(fd: i32, name: &CString) -> std::io::Result<File> {
        // SAFETY: name is NUL-terminated; openat returns a new owned descriptor.
        let result = unsafe {
            libc::openat(
                fd,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: result is a valid new file descriptor owned by this function.
        Ok(unsafe { File::from_raw_fd(result) })
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let leaf = output.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "output needs a directory name",
        )
    })?;
    let leaf = string(leaf)?;
    let mut anchor = open_dir(
        libc::AT_FDCWD,
        &string(OsStr::new(if output.is_absolute() { "/" } else { "." }))?,
    )?;
    for part in parent.components() {
        match part {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => anchor = open_dir(anchor.as_raw_fd(), &string(name)?)?,
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "output parent traversal is refused",
                ))
            }
        }
    }
    let mut parent_info = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fstat borrows the live descriptor and writes a valid stat on success.
    if unsafe { libc::fstat(anchor.as_raw_fd(), parent_info.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let parent_info = unsafe { parent_info.assume_init() };
    // Host users sharing our UID, and root, are trusted. A sticky shared parent
    // protects our staging name from other UIDs; nonsticky shared writes do not.
    let trusted_owner = parent_info.st_uid == unsafe { libc::geteuid() } || parent_info.st_uid == 0;
    let shared_write = parent_info.st_mode & 0o022 != 0;
    if !trusted_owner || (shared_write && parent_info.st_mode & libc::S_ISVTX == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "output parent must have trusted ownership and no unprotected shared write access",
        ));
    }
    let mut stage = None;
    for attempt in 0..100 {
        let name = CString::new(format!(".cortados-stage-{}-{attempt}", std::process::id()))
            .expect("ASCII stage name");
        // SAFETY: anchored directory descriptor and valid C string.
        if unsafe { libc::mkdirat(anchor.as_raw_fd(), name.as_ptr(), 0o700) } == 0 {
            stage = Some(name);
            break;
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(std::io::Error::last_os_error());
        }
    }
    let stage = stage.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "cannot allocate output staging directory",
        )
    })?;
    let directory = match open_dir(anchor.as_raw_fd(), &stage) {
        Ok(directory) => directory,
        Err(error) => {
            // SAFETY: remove only the empty staging directory we just created.
            unsafe { libc::unlinkat(anchor.as_raw_fd(), stage.as_ptr(), libc::AT_REMOVEDIR) };
            return Err(error);
        }
    };
    let mut written = Vec::new();
    let mut committed = false;
    let result = (|| {
        let mut names: Vec<String> = artifacts
            .keys()
            .filter(|key| key.as_str() != "bundle.json")
            .cloned()
            .collect();
        if artifacts.contains_key("bundle.json") {
            names.push("bundle.json".into());
        }
        for name in names {
            if name.contains(['/', '\\']) || name == "." || name == ".." {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "invalid artifact filename",
                ));
            }
            let c_name = string(OsStr::new(&name))?;
            // SAFETY: flags create a new regular file under the pinned staging dir.
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    c_name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            written.push(c_name);
            // SAFETY: successful openat returned a new owned descriptor.
            let mut file = unsafe { File::from_raw_fd(fd) };
            file.write_all(&artifacts[&name])?;
            file.sync_all()?;
        }
        directory.sync_all()?;
        let mut pinned = std::mem::MaybeUninit::<libc::stat>::uninit();
        let mut named = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: descriptors remain live and both stat outputs are writable.
        if unsafe { libc::fstat(directory.as_raw_fd(), pinned.as_mut_ptr()) } != 0
            || unsafe {
                libc::fstatat(
                    anchor.as_raw_fd(),
                    stage.as_ptr(),
                    named.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let (pinned, named) = unsafe { (pinned.assume_init(), named.assume_init()) };
        if pinned.st_dev != named.st_dev || pinned.st_ino != named.st_ino {
            return Err(std::io::Error::other(
                "staging directory changed during publication",
            ));
        }
        // SAFETY: descriptors pin both parent directories; NOREPLACE is atomic.
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                anchor.as_raw_fd(),
                stage.as_ptr(),
                anchor.as_raw_fd(),
                leaf.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        committed = true;
        sync_parent(&anchor)?;
        Ok(())
    })();
    if result.is_err() && !committed {
        for name in written {
            // SAFETY: remove only names created in our pinned staging directory.
            unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
        }
        // SAFETY: AT_REMOVEDIR never removes a nonempty or symlink directory.
        unsafe { libc::unlinkat(anchor.as_raw_fd(), stage.as_ptr(), libc::AT_REMOVEDIR) };
    }
    result
}

#[cfg(not(target_os = "linux"))]
fn write_bundle(_: &Path, _: &BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "safe output requires Linux",
    ))
}

fn worker(options: &Options) -> Result<u8, Vec<Diagnostic>> {
    confine(&options.limits, options.wall_seconds)?;
    let bundle = analysis::analyse(&options.input, &options.limits)?;
    bundle.validate()?;
    let artifacts = bundle.artifacts();
    let manifest: serde_json::Value = serde_json::from_slice(&artifacts["bundle.json"])
        .map_err(|e| diagnostic("COR-E100", e.to_string()))?;
    write_bundle(&options.output, &artifacts).map_err(|e| {
        diagnostic(
            "COR-E500",
            format!("cannot publish new output directory: {e}"),
        )
    })?;
    println!(
        "{}",
        serde_json::json!({"profile_id":PROFILE, "completeness":manifest["completeness"], "artifacts":artifacts.len()})
    );
    Ok(if manifest["completeness"]["syntax_complete"] == true {
        0
    } else {
        4
    })
}

fn run(args: &[String]) -> Result<u8, Vec<Diagnostic>> {
    let Some(options) = parse(args)? else {
        return Ok(0);
    };
    if options.worker {
        return worker(&options);
    }
    #[cfg(target_os = "linux")]
    let executable = PathBuf::from("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let executable = std::env::current_exe().map_err(|e| diagnostic("COR-E500", e.to_string()))?;
    let mut child = Command::new(executable)
        .args(args)
        .arg("--worker")
        .env_clear()
        .env("CORTADOS_INTERNAL_WORKER", "1")
        .spawn()
        .map_err(|e| diagnostic("COR-E500", e.to_string()))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| diagnostic("COR-E500", e.to_string()))?
        {
            return status.code().map(|c| c as u8).ok_or_else(|| {
                diagnostic(
                    "COR-E500",
                    "analysis worker terminated by a resource limit or signal",
                )
            });
        }
        if started.elapsed() >= Duration::from_secs(options.wall_seconds) {
            let _ = child.kill();
            let reap_deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < reap_deadline {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            return Err(diagnostic("COR-E500", "analysis exceeded wall-time budget"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn main() -> ExitCode {
    let arguments = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| diagnostic("COR-E100", "arguments must be UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>();
    match arguments.and_then(|args| run(&args)) {
        Ok(code) => ExitCode::from(code),
        Err(diagnostics) => {
            eprintln!(
                "{}",
                serde_json::json!({"contract":"GY-CORTADOS-DIAGNOSTICS/0.1","diagnostics":diagnostics})
            );
            ExitCode::from(4)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn a_postcommit_sync_failure_keeps_the_published_bundle() {
        use std::os::unix::fs::DirBuilderExt;
        let root = std::env::temp_dir().join(format!(
            "cortados-postcommit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let output = root.join("analysis");
        let artifacts = BTreeMap::from([("bundle.json".into(), b"committed manifest".to_vec())]);
        let error = write_bundle_with_sync(&output, &artifacts, |_| {
            Err(std::io::Error::other("injected sync failure"))
        });
        assert!(error.is_err());
        assert_eq!(
            std::fs::read(output.join("bundle.json")).unwrap(),
            b"committed manifest"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
