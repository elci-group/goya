//! Capability-mediated surface roles.
//!
//! A controller may submit intents. It does not gain permission to commit
//! authoritative state. Assignment fails when a declared requirement is
//! absent. A preference, such as touch or a large display, does not.
//! The capability record is four booleans. It carries no resolution, sensor,
//! or operating-system name.

use crate::canonical::{decode, encode, Value};
use crate::channel::{AppKind, Channel};
use crate::Error;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    Primary,
    Controller,
    Auxiliary,
}

/// Minimised device facts. These are not a fingerprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceCaps {
    pub display: bool,
    pub interactive_input: bool,
    /// Advertised only. Assignment does not require touch.
    pub touch: bool,
    /// Advertised only. Assignment does not require a large display.
    pub large_display: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDecl {
    pub role: SurfaceRole,
    pub requires_display: bool,
    pub requires_input: bool,
    pub prefers_touch: bool,
    pub prefers_large_display: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub submit_intent: bool,
    pub mutate: bool,
    pub transfer: bool,
    pub administer: bool,
}

impl Permissions {
    pub const fn authority() -> Self {
        Self {
            read: true,
            submit_intent: true,
            mutate: true,
            transfer: true,
            administer: false,
        }
    }

    pub const fn follower() -> Self {
        Self {
            read: true,
            submit_intent: true,
            mutate: false,
            transfer: false,
            administer: false,
        }
    }

