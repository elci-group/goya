//! Key derivation and sealing of `goya.lanlink/0`, from Tapestry's
//! `tv_link.rs`. The golden vector in the tests is the one pinned in
//! Tapestry and in Arachnid's `tests/runtime/lanlink-vectors.html`.

use crate::wire::{INFO_KEYS, INFO_SAS, PROTOCOL};
use ring::{aead, digest, hkdf};

/// Keys and code derived from one ECDH agreement.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Derived {
    pub tv_to_desktop: [u8; 32],
    pub desktop_to_tv: [u8; 32],
    pub sas: String,
}

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// HKDF-SHA256 over the shared secret, salted with both public keys.
pub fn derive(z: &[u8], tv_pub: &[u8], desk_pub: &[u8]) -> Derived {
    let mut both = Vec::with_capacity(tv_pub.len() + desk_pub.len());
    both.extend_from_slice(tv_pub);
    both.extend_from_slice(desk_pub);
    let salt_bytes = digest::digest(&digest::SHA256, &both);
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt_bytes.as_ref()).extract(z);
    let mut okm = [0u8; 64];
    prk.expand(&[INFO_KEYS], Len(64))
        .and_then(|o| o.fill(&mut okm))
        .expect("64 bytes is within HKDF-SHA256 limits");
    let mut sas_bytes = [0u8; 4];
    prk.expand(&[INFO_SAS], Len(4))
        .and_then(|o| o.fill(&mut sas_bytes))
        .expect("4 bytes is within HKDF-SHA256 limits");
    let mut tv_to_desktop = [0u8; 32];
    let mut desktop_to_tv = [0u8; 32];
    tv_to_desktop.copy_from_slice(&okm[..32]);
    desktop_to_tv.copy_from_slice(&okm[32..]);
    Derived {
        tv_to_desktop,
        desktop_to_tv,
        sas: format!("{:06}", u32::from_be_bytes(sas_bytes) % 1_000_000),
    }
}

/// `PROTOCOL ‖ 0x00 ‖ pairing id ‖ 0x00 ‖ path`.
pub fn aad(pairing_id: &str, path: &str) -> Vec<u8> {
    let mut a = Vec::new();
    a.extend_from_slice(PROTOCOL.as_bytes());
    a.push(0);
    a.extend_from_slice(pairing_id.as_bytes());
    a.push(0);
    a.extend_from_slice(path.as_bytes());
    a
}

fn nonce(ctr: u64) -> aead::Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&ctr.to_be_bytes());
    aead::Nonce::assume_unique_for_key(n)
}

fn key(bytes: &[u8; 32]) -> aead::LessSafeKey {
    aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, bytes).expect("32-byte key"))
}

pub fn seal(k: &[u8; 32], ctr: u64, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let mut buf = plaintext.to_vec();
    key(k)
        .seal_in_place_append_tag(nonce(ctr), aead::Aad::from(aad), &mut buf)
        .expect("AES-GCM seal");
    buf
}

pub fn open(k: &[u8; 32], ctr: u64, aad: &[u8], ct: &[u8]) -> Option<Vec<u8>> {
    let mut buf = ct.to_vec();
    let plain = key(k).open_in_place(nonce(ctr), aead::Aad::from(aad), &mut buf).ok()?;
    Some(plain.to_vec())
}

pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    digest::digest(&digest::SHA256, bytes).as_ref().to_vec()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::wire::MSG_PATH;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use ring::agreement;
    use ring::rand::SystemRandom;

    /// Golden vector shared with Tapestry and Arachnid's TV runtime.
    pub fn vector() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let z: Vec<u8> = (0u8..32).collect();
        let tv: Vec<u8> = (0u8..65).map(|i| i.wrapping_mul(3)).collect();
        let desk: Vec<u8> = (0u8..65).map(|i| i.wrapping_mul(7)).collect();
        (z, tv, desk)
    }

    #[test]
    fn golden_vector_matches_tapestry_and_arachnid() {
        let (z, tv, desk) = vector();
        let d = derive(&z, &tv, &desk);
        let a = aad("00112233445566778899aabbccddeeff", MSG_PATH);
        let ct = seal(&d.tv_to_desktop, 1, &a, br#"{"type":"confirm"}"#);
        assert_eq!(d.sas, "697695");
        assert_eq!(hex(&sha256(&d.tv_to_desktop)), "fdfaed7d64ebabfcb079d547f7608892116000e18c2c9d2e298923738202394c");
        assert_eq!(hex(&sha256(&d.desktop_to_tv)), "98c3686e316fbe2f3b84dcbe2e3e909748630fb4efe956ec3c3a23c6af7ad24f");
        assert_eq!(B64.encode(&ct), "7tyNrS4BmMrEYTaYRiqaYLkfbllRRo8cOHPyk/tSxmEP+Q==");
        assert_eq!(open(&d.tv_to_desktop, 1, &a, &ct).unwrap(), br#"{"type":"confirm"}"#);
        assert!(open(&d.tv_to_desktop, 2, &a, &ct).is_none(), "counter is bound");
        assert!(open(&d.desktop_to_tv, 1, &a, &ct).is_none(), "direction is bound");
        assert!(open(&d.tv_to_desktop, 1, &aad("other", MSG_PATH), &ct).is_none(), "pairing is bound");
    }

    #[test]
    fn real_ecdh_agrees_on_both_sides() {
        let rng = SystemRandom::new();
        let a = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let b = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let (pa, pb) = (a.compute_public_key().unwrap(), b.compute_public_key().unwrap());
        assert_eq!(pa.as_ref().len(), 65, "raw uncompressed, as WebCrypto exports");
        let za = agreement::agree_ephemeral(a, &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, pb.as_ref()), |z| z.to_vec()).unwrap();
        let zb = agreement::agree_ephemeral(b, &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, pa.as_ref()), |z| z.to_vec()).unwrap();
        assert_eq!(derive(&za, pa.as_ref(), pb.as_ref()), derive(&zb, pa.as_ref(), pb.as_ref()));
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(hex_decode(&hex(&[0, 15, 255])).unwrap(), vec![0, 15, 255]);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }
}
