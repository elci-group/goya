//! Lower one Sash-admitted interactive counter to GY-EMP-NIR/1.

use crate::{sha256_hex, Fault, PROFILE};
use emperor_runtime::{Capabilities, Element, NativeIr};
use sash::interactive::{ControlRole, CUT_ALLOW, CUT_DEFAULT, CUT_DENY, PROFILE_CLIPBOARD};

#[derive(Debug)]
pub struct Lowered {
    pub ir: NativeIr,
    pub json: String,
    pub sash_profile: &'static str,
    pub handoff_contract: &'static str,
}

pub fn lower(source: &str) -> Result<Lowered, Fault> {
    let handoff = goya_lang::handoff::admit(source);
    let opened = sash::admit::open(source);
    let (contract, admitted) = match (handoff, opened) {
        (Ok(handoff), Ok(sash::admit::SceneTarget::Interactive(admitted))) => {
            (handoff.contract, admitted)
        }
        (Ok(_), Ok(sash::admit::SceneTarget::Static(_))) => {
            return Err(Fault::new(
                1,
                "SASH-CAP",
                "this profile admits only the interactive counter",
            ));
        }
        (Err(_), Err(diagnostic)) => {
            return Err(Fault::new(1, diagnostic.code, diagnostic.message));
        }
        (Ok(_), Err(diagnostic)) => {
            return Err(Fault::new(1, diagnostic.code, diagnostic.message));
        }
        (Err(_), Ok(_)) => {
            return Err(Fault::new(
                1,
                "EMP-ADMIT",
                "shared frontend and Sash admission disagreed",
            ));
        }
    };
    if admitted.profile != PROFILE_CLIPBOARD {
        return Err(Fault::new(
            1,
            "SASH-CAP",
            format!(
                "this profile admits {PROFILE_CLIPBOARD} and found {}",
                admitted.profile
            ),
        ));
    }
    let digest = sash::hex_encode(&admitted.source_digest);
    if digest != sha256_hex(source.as_bytes()) {
        return Err(Fault::new(
            1,
            "EMP-NIR",
            "Sash source digest does not match the scene bytes",
        ));
    }
    let cut = match admitted.cut_mode {
        CUT_ALLOW => "allow",
        CUT_DEFAULT => "default",
        CUT_DENY => "deny",
        _ => {
            return Err(Fault::new(
                1,
                "EMP-NIR",
                format!(
                    "cut mode {} is not allow, default, or deny",
                    admitted.cut_mode
                ),
            ));
        }
    };
    let mut elements = Vec::with_capacity(admitted.controls.len());
    for (index, control) in admitted.controls.iter().enumerate() {
        if control.id_range.start > control.id_range.end || control.id_range.end > source.len() {
            return Err(Fault::new(
                1,
                "EMP-NIR",
                format!("id span for {} is outside the scene", control.semantic_id),
            ));
        }
        let role = role_of(
            control.role,
            index,
            &control.semantic_id,
            &admitted.state_key,
        );
        elements.push(Element {
            id: control.semantic_id.clone(),
            role: role.to_owned(),
            text: control.text.clone(),
            x_dip: control.x_dip,
            y_dip: control.y_dip,
            width_dip: control.width_dip,
            height_dip: control.height_dip,
            span_start: control.id_range.start,
            span_end: control.id_range.end,
        });
    }
    let roles = |name: &str| {
        elements
            .iter()
            .filter(|element| element.role == name)
            .count()
    };
    if roles("client") != 1 || roles("edit") != 1 || roles("button") != 1 || roles("result") != 1 {
        return Err(Fault::new(
            1,
            "EMP-NIR",
            "lowering did not produce one client, edit, button, and result",
        ));
    }
    if elements.iter().any(|element| element.role == "static") {
        return Err(Fault::new(
            1,
            "EMP-NIR",
            "this profile has no remaining static control",
        ));
    }
    let ir = NativeIr {
        schema: "GY-EMP-NIR/1".to_owned(),
        profile: PROFILE.to_owned(),
        source_digest_sha256: digest,
        scene: source.to_owned(),
        action_id: admitted.action_id,
        state_id: admitted.state_key,
        input_id: admitted.input_key,
        initial_state: admitted.initial_state,
        initial_input: admitted.initial_input,
        width_dip: admitted.width_dip,
        height_dip: admitted.height_dip,
        elements,
        capabilities: Capabilities {
            copy: admitted.copy_allowed,
            paste: admitted.paste_allowed,
            cut: cut.to_owned(),
            filesystem: "none".to_owned(),
            network: false,
            subprocess: false,
            web_content: false,
        },
    };
    let json = serde_json::to_string(&ir)
        .map_err(|err| Fault::new(1, "EMP-NIR", format!("native ir did not serialise: {err}")))?;
    Ok(Lowered {
        ir,
        json,
        sash_profile: admitted.profile,
        handoff_contract: contract,
    })
}