    pub const fn reader() -> Self {
        Self {
            read: true,
            submit_intent: false,
            mutate: false,
            transfer: false,
            administer: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub permissions: Permissions,
    pub touch_available: bool,
    pub large_display_available: bool,
    pub preferences_met: bool,
}

pub fn assign_surface(decl: &SurfaceDecl, caps: &DeviceCaps) -> Result<Assignment, Error> {
    if decl.requires_display && !caps.display {
        return Err(Error::new("RMT-E7001", "device lacks a required display"));
    }
    if decl.requires_input && !caps.interactive_input {
        return Err(Error::new(
            "RMT-E7001",
            "device lacks required interactive input",
        ));
    }
    let permissions = match decl.role {
        SurfaceRole::Primary => Permissions::authority(),
        SurfaceRole::Controller => Permissions::follower(),
        SurfaceRole::Auxiliary => Permissions::reader(),
    };
    let touch_ok = !decl.prefers_touch || caps.touch;
    let large_ok = !decl.prefers_large_display || caps.large_display;
    Ok(Assignment {
        permissions,
        touch_available: caps.touch,
        large_display_available: caps.large_display,
        preferences_met: touch_ok && large_ok,
    })
}

/// Bind `decl` to `replica` when the device can fill the role.
///
/// The active writer stays the writer. A replica that is not the authority
/// cannot take the primary role.
pub fn adopt_surface(
    replica: &mut crate::Replica,
    decl: &SurfaceDecl,
    caps: &DeviceCaps,
) -> Result<Assignment, Error> {
    let assignment = assign_surface(decl, caps)?;
    replica.adopt_permissions(assignment.permissions)?;
    Ok(assignment)
}

/// Canonical capability record. Exactly four booleans.
pub fn encode_capabilities(caps: &DeviceCaps) -> Result<Vec<u8>, Error> {
    let mut record = BTreeMap::new();
    record.insert(1, Value::Bool(caps.display));
    record.insert(2, Value::Bool(caps.interactive_input));
    record.insert(3, Value::Bool(caps.touch));
    record.insert(4, Value::Bool(caps.large_display));
    encode(&Value::Record(record))
}

pub fn decode_capabilities(bytes: &[u8]) -> Result<DeviceCaps, Error> {
    let value = decode(bytes)?;
    if encode(&value)? != bytes {
        return Err(Error::new("RMT-E2001", "capabilities are not canonical"));
    }
    let Value::Record(fields) = value else {
        return Err(Error::new("RMT-E2001", "capabilities are not canonical"));
    };
    if fields.len() != 4 {
        return Err(Error::new("RMT-E2001", "capabilities are not canonical"));
    }
    Ok(DeviceCaps {
        display: bool_field(&fields, 1)?,
        interactive_input: bool_field(&fields, 2)?,
        touch: bool_field(&fields, 3)?,
        large_display: bool_field(&fields, 4)?,
    })
}

/// Advertise `caps` for `decl` on the existing channel.
///
/// The record is checked before it is sealed. A missing requirement sends
/// nothing. Kind 9 is the capability frame.
pub fn deliver_capabilities(
    sender: &mut Channel,
    receiver: &mut Channel,
    decl: &SurfaceDecl,
    caps: &DeviceCaps,
) -> Result<Assignment, Error> {
    let assignment = assign_surface(decl, caps)?;
    let plaintext = encode_capabilities(caps)?;
    let frame = sender.seal_app(AppKind::Capabilities, &plaintext)?;
    let (kind, opened) = receiver.open_app(&frame)?;
    if kind != AppKind::Capabilities || opened.as_slice() != plaintext.as_slice() {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let advertised = decode_capabilities(&opened)?;
    let accepted = assign_surface(decl, &advertised)?;
    if accepted != assignment {
        return Err(Error::new(
            "RMT-E7001",
            "device lacks required interactive input",
        ));
    }
    Ok(accepted)
}

fn bool_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<bool, Error> {
    match fields.get(&id) {
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(Error::new("RMT-E2001", "capabilities are not canonical")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{decode, Value};
    use crate::channel::Channel;
    use crate::id_from_byte;
    use crate::pairs::{
        completion_permille, enqueue_select, execute_intent, join, open, state_ok, F_SCORE,
        F_VIEWPORT, SCHEMA,
    };
    use crate::session::Intent;
    use crate::sync::deliver_intent;
    use crate::Replica;

    fn deck() -> [u32; 8] {
        [0, 1, 2, 3, 3, 2, 1, 0]
    }

    fn tv() -> [u8; 16] {
        id_from_byte(1)
    }

    fn phone() -> [u8; 16] {
        id_from_byte(2)
    }

    fn board_decl() -> SurfaceDecl {
        SurfaceDecl {
            role: SurfaceRole::Primary,
            requires_display: true,
            requires_input: true,
            prefers_touch: false,
            prefers_large_display: true,
        }
    }

    fn controller_decl() -> SurfaceDecl {
        SurfaceDecl {
            role: SurfaceRole::Controller,
            requires_display: false,
            requires_input: true,
            prefers_touch: true,
            prefers_large_display: false,
        }
    }

    fn board_caps() -> DeviceCaps {
        DeviceCaps {
            display: true,
            interactive_input: true,
            touch: false,
            large_display: true,
        }
    }

    fn controller_caps() -> DeviceCaps {
        DeviceCaps {
            display: false,
            interactive_input: true,
            touch: false,
            large_display: false,
        }
    }

    fn accepted() -> crate::Pairing {
        let mut pairing = crate::Pairing::offer(b"pairing-code-16b", 0, 30).unwrap();
        pairing.accept(b"pairing-code-16b", true, 1).unwrap();
        pairing
    }

    fn negotiated() -> (Channel, Channel) {
        let manifest = SCHEMA.manifest().unwrap();
        let mut origin = Channel::establish(tv(), phone(), &accepted()).unwrap();
        let mut follower = Channel::establish(phone(), tv(), &accepted()).unwrap();
        let hello = origin.seal_hello(&manifest).unwrap();
        let reply = follower.seal_hello(&manifest).unwrap();
        origin.open_hello(&manifest, &reply).unwrap();
        follower.open_hello(&manifest, &hello).unwrap();
        (origin, follower)
    }

    #[test]
    fn controller_does_not_receive_mutation_rights() {
        let assignment = assign_surface(&controller_decl(), &controller_caps()).unwrap();
        assert!(assignment.permissions.submit_intent);
        assert!(!assignment.permissions.mutate);
        assert!(!assignment.touch_available);
        assert!(!assignment.preferences_met);
    }

    #[test]
    fn missing_display_is_rejected() {
        let error = assign_surface(
            &board_decl(),
            &DeviceCaps {
                display: false,
                interactive_input: true,
                touch: false,
                large_display: true,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E7001");
    }

    #[test]
    fn missing_preference_still_assigns() {
        let assignment = assign_surface(
            &board_decl(),
            &DeviceCaps {
                display: true,
                interactive_input: true,
                touch: false,
                large_display: false,
            },
        )
        .unwrap();
        assert!(assignment.permissions.mutate);
        assert!(!assignment.preferences_met);
        assert!(!assignment.large_display_available);
    }

    #[test]
    fn roles_cannot_take_or_drop_the_writer() {
        let mut origin = open(tv(), id_from_byte(9), &deck()).unwrap();
        let error = adopt_surface(&mut origin, &controller_decl(), &controller_caps()).unwrap_err();
        assert_eq!(error.code, "RMT-E4006");
        assert!(origin.is_writer());

        let mut follower = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
        let error = adopt_surface(&mut follower, &board_decl(), &board_caps()).unwrap_err();
        assert_eq!(error.code, "RMT-E4006");
        assert!(!follower.is_writer());
        assert!(follower.permissions().submit_intent);
    }

    #[test]
    fn capability_record_is_four_booleans() {
        let bytes = encode_capabilities(&controller_caps()).unwrap();
        assert_eq!(decode_capabilities(&bytes).unwrap(), controller_caps());
        let mut extra = BTreeMap::new();
        extra.insert(1, Value::Bool(false));
        extra.insert(2, Value::Bool(true));
        extra.insert(3, Value::Bool(false));
        extra.insert(4, Value::Bool(false));
        extra.insert(5, Value::Bool(true));
        let error = decode_capabilities(&encode(&Value::Record(extra)).unwrap()).unwrap_err();
        assert_eq!(error.code, "RMT-E2001");
        let mut illegal = bytes;
        let last = illegal.len() - 1;
        illegal[last] = 2;
        assert_eq!(decode_capabilities(&illegal).unwrap_err().code, "RMT-E2001");
    }

    #[test]
    fn controller_submits_cards_the_board_commits() {
        let mut origin = open(tv(), id_from_byte(9), &deck()).unwrap();
        let board = adopt_surface(&mut origin, &board_decl(), &board_caps()).unwrap();
        assert!(board.preferences_met);
        assert!(origin.is_writer());
        origin.set_local(F_VIEWPORT, Value::Int(1920)).unwrap();

        let mut follower = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
        let controls =
            adopt_surface(&mut follower, &controller_decl(), &controller_caps()).unwrap();
        assert!(!controls.preferences_met);
        assert!(controls.permissions.submit_intent);
        assert!(!controls.permissions.mutate);
        follower.set_local(F_VIEWPORT, Value::Int(1080)).unwrap();

        let (mut board_channel, mut controller_channel) = negotiated();
        let idle = format!("{board_channel:?}{controller_channel:?}");
        let refused = deliver_capabilities(
            &mut controller_channel,
            &mut board_channel,
            &controller_decl(),
            &DeviceCaps {
                display: false,
                interactive_input: false,
                touch: true,
                large_display: false,
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "RMT-E7001");
        assert_eq!(format!("{board_channel:?}{controller_channel:?}"), idle);

        let accepted = deliver_capabilities(
            &mut controller_channel,
            &mut board_channel,
            &controller_decl(),
            &controller_caps(),
        )
        .unwrap();
        assert_eq!(accepted, controls);
        let plain = encode_capabilities(&controller_caps()).unwrap();
        assert_eq!(decode_capabilities(&plain).unwrap(), controller_caps());

        play(
            &mut controller_channel,
            &mut board_channel,
            &mut origin,
            &mut follower,
            0,
        );
        play(
            &mut controller_channel,
            &mut board_channel,
            &mut origin,
            &mut follower,
            7,
        );
        assert_eq!(origin.revision(), 2);
        assert_eq!(follower.revision(), 2);
        assert_eq!(origin.values().get(&F_SCORE), Some(&Value::Int(1)));
        assert_eq!(completion_permille(&origin).unwrap(), 250);
        assert_eq!(
            origin.fingerprint().unwrap(),
            follower.fingerprint().unwrap()
        );
        assert!(!follower.is_writer());
        assert!(origin.is_writer());
        assert_eq!(origin.local(F_VIEWPORT), Some(&Value::Int(1920)));
        assert_eq!(follower.local(F_VIEWPORT), Some(&Value::Int(1080)));
        let Value::Record(fields) = decode(&follower.portable_document().unwrap()).unwrap() else {
            panic!("portable body is not a record");
        };
        assert!(!fields.contains_key(&F_VIEWPORT));
        assert_eq!(
            origin
                .commit_authority(Vec::new(), state_ok)
                .unwrap_err()
                .code,
            "RMT-E5004"
        );
        assert_eq!(
            follower
                .commit_authority(
                    vec![crate::mutation::Operation::Set {
                        field: F_SCORE,
                        value: Value::Int(2),
                    }],
                    |_| true,
                )
                .unwrap_err()
                .code,
            "RMT-E5005"
        );
        assert_eq!(
            follower.prepare_handoff(tv()).unwrap_err().code,
            "RMT-E5005"
        );

        let mut statistics = join(id_from_byte(3), &origin.encode_snapshot().unwrap()).unwrap();
        let reading = adopt_surface(
            &mut statistics,
            &SurfaceDecl {
                role: SurfaceRole::Auxiliary,
                requires_display: true,
                requires_input: false,
                prefers_touch: false,
                prefers_large_display: false,
            },
            &DeviceCaps {
                display: true,
                interactive_input: false,
                touch: false,
                large_display: false,
            },
        )
        .unwrap();
        assert!(!reading.permissions.submit_intent);
        assert!(!reading.permissions.mutate);
        assert_eq!(completion_permille(&statistics).unwrap(), 250);
        assert_eq!(
            enqueue_select(&mut statistics, 1).unwrap_err().code,
            "RMT-E4006"
        );
        assert_eq!(
            statistics.fingerprint().unwrap(),
            origin.fingerprint().unwrap()
        );
    }

    fn play(
        controller_channel: &mut Channel,
        board_channel: &mut Channel,
        board: &mut Replica,
        controller: &mut Replica,
        card: u32,
    ) {
        enqueue_select(controller, card).unwrap();
        assert_eq!(controller.pending_len(), 1);
        deliver_intent(
            controller_channel,
            board_channel,
            board,
            controller,
            Intent {
                action: 1,
                argument: u64::from(card),
                origin: phone(),
            },
            execute_intent,
            state_ok,
        )
        .unwrap();
        assert_eq!(controller.pending_len(), 1);
        assert!(controller.acknowledge_pending(1, u64::from(card)));
        assert_eq!(controller.pending_len(), 0);
    }
}
