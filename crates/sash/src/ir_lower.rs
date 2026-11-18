//! Lowering from Admitted types to IR

use crate::admit::AdmittedWindow;
use crate::interactive::{AdmittedInteractive, ControlRole as AdmittedControlRole};
use crate::{emit, hex_encode};
use goya_lang::SourceRange as AdmittedRange;
use sash_ir::*;

/// Convert an admitted [`goya_lang::SourceRange`] into the IR-equivalent
/// [`sash_ir::SourceRange`]. Both are half-open UTF-8 byte offsets with the
/// same field layout, so this is a direct field copy.
fn to_ir_range(range: AdmittedRange) -> SourceRange {
    SourceRange {
        start: range.start,
        end: range.end,
    }
}

/// Lower an AdmittedWindow to IR
pub fn lower_static_window(admitted: &AdmittedWindow) -> IrDocument {
    let controls: Vec<Control> = admitted
        .labels
        .iter()
        .enumerate()
        .map(|(index, label)| Control {
            index,
            role: ControlRole::Static,
            // The static-window program places controls positionally: the
            // menu parameter of CreateWindowExW is the 1-based tree index.
            control_id: (index + 1) as u16,
            name: format!("label_{index}"),
            semantic_id: label.semantic_id.clone(),
            text: label.text.clone(),
            x_dip: label.x_dip,
            y_dip: label.y_dip,
            width_dip: label.width_dip,
            height_dip: label.height_dip,
            text_range: to_ir_range(label.text_range),
            id_range: to_ir_range(label.id_range),
            extent_range: to_ir_range(label.extent_range),
        })
        .collect();

    let control_origins: Vec<ControlOrigin> = admitted
        .labels
        .iter()
        .enumerate()
        .map(|(index, label)| ControlOrigin {
            index,
            text_range: to_ir_range(label.text_range),
            id_range: to_ir_range(label.id_range),
            extent_range: to_ir_range(label.extent_range),
        })
        .collect();

    let window = Window {
        scene_name: admitted.scene_name.clone(),
        semantic_id: admitted.semantic_id.clone(),
        width_dip: admitted.width_dip,
        height_dip: admitted.height_dip,
        width_mp: admitted.width_mp,
        height_mp: admitted.height_mp,
    };

    let identity = Identity {
        root_id_utf8: admitted.semantic_id.as_bytes().to_vec(),
        root_id_len: admitted.semantic_id.len(),
        title_utf8: admitted.scene_name.as_bytes().to_vec(),
        title_len: admitted.scene_name.len(),
    };

    let resources = Resources {
        cargo_toml: emit::CARGO_TOML.to_string(),
        build_rs: emit::BUILD_RS.to_string(),
        manifest_xml: emit::MANIFEST.to_string(),
        resource_script: emit::RESOURCE_SCRIPT.to_string(),
    };

    let provenance = Provenance {
        source_digest: hex_encode(&admitted.source_digest),
        ontology_range: to_ir_range(admitted.ontology_range),
        id_range: to_ir_range(admitted.id_range),
        extent_range: to_ir_range(admitted.extent_range),
        control_origins,
    };

    IrDocument {
        version: IR_VERSION.to_string(),
        profile: crate::admit::PROFILE.to_string(),
        window,
        controls,
        transition: None,
        gesture_policy: None,
        identity,
        resources,
        provenance,
    }
}