fn role_of(role: ControlRole, index: usize, semantic_id: &str, state_key: &str) -> &'static str {
    match role {
        ControlRole::Edit => "edit",
        ControlRole::Button => "button",
        ControlRole::Static if index == 0 && semantic_id.ends_with(".root") => "client",
        ControlRole::Static if semantic_id == format!("state.{state_key}") => "result",
        ControlRole::Static => "static",
    }
}

pub fn embed_rust(ir: &NativeIr, json: &str) -> String {
    let mut elements = String::from("pub static ELEMENTS: [emperor_runtime::CompiledElement; ");
    elements.push_str(&ir.elements.len().to_string());
    elements.push_str("] = [");
    for element in &ir.elements {
        elements.push_str("\nemperor_runtime::CompiledElement { id: ");
        elements.push_str(&rust_string(&element.id));
        elements.push_str(", role: ");
        elements.push_str(&rust_string(&element.role));
        elements.push_str(", text: ");
        elements.push_str(&rust_string(&element.text));
        elements.push_str(", x_dip: ");
        elements.push_str(&element.x_dip.to_string());
        elements.push_str(", y_dip: ");
        elements.push_str(&element.y_dip.to_string());
        elements.push_str(", width_dip: ");
        elements.push_str(&element.width_dip.to_string());
        elements.push_str(", height_dip: ");
        elements.push_str(&element.height_dip.to_string());
        elements.push_str(", span_start: ");
        elements.push_str(&element.span_start.to_string());
        elements.push_str(", span_end: ");
        elements.push_str(&element.span_end.to_string());
        elements.push_str(" },");
    }
    elements.push_str("\n];\n");
    let mut file = String::from("// Generated by Emperor ");
    file.push_str(crate::COMPILER_VERSION);
    file.push_str(". Do not edit.\n");
    file.push_str("pub const NIR_JSON: &str = ");
    file.push_str(&rust_string(json));
    file.push_str(";\npub const SCENE_SRC: &str = ");
    file.push_str(&rust_string(&ir.scene));
    file.push_str(";\n");
    file.push_str(&elements);
    file.push_str(
        "pub static SCENE: emperor_runtime::CompiledScene = emperor_runtime::CompiledScene {\n",
    );
    file.push_str("    nir_json: NIR_JSON,\n    schema: ");
    file.push_str(&rust_string(&ir.schema));
    file.push_str(",\n    profile: ");
    file.push_str(&rust_string(&ir.profile));
    file.push_str(",\n    source_digest_sha256: ");
    file.push_str(&rust_string(&ir.source_digest_sha256));
    file.push_str(",\n    scene: SCENE_SRC,\n    action_id: ");
    file.push_str(&rust_string(&ir.action_id));
    file.push_str(",\n    state_id: ");
    file.push_str(&rust_string(&ir.state_id));
    file.push_str(",\n    input_id: ");
    file.push_str(&rust_string(&ir.input_id));
    file.push_str(",\n    initial_state: ");
    file.push_str(&ir.initial_state.to_string());
    file.push_str(",\n    initial_input: ");
    file.push_str(&ir.initial_input.to_string());
    file.push_str(",\n    width_dip: ");
    file.push_str(&ir.width_dip.to_string());
    file.push_str(",\n    height_dip: ");
    file.push_str(&ir.height_dip.to_string());
    file.push_str(",\n    copy: ");
    file.push_str(if ir.capabilities.copy {
        "true"
    } else {
        "false"
    });
    file.push_str(",\n    paste: ");
    file.push_str(if ir.capabilities.paste {
        "true"
    } else {
        "false"
    });
    file.push_str(",\n    cut: ");
    file.push_str(&rust_string(&ir.capabilities.cut));
    file.push_str(",\n    filesystem: ");
    file.push_str(&rust_string(&ir.capabilities.filesystem));
    file.push_str(",\n    network: false,\n    subprocess: false,\n    web_content: false,\n    elements: &ELEMENTS,\n};\n");
    file.push_str(
        "pub fn embedded() -> Option<&'static emperor_runtime::CompiledScene> { Some(&SCENE) }\n",
    );
    file
}

