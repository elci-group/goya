//! Transport admission and discovery fallback.
//!
//! The crate opens no socket. mDNS, Bluetooth, platform discovery, and a
//! rendezvous service are not implemented, so a caller that claims them still
//! gets the manual endpoint. Pairing-code discovery stays available beside
//! that fallback. The encrypted channel lives in [`crate::channel`].

use crate::canonical::MAX_BYTES;
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportCapabilities {
    pub authenticated_encryption: bool,
    pub reliable: bool,
    pub max_message_bytes: u32,
}

/// Mechanisms named by the directive. Only the manual endpoint and the
/// pairing-code fallback are implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryMechanism {
    ManualEndpoint,
    PairingCode,
    Mdns,
    PlatformNative,
    BluetoothLe,
    Rendezvous,
}

/// What a host claims it can do. A true bit is not proof that this crate
/// has an adapter for that bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformDiscovery {
    pub multicast: bool,
    pub inbound_sockets: bool,
    pub background_execution: bool,
    pub bluetooth_le: bool,
    pub platform_native: bool,
    pub rendezvous: bool,
}

/// Discovery decision for one host report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoveryChoice {
    pub selected: DiscoveryMechanism,
    pub pairing_fallback: DiscoveryMechanism,
    pub reported: PlatformDiscovery,
}

/// Keep the manual endpoint until a host adapter has been measured.
///
/// Claimed multicast, inbound sockets, background execution, Bluetooth,
/// platform-native discovery, and rendezvous are preserved on
/// [`DiscoveryChoice::reported`] and are not contacted.
pub fn select_discovery(reported: PlatformDiscovery) -> DiscoveryChoice {
    DiscoveryChoice {
        selected: DiscoveryMechanism::ManualEndpoint,
        pairing_fallback: DiscoveryMechanism::PairingCode,
        reported,
    }
}

pub fn admit_transport(caps: &TransportCapabilities) -> Result<(), Error> {
    if !caps.authenticated_encryption || !caps.reliable {
        return Err(Error::new(
            "RMT-E4005",
            "transport lacks authenticated encryption",
        ));
    }
    if caps.max_message_bytes == 0 || caps.max_message_bytes > MAX_BYTES as u32 {
        return Err(Error::new(
            "RMT-E8001",
            "transport message bound is outside the canonical budget",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_transport_is_refused() {
        let error = admit_transport(&TransportCapabilities {
            authenticated_encryption: false,
            reliable: true,
            max_message_bytes: 4096,
        })
        .unwrap_err();
        assert_eq!(error.code, "RMT-E4005");
    }

    #[test]
    fn unmeasured_claims_stay_on_the_manual_endpoint() {
        let quiet = select_discovery(PlatformDiscovery {
            multicast: false,
            inbound_sockets: false,
            background_execution: false,
            bluetooth_le: false,
            platform_native: false,
            rendezvous: false,
        });
        let loud = select_discovery(PlatformDiscovery {
            multicast: true,
            inbound_sockets: true,
            background_execution: true,
            bluetooth_le: true,
            platform_native: true,
            rendezvous: true,
        });
        assert_eq!(quiet.selected, DiscoveryMechanism::ManualEndpoint);
        assert_eq!(loud.selected, DiscoveryMechanism::ManualEndpoint);
        assert_eq!(quiet.pairing_fallback, DiscoveryMechanism::PairingCode);
        assert_eq!(loud.pairing_fallback, DiscoveryMechanism::PairingCode);
        assert!(loud.reported.multicast);
        assert!(loud.reported.rendezvous);
        assert_ne!(loud.selected, DiscoveryMechanism::Mdns);
        assert_ne!(loud.selected, DiscoveryMechanism::Rendezvous);
    }
}