/// Lower an AdmittedInteractive to IR
pub fn lower_interactive_window(admitted: &AdmittedInteractive) -> IrDocument {
    let controls: Vec<Control> = admitted
        .controls
        .iter()
        .enumerate()
        .map(|(index, control)| {
            let role = match control.role {
                AdmittedControlRole::Static => ControlRole::Static,
                AdmittedControlRole::Edit => ControlRole::Edit,
                AdmittedControlRole::Button => ControlRole::Button,
            };
            Control {
                index,
                role,
                control_id: control.control_id,
                name: control.name.clone(),
                semantic_id: control.semantic_id.clone(),
                text: control.text.clone(),
                x_dip: control.x_dip,
                y_dip: control.y_dip,
                width_dip: control.width_dip,
                height_dip: control.height_dip,
                text_range: to_ir_range(control.text_range),
                id_range: to_ir_range(control.id_range),
                extent_range: to_ir_range(control.extent_range),
            }
        })
        .collect();

    let control_origins: Vec<ControlOrigin> = admitted
        .controls
        .iter()
        .enumerate()
        .map(|(index, control)| ControlOrigin {
            index,
            text_range: to_ir_range(control.text_range),
            id_range: to_ir_range(control.id_range),
            extent_range: to_ir_range(control.extent_range),
        })
        .collect();

    let transition = Some(Transition {
        action_id: admitted.action_id.clone(),
        action_name: admitted.action_name.clone(),
        action_range: to_ir_range(admitted.action_range),
        state_key: admitted.state_key.clone(),
        input_key: admitted.input_key.clone(),
        initial_state: admitted.initial_state,
        initial_input: admitted.initial_input,
        state_index: admitted.state_index,
        input_index: admitted.input_index,
        button_index: admitted.button_index,
        button_id: admitted.controls[admitted.button_index].control_id,
        expr: lower_int_expr(&admitted.expr),
    });

    let gesture_policy = Some(GesturePolicy {
        copy_allowed: admitted.copy_allowed,
        paste_allowed: admitted.paste_allowed,
        cut_mode: match admitted.cut_mode {
            crate::interactive::CUT_ALLOW => CutMode::Allow,
            crate::interactive::CUT_DENY => CutMode::Deny,
            _ => CutMode::Default,
        },
    });

    let window = Window {
        scene_name: admitted.scene_name.clone(),
        semantic_id: admitted.semantic_id.clone(),
        width_dip: admitted.width_dip,
        height_dip: admitted.height_dip,
        width_mp: 0, // Not tracked in interactive
        height_mp: 0,
    };

    let identity = Identity {
        root_id_utf8: admitted.semantic_id.as_bytes().to_vec(),
        root_id_len: admitted.semantic_id.len(),
        title_utf8: admitted.scene_name.as_bytes().to_vec(),
        title_len: admitted.scene_name.len(),
    };

    let profile_version = if admitted.profile == crate::interactive::PROFILE_CLIPBOARD {
        "0.0.2"
    } else {
        "0.0.1"
    };

    let resources = Resources {
        cargo_toml: emit::CARGO_TOML.to_string(),
        build_rs: emit::BUILD_RS.to_string(),
        manifest_xml: format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="ELCI.Goya.Sash.InteractiveWindow" version="{profile_version}.0" processorArchitecture="amd64"/>
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
        ),
        resource_script: format!(
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
      VALUE "FileDescription", "Goya Sash interactive window"
      VALUE "ProductName", "Goya Sash"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "App.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
            commas = profile_version.replace('.', ","),
            version = profile_version
        ),
    };

    let provenance = Provenance {
        source_digest: hex_encode(&admitted.source_digest),
        ontology_range: to_ir_range(admitted.ontology_range),
        id_range: to_ir_range(admitted.id_range),
        extent_range: to_ir_range(admitted.extent_range),
        control_origins,
    };

    IrDocument {
        version: IR_VERSION.to_string(),
        profile: admitted.profile.to_string(),
        window,
        controls,
        transition,
        gesture_policy,
        identity,
        resources,
        provenance,
    }
}

pub(crate) fn lower_int_expr(expr: &crate::interactive::IntExpr) -> IntExpr {
    match expr {
        crate::interactive::IntExpr::State => IntExpr::State,
        crate::interactive::IntExpr::Input => IntExpr::Input,
        crate::interactive::IntExpr::Literal(v) => IntExpr::Literal(*v),
        crate::interactive::IntExpr::Add(left, right) => IntExpr::Add(
            Box::new(lower_int_expr(left)),
            Box::new(lower_int_expr(right)),
        ),
    }
}
#[cfg(test)]
mod source_range_parity_tests {
    // `sash_ir::SourceRange` is a hand-maintained structural duplicate of
    // `goya_lang::SourceRange` (sash-ir does not depend on goya-lang).
    // This test fails if the two types drift apart.
    use super::to_ir_range;
    use goya_lang::SourceRange as LangRange;
    use sash_ir::SourceRange as IrRange;

    const _: () = {
        assert!(std::mem::size_of::<LangRange>() == std::mem::size_of::<IrRange>());
        assert!(std::mem::align_of::<LangRange>() == std::mem::align_of::<IrRange>());
    };

    #[test]
    fn source_range_layout_parity() {
        for (start, end) in [
            (0usize, 0usize),
            (0, 1),
            (7, 42),
            (usize::MAX / 2, usize::MAX / 2 + 1),
            (usize::MAX - 1, usize::MAX),
        ] {
            let lang = LangRange { start, end };
            let ir = to_ir_range(lang);
            assert_eq!((ir.start, ir.end), (lang.start, lang.end));
            // Field-for-field lossless round trip implies identical layout.
            let back = LangRange { start: ir.start, end: ir.end };
            assert_eq!(back, lang);
        }
    }
}
