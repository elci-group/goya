//! Deterministic `.ipk` packaging (Spinneret), with no Node.js or ares-cli.
//!
//! The layout matches what `ares-package` (`@webos-tools/cli`,
//! `lib/package.js`) writes for a packaged web app:
//!
//! ```text
//! <id>_<version>_all.ipk          System V ar
//! ├── debian-binary               "2.0\n"
//! ├── control.tar.gz              ./control
//! └── data.tar.gz
//!     └── usr/palm/applications/<id>/…           the app directory
//!     └── usr/palm/packages/<id>/packageinfo.json
//! ```
//!
//! The control file has the same fields, in the same order, as ares.
//!
//! Deliberate differences from ares, all for reproducibility:
//! * The ar member mtime is 0. Ares writes the build time.
//! * Tar entries have mtime 0 and are owned by `root`.
//! * gzip uses stored DEFLATE blocks.
//! * `Installed-Size` is the sum of file sizes. Ares sums `lstat` sizes,
//!   including directory blocks, which depend on the host.
//!
//! None of these differences has been checked against the TV's installer
//! yet. That check needs the owner's TV in Developer Mode.

use crate::appinfo::{json_string, AppInfo};
use crate::archive::{ar, gzip, tar, ArchiveError, TarEntry};
use std::collections::BTreeMap;

pub struct Ipk {
    pub file_name: String,
    pub bytes: Vec<u8>,
    /// Exact `control` text, for reports.
    pub control: String,
}

/// `packageinfo.json` exactly as ares writes it:
/// `JSON.stringify({id, version, app}, null, 2) + "\n"`.
pub fn packageinfo(app: &AppInfo) -> String {
    format!(
        "{{\n  \"id\": {},\n  \"version\": {},\n  \"app\": {}\n}}\n",
        json_string(&app.id),
        json_string(&app.version),
        json_string(&app.id)
    )
}

pub fn control(app: &AppInfo, installed_size: u64) -> String {
    [
        format!("Package: {}", app.id),
        format!("Version: {}", app.version),
        "Section: misc".into(),
        "Priority: optional".into(),
        "Architecture: all".into(),
        format!("Installed-Size: {installed_size}"),
        "Maintainer: N/A ".into(),
        "Description: This is a webOS application.".into(),
        "webOS-Package-Format-Version: 2".into(),
        "webOS-Packager-Version: x.y.x".into(),
    ]
    .join("\n")
        + "\n"
}

/// Package `files` (paths relative to the app directory, `/`-separated,
/// including `appinfo.json`) into an IPK.
pub fn package(app: &AppInfo, files: &[(String, Vec<u8>)]) -> Result<Ipk, ArchiveError> {
    package_with_executables(app, files, &[])
}

