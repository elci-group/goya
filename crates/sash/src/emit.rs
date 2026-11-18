//! Structured emission of one native Rust window.
//!
//! Authored text is emitted as decimal bytes. It is never interpolated into
//! a Rust string, a resource script, or a command line.

use crate::admit::AdmittedWindow;

pub struct Generated {
    pub main_rs: String,
    pub cargo_toml: &'static str,
    pub build_rs: &'static str,
    pub manifest_xml: String,
    pub resource_script: String,
    pub source_map: serde_json::Value,
}

pub fn generate(window: &AdmittedWindow) -> Generated {
    let window_ir = crate::ir_lower::lower_static_window(window);
    crate::ir_codegen::codegen_from_ir(&window_ir)
}

pub(crate) fn array_line(out: &mut String, name: &str, bytes: &[u8]) -> (usize, usize) {
    out.push_str("const ");
    out.push_str(name);
    out.push_str(": &[u8] = &");
    let start = out.len();
    out.push('[');
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&byte.to_string());
    }
    out.push(']');
    let end = out.len();
    out.push_str(";\n");
    (start, end)
}

pub(crate) fn origin(
    property: &str,
    generated: &(usize, usize),
    source: &goya_lang::SourceRange,
    semantic_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "property": property,
        "semantic_id": semantic_id,
        "generated_range": {"start": generated.0, "end": generated.1},
        "source_range": {"start": source.start, "end": source.end},
    })
}

pub(crate) const CARGO_TOML: &str = r#"[package]
name = "app"
version = "0.0.1"
edition = "2021"
publish = false
build = "build.rs"

[profile.release]
opt-level = 3
lto = false
codegen-units = 1
panic = "abort"
debug = true
strip = "none"
incremental = false
"#;

pub(crate) const BUILD_RS: &str = r#"fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let rc = std::path::Path::new(&manifest_dir).join("app.rc");
    let res = std::path::Path::new(&out_dir).join("app.res");
    let status = std::process::Command::new("llvm-rc")
        .arg("/fo")
        .arg(&res)
        .arg(&rc)
        .status()
        .expect("llvm-rc");
    if !status.success() {
        panic!("llvm-rc failed");
    }
    println!("cargo:rerun-if-changed={}", rc.display());
    println!("cargo:rerun-if-changed={manifest_dir}/app.manifest");
    println!("cargo:rustc-link-arg-bins={}", res.display());
}
"#;

pub(crate) const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="ELCI.Goya.Sash.StaticWindow" version="0.0.1.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;

pub(crate) const RESOURCE_SCRIPT: &str = r#"1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION 0,0,1,0
PRODUCTVERSION 0,0,1,0
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x00040004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Goya Sash static window"
      VALUE "ProductName", "Goya Sash"
      VALUE "FileVersion", "0.0.1"
      VALUE "ProductVersion", "0.0.1"
      VALUE "OriginalFilename", "App.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#;

/// Manifest for the static window. The win8 floor is system DPI aware and
/// names a `.Legacy` assembly, so the verifier reads the floor the image
/// was built for. Nothing here requests per-monitor awareness.
pub(crate) fn static_manifest(legacy: bool) -> String {
    if legacy {
        return r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="ELCI.Goya.Sash.StaticWindow.Legacy" version="0.0.1.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
    </windowsSettings>
  </application>
</assembly>
"#
        .to_string();
    }
    MANIFEST.to_string()
}

/// Resource script for the static window.
pub(crate) fn static_resource_script(legacy: bool) -> String {
    if !legacy {
        return RESOURCE_SCRIPT.to_string();
    }
    r#"1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION 0,0,1,0
PRODUCTVERSION 0,0,1,0
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x00040004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Goya Sash static window (legacy)"
      VALUE "ProductName", "Goya Sash"
      VALUE "FileVersion", "0.0.1"
      VALUE "ProductVersion", "0.0.1"
      VALUE "OriginalFilename", "App.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    .to_string()
}