fn rust_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if other.is_control() => {
                out.push_str(&format!("\\u{{{:x}}}", u32::from(other)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNTER: &str = include_str!("../../sash/fixtures/interactive-counter.gscene");
    const STATIC: &str = include_str!("../../sash/fixtures/static-window.gscene");

    #[test]
    fn counter_lowers_to_the_admitted_rectangles() {
        let lowered = lower(COUNTER).expect("lower");
        assert_eq!(lowered.ir.profile, PROFILE);
        assert_eq!(lowered.ir.state_id, "count");
        assert_eq!(lowered.ir.input_id, "amount");
        assert_eq!(lowered.ir.action_id, "action.add");
        assert_eq!((lowered.ir.width_dip, lowered.ir.height_dip), (320, 160));
        assert!(lowered.ir.capabilities.copy);
        assert!(!lowered.ir.capabilities.paste);
        assert_eq!(lowered.ir.capabilities.cut, "default");
        assert_eq!(lowered.ir.capabilities.filesystem, "none");
        let amount = &lowered.ir.elements[1];
        assert_eq!(amount.id, "window.amount");
        assert_eq!(
            (
                amount.x_dip,
                amount.y_dip,
                amount.width_dip,
                amount.height_dip
            ),
            (8, 64, 120, 32)
        );
        let add = &lowered.ir.elements[2];
        assert_eq!(add.id, "window.add");
        assert_eq!(
            (add.x_dip, add.y_dip, add.width_dip, add.height_dip),
            (136, 64, 80, 32)
        );
        let total = &lowered.ir.elements[3];
        assert_eq!(total.id, "state.count");
        assert_eq!(total.role, "result");
        assert_eq!(
            (total.x_dip, total.y_dip, total.width_dip, total.height_dip),
            (8, 112, 160, 32)
        );
        let parsed: NativeIr = serde_json::from_str(&lowered.json).expect("json");
        assert_eq!(parsed, lowered.ir);
        let report = emperor_runtime::oracle(&lowered.json).expect("oracle");
        assert_eq!((report.count, report.revision), (1, 1));
        assert!(!report.paste_allowed);
        let interaction =
            emperor_runtime::exercise_interaction(&lowered.json).expect("interaction");
        assert_eq!(
            interaction,
            emperor_runtime::InteractionReport {
                count: 3,
                revision: 2,
                edit: "2".to_owned(),
                focus: "button".to_owned(),
                paste_ignored: true,
                copy_allowed: true,
                paste_allowed: false,
                add_hit: true,
                rects_stable: true,
                space_activates: true,
            }
        );
    }

    #[test]
    fn static_window_is_rejected() {
        let err = lower(STATIC).expect_err("static");
        assert_eq!(err.code, "SASH-CAP");
        assert_eq!(err.exit, 1);
    }
}