/// [`package`], with the files named in `executables` written with mode
/// 0755. A native app (`"type": "native"`, used by Viuda) needs its `main`
/// to be executable.
pub fn package_with_executables(
    app: &AppInfo,
    files: &[(String, Vec<u8>)],
    executables: &[&str],
) -> Result<Ipk, ArchiveError> {
    let app_root = format!("usr/palm/applications/{}/", app.id);
    let pkg_root = format!("usr/palm/packages/{}/", app.id);
    let mut tree: BTreeMap<String, Option<(Vec<u8>, bool)>> = BTreeMap::new();
    let add_dirs = |tree: &mut BTreeMap<String, Option<(Vec<u8>, bool)>>, path: &str| {
        let mut end = 0;
        while let Some(slash) = path[end..].find('/') {
            end += slash + 1;
            tree.entry(path[..end].to_string()).or_insert(None);
        }
    };
    for (rel, bytes) in files {
        let path = format!("{app_root}{rel}");
        add_dirs(&mut tree, &path);
        tree.insert(path, Some((bytes.clone(), executables.contains(&rel.as_str()))));
    }
    let info_path = format!("{pkg_root}packageinfo.json");
    add_dirs(&mut tree, &info_path);
    tree.insert(info_path, Some((packageinfo(app).into_bytes(), false)));

    let installed_size: u64 = tree.values().flatten().map(|(b, _)| b.len() as u64).sum();
    let entries: Vec<TarEntry> = tree
        .into_iter()
        .map(|(path, data)| match data {
            None => TarEntry::Dir(path),
            Some((bytes, false)) => TarEntry::File(path, bytes),
            Some((bytes, true)) => TarEntry::Exec(path, bytes),
        })
        .collect();
    let data = gzip(&tar(&entries)?);
    let control_text = control(app, installed_size);
    let control_tgz = gzip(&tar(&[TarEntry::File(
        "control".into(),
        control_text.clone().into_bytes(),
    )])?);
    let bytes = ar(&[
        ("debian-binary", b"2.0\n"),
        ("control.tar.gz", &control_tgz),
        ("data.tar.gz", &data),
    ]);
    Ok(Ipk {
        file_name: format!("{}_{}_all.ipk", app.id, app.version),
        bytes,
        control: control_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::read;

    fn app() -> AppInfo {
        AppInfo {
            id: "com.goya.arachnid.demo".into(),
            title: "Demo".into(),
            version: "1.0.0".into(),
            vendor: None,
        }
    }

    fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            ("appinfo.json".into(), app().to_json().into_bytes()),
            ("index.html".into(), b"<!doctype html>".to_vec()),
            ("assets/x.txt".into(), b"x".to_vec()),
        ]
    }

    #[test]
    fn layout_matches_ares() {
        let ipk = package(&app(), &files()).unwrap();
        assert_eq!(ipk.file_name, "com.goya.arachnid.demo_1.0.0_all.ipk");
        let members = read::ar(&ipk.bytes).unwrap();
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["debian-binary", "control.tar.gz", "data.tar.gz"]);
        assert_eq!(members[0].1, b"2.0\n");

        let control = read::tar(&read::gunzip_stored(&members[1].1).unwrap()).unwrap();
        assert_eq!(control.len(), 1);
        assert_eq!(control[0].0, "control");
        let text = String::from_utf8(control[0].3.clone()).unwrap();
        assert!(
            text.starts_with("Package: com.goya.arachnid.demo\nVersion: 1.0.0\nSection: misc\n")
        );

        let data = read::tar(&read::gunzip_stored(&members[2].1).unwrap()).unwrap();
        let paths: Vec<&str> = data.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            paths,
            [
                "usr/",
                "usr/palm/",
                "usr/palm/applications/",
                "usr/palm/applications/com.goya.arachnid.demo/",
                "usr/palm/applications/com.goya.arachnid.demo/appinfo.json",
                "usr/palm/applications/com.goya.arachnid.demo/assets/",
                "usr/palm/applications/com.goya.arachnid.demo/assets/x.txt",
                "usr/palm/applications/com.goya.arachnid.demo/index.html",
                "usr/palm/packages/",
                "usr/palm/packages/com.goya.arachnid.demo/",
                "usr/palm/packages/com.goya.arachnid.demo/packageinfo.json",
            ]
        );
        let info = data.last().unwrap();
        assert_eq!(
            String::from_utf8(info.3.clone()).unwrap(),
            "{\n  \"id\": \"com.goya.arachnid.demo\",\n  \"version\": \"1.0.0\",\n  \"app\": \"com.goya.arachnid.demo\"\n}\n"
        );
        let size: usize = data.iter().map(|e| e.3.len()).sum();
        assert!(text.contains(&format!("Installed-Size: {size}\n")));
    }

    #[test]
    fn packaging_is_byte_reproducible() {
        let a = package(&app(), &files()).unwrap().bytes;
        let mut shuffled = files();
        shuffled.reverse();
        let b = package(&app(), &shuffled).unwrap().bytes;
        assert_eq!(a, b);
    }
}