/// Manifest for the interactive window. `profile_version` is `"0.0.1"` or
/// `"0.0.2"` following the authored clipboard policy; the win8 floor names a
/// `.Legacy` assembly and stays system DPI aware.
pub(crate) fn interactive_manifest(profile_version: &str, legacy: bool) -> String {
    let name = if legacy {
        "ELCI.Goya.Sash.InteractiveWindow.Legacy"
    } else {
        "ELCI.Goya.Sash.InteractiveWindow"
    };
    if legacy {
        return format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="{name}" version="{profile_version}.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
    </windowsSettings>
  </application>
</assembly>
"#
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="{name}" version="{profile_version}.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#
    )
}

/// Resource script for the interactive window.
pub(crate) fn interactive_resource_script(profile_version: &str, legacy: bool) -> String {
    let commas = profile_version.replace('.', ",");
    let description = if legacy {
        "Goya Sash interactive window (legacy)"
    } else {
        "Goya Sash interactive window"
    };
    format!(
        r#"1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION {commas},0
PRODUCTVERSION {commas},0
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x00040004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "{description}"
      VALUE "ProductName", "Goya Sash"
      VALUE "FileVersion", "{profile_version}"
      VALUE "ProductVersion", "{profile_version}"
      VALUE "OriginalFilename", "App.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}


#[cfg(test)]
mod tests {
    use crate::admit::admit;

    #[test]
    fn authored_bytes_are_not_rust_string_literals() {
        let source = "@ontology hello {\n  @cell root {\n    id: \"window.root\";\n    text: \"hello\\oops\";\n    extent: (80, 40, 0);\n  }\n}\n";
        let window = admit(source).unwrap();
        assert_eq!(window.text, "hello\\oops");
        let generated = super::generate(&window);
        assert!(!generated.main_rs.contains("hello\\oops"));
        assert!(generated
            .main_rs
            .contains("104,101,108,108,111,92,111,111,112,115"));
        assert!(generated.main_rs.contains("const CLIENT_DIP_W: i32 = 320;"));
        assert!(!generated.manifest_xml.contains("hello"));
        assert!(!generated.resource_script.contains("hello"));
        let text = generated.source_map["files"][0]["origins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["property"] == "text")
            .unwrap();
        let start = text["generated_range"]["start"].as_u64().unwrap() as usize;
        let end = text["generated_range"]["end"].as_u64().unwrap() as usize;
        let bytes: Vec<u8> = generated.main_rs[start + 1..end - 1]
            .split(',')
            .map(|part| part.parse().unwrap())
            .collect();
        assert_eq!(bytes, window.text.as_bytes());
    }

    #[test]
    fn nested_fixture_emits_one_static_per_text_cell() {
        let window = admit(include_str!("../fixtures/nested-static.gscene")).unwrap();
        let generated = super::generate(&window);
        assert!(generated.main_rs.contains("x: 0, y: 0, w: 320, h: 160"));
        assert!(generated.main_rs.contains("x: 40, y: 20, w: 80, h: 40"));
        assert!(generated.main_rs.contains("const TEXT_0:"));
        assert!(generated.main_rs.contains("const TEXT_1:"));
        assert!(generated.main_rs.contains("84,104,101,114,101"));
        assert!(!generated.main_rs.contains("\"There\""));
        assert!(!generated.main_rs.contains("\"Hello\""));
    }

    #[test]
    fn labels_use_the_dpi_message_font() {
        let window = admit(include_str!("../fixtures/static-window.gscene")).unwrap();
        let generated = super::generate(&window);
        assert!(generated.main_rs.contains("SystemParametersInfoForDpi"));
        assert!(generated.main_rs.contains("CreateFontIndirectW"));
        assert!(generated
            .main_rs
            .contains("std::mem::size_of::<NonClientMetricsW>() == 504"));
        assert!(generated
            .main_rs
            .contains("std::mem::size_of::<LogFontW>() == 92"));
        assert!(!generated.main_rs.contains("GetStockObject"));
        assert!(!generated.main_rs.contains("DEFAULT_GUI_FONT"));
        assert!(window
            .dispositions
            .iter()
            .any(|item| item.subject == "background" && item.detail.contains("message font")));
    }
}
