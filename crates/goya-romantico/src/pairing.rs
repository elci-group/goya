//! Explicit pairing consent.
//!
//! The core does not read the operating-system random source. The caller
//! supplies 16 bytes. Five mismatches lock the offer. Expiry uses a caller
//! tick, not a wall clock.

use crate::Error;

#[derive(Clone, PartialEq, Eq)]
pub struct Pairing {
    code: [u8; 16],
    expires_at: u64,
    attempts: u8,
    locked: bool,
    accepted: bool,
}

impl Pairing {
    pub fn offer(code: &[u8], now: u64, ttl: u64) -> Result<Self, Error> {
        if code.len() != 16 {
            return Err(Error::new("RMT-E4002", "pairing code must be 16 bytes"));
        }
        if ttl == 0 {
            return Err(Error::new("RMT-E4004", "pairing lifetime must be non-zero"));
        }
        let expires_at = now
            .checked_add(ttl)
            .ok_or(Error::new("RMT-E8001", "pairing expiry overflowed"))?;
        let mut owned = [0u8; 16];
        owned.copy_from_slice(code);
        Ok(Self {
            code: owned,
            expires_at,
            attempts: 0,
            locked: false,
            accepted: false,
        })
    }

    pub fn accept(&mut self, code: &[u8], consent: bool, now: u64) -> Result<(), Error> {
        if self.locked {
            return Err(Error::new("RMT-E4003", "pairing attempts are exhausted"));
        }
        if now > self.expires_at {
            return Err(Error::new("RMT-E4004", "pairing code has expired"));
        }
        if !consent {
            return Err(Error::new("RMT-E4001", "pairing requires explicit consent"));
        }
        if code.len() != 16 || !constant_time_eq(code, &self.code) {
            self.attempts = self.attempts.saturating_add(1);
            if self.attempts >= 5 {
                self.locked = true;
            }
            return Err(Error::new("RMT-E4002", "pairing code was rejected"));
        }
        self.accepted = true;
        Ok(())
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub(crate) fn shared_secret(&self) -> Result<[u8; 16], Error> {
        if !self.accepted {
            return Err(Error::new("RMT-E4001", "pairing requires explicit consent"));
        }
        Ok(self.code)
    }
}

impl std::fmt::Debug for Pairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pairing")
            .field("expires_at", &self.expires_at)
            .field("attempts", &self.attempts)
            .field("locked", &self.locked)
            .field("accepted", &self.accepted)
            .finish()
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> [u8; 16] {
        *b"pairing-code-16b"
    }

    #[test]
    fn consent_is_required() {
        let mut offer = Pairing::offer(&code(), 0, 10).unwrap();
        let error = offer.accept(&code(), false, 1).unwrap_err();
        assert_eq!(error.code, "RMT-E4001");
        assert!(!offer.is_accepted());
    }

    #[test]
    fn short_code_is_rejected() {
        let error = Pairing::offer(b"short", 0, 10).unwrap_err();
        assert_eq!(error.code, "RMT-E4002");
    }

    #[test]
    fn five_mismatches_lock_the_offer() {
        let mut offer = Pairing::offer(&code(), 0, 10).unwrap();
        let wrong = [9u8; 16];
        for _ in 0..5 {
            assert_eq!(offer.accept(&wrong, true, 1).unwrap_err().code, "RMT-E4002");
        }
        assert_eq!(
            offer.accept(&code(), true, 1).unwrap_err().code,
            "RMT-E4003"
        );
    }

    #[test]
    fn expired_code_is_rejected() {
        let mut offer = Pairing::offer(&code(), 5, 2).unwrap();
        assert_eq!(
            offer.accept(&code(), true, 8).unwrap_err().code,
            "RMT-E4004"
        );
    }
}
