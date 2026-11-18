//! Deterministic process-local runtimes for the vendor hardware caprichios.
//!
//! [`KeychainVault`] models the contract of a namespace-scoped, value-bounded
//! Keychain key-value surface. [`MetalDispatchPlan`] performs the checked
//! arithmetic of an Apple Silicon GPU dispatch without touching a GPU.
//! [`BiometricGate`] and [`PaymentRequest`] are vendor-neutral primitives
//! shared by the Touch ID, Face ID, Windows Hello, Apple Pay, Google Pay, and
//! Samsung Pay caprichios. None of these types call platform frameworks;
//! hosts running on real hardware report their real facilities through the
//! caprichio runtime capabilities instead.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeychainError {
    EmptyKey,
    KeyTooLong,
    ValueTooLarge,
    VaultFull,
    UnknownKey,
    Poisoned,
}

impl fmt::Display for KeychainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyKey => "keychain key must not be empty",
            Self::KeyTooLong => "keychain key exceeds the supported bound",
            Self::ValueTooLarge => "value exceeds the negotiated bound",
            Self::VaultFull => "vault holds the negotiated maximum item count",
            Self::UnknownKey => "no item exists under that key",
            Self::Poisoned => "vault lock was poisoned",
        })
    }
}

impl std::error::Error for KeychainError {}

/// Namespace-scoped bounded key-value store mirroring Keychain semantics:
/// overwrite-in-place, distinct items per key, no cross-namespace reads.
#[derive(Debug)]
pub struct KeychainVault {
    namespace: String,
    max_value_bytes: usize,
    max_items: usize,
    items: Mutex<BTreeMap<String, Vec<u8>>>,
}

pub const MAX_KEY_BYTES: usize = 256;
pub const MAX_ITEMS: usize = 1_000_000;

impl KeychainVault {
    pub fn new(
        namespace: String,
        max_value_bytes: usize,
        max_items: usize,
    ) -> Result<Self, KeychainError> {
        if namespace.is_empty() {
            return Err(KeychainError::EmptyKey);
        }
        if max_value_bytes == 0 {
            return Err(KeychainError::ValueTooLarge);
        }
        if max_items == 0 || max_items > MAX_ITEMS {
            return Err(KeychainError::VaultFull);
        }
        Ok(Self {
            namespace,
            max_value_bytes,
            max_items,
            items: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }

    pub fn max_items(&self) -> usize {
        self.max_items
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>>, KeychainError> {
        self.items.lock().map_err(|_| KeychainError::Poisoned)
    }

    pub fn set(&self, key: &str, value: &[u8]) -> Result<(), KeychainError> {
        if key.is_empty() {
            return Err(KeychainError::EmptyKey);
        }
        if key.len() > MAX_KEY_BYTES {
            return Err(KeychainError::KeyTooLong);
        }
        if value.len() > self.max_value_bytes {
            return Err(KeychainError::ValueTooLarge);
        }
        let mut items = self.lock()?;
        if !items.contains_key(key) && items.len() == self.max_items {
            return Err(KeychainError::VaultFull);
        }
        items.insert(key.to_owned(), value.to_vec());
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KeychainError> {
        Ok(self.lock()?.get(key).cloned())
    }

    pub fn delete(&self, key: &str) -> Result<(), KeychainError> {
        let mut items = self.lock()?;
        items
            .remove(key)
            .map(|_| ())
            .ok_or(KeychainError::UnknownKey)
    }

    pub fn len(&self) -> Result<usize, KeychainError> {
        Ok(self.lock()?.len())
    }

    pub fn is_empty(&self) -> Result<bool, KeychainError> {
        Ok(self.lock()?.is_empty())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetalError {
    ZeroThreadsPerGroup,
    ThreadsPerGroupNotPowerOfTwo,
    ThreadsPerGroupTooLarge,
    ZeroGroups,
    GroupsTooLarge,
    ZeroBufferBytes,
    BufferTooLarge,
    ThreadCountOverflow,
}

impl fmt::Display for MetalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroThreadsPerGroup => "threads per threadgroup must be positive",
            Self::ThreadsPerGroupNotPowerOfTwo => "threads per threadgroup must be a power of two",
            Self::ThreadsPerGroupTooLarge => "threads per threadgroup exceeds the Metal limit of 1024",
            Self::ZeroGroups => "threadgroup count must be positive",
            Self::GroupsTooLarge => "threadgroup count exceeds the supported bound",
            Self::ZeroBufferBytes => "buffer bound must be positive",
            Self::BufferTooLarge => "buffer bound exceeds the supported bound",
            Self::ThreadCountOverflow => "total thread count does not fit a u32 dispatch",
        })
    }
}

impl std::error::Error for MetalError {}

pub const MAX_THREADS_PER_GROUP: u32 = 1024;
pub const MAX_GROUPS: u32 = 1_000_000;
pub const MAX_BUFFER_BYTES: u64 = 1 << 30;

/// Checked dispatch arithmetic for an Apple Silicon Metal kernel launch.
/// This type plans the dispatch; it never compiles shaders or enqueues work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetalDispatchPlan {
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
}

impl MetalDispatchPlan {
    pub fn new(
        threads_per_group: u32,
        groups: u32,
        max_buffer_bytes: u64,
    ) -> Result<Self, MetalError> {
        if threads_per_group == 0 {
            return Err(MetalError::ZeroThreadsPerGroup);
        }
        if !threads_per_group.is_power_of_two() {
            return Err(MetalError::ThreadsPerGroupNotPowerOfTwo);
        }
        if threads_per_group > MAX_THREADS_PER_GROUP {
            return Err(MetalError::ThreadsPerGroupTooLarge);
        }
        if groups == 0 {
            return Err(MetalError::ZeroGroups);
        }
        if groups > MAX_GROUPS {
            return Err(MetalError::GroupsTooLarge);
        }
        if max_buffer_bytes == 0 {
            return Err(MetalError::ZeroBufferBytes);
        }
        if max_buffer_bytes > MAX_BUFFER_BYTES {
            return Err(MetalError::BufferTooLarge);
        }
        if u32::try_from(groups as u64 * threads_per_group as u64).is_err() {
            return Err(MetalError::ThreadCountOverflow);
        }
        Ok(Self {
            threads_per_group,
            groups,
            max_buffer_bytes,
        })
    }

    pub fn threads_per_group(&self) -> u32 {
        self.threads_per_group
    }

    pub fn groups(&self) -> u32 {
        self.groups
    }

    pub fn max_buffer_bytes(&self) -> u64 {
        self.max_buffer_bytes
    }

    pub fn total_threads(&self) -> u32 {
        self.groups * self.threads_per_group
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthError {
    ZeroAttempts,
    AttemptsTooLarge,
    ZeroTimeout,
    TimeoutTooLarge,
    AttemptsExhausted,
    AlreadyResolved,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroAttempts => "attempt bound must be positive",
            Self::AttemptsTooLarge => "attempt bound exceeds the supported bound",
            Self::ZeroTimeout => "prompt timeout must be positive",
            Self::TimeoutTooLarge => "prompt timeout exceeds the supported bound",
            Self::AttemptsExhausted => "attempt bound was reached without a match",
            Self::AlreadyResolved => "authentication already admitted or denied",
        })
    }
}

impl std::error::Error for AuthError {}

pub const MAX_ATTEMPTS: u8 = 8;
pub const MAX_AUTH_TIMEOUT_MS: u32 = 120_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthOutcome {
    /// Match accepted; the gate is resolved.
    Admitted,
    /// No match yet; attempts remain.
    Retry,
}

/// Deterministic attempt-accounting for a bounded biometric prompt. This type
/// counts attempts and enforces the negotiated bounds; the sensor match itself
/// is supplied by the caller, so no biometric hardware is touched.
#[derive(Debug)]
pub struct BiometricGate {
    max_attempts: u8,
    timeout_ms: u32,
    attempts: u8,
    resolved: bool,
}

impl BiometricGate {
    pub fn new(max_attempts: u8, timeout_ms: u32) -> Result<Self, AuthError> {
        if max_attempts == 0 {
            return Err(AuthError::ZeroAttempts);
        }
        if max_attempts > MAX_ATTEMPTS {
            return Err(AuthError::AttemptsTooLarge);
        }
        if timeout_ms == 0 {
            return Err(AuthError::ZeroTimeout);
        }
        if timeout_ms > MAX_AUTH_TIMEOUT_MS {
            return Err(AuthError::TimeoutTooLarge);
        }
        Ok(Self {
            max_attempts,
            timeout_ms,
            attempts: 0,
            resolved: false,
        })
    }

    pub fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// Record one sensor result. A match admits and resolves the gate; the
    /// final unmatched attempt resolves it as exhausted.
    pub fn attempt(&mut self, matched: bool) -> Result<AuthOutcome, AuthError> {
        if self.resolved {
            return Err(AuthError::AlreadyResolved);
        }
        self.attempts = self.attempts.saturating_add(1);
        if matched {
            self.resolved = true;
            return Ok(AuthOutcome::Admitted);
        }
        if self.attempts >= self.max_attempts {
            self.resolved = true;
            return Err(AuthError::AttemptsExhausted);
        }
        Ok(AuthOutcome::Retry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentError {
    BadMerchantId,
    BadCurrencyCode,
    ZeroAmountBound,
    AmountBoundTooLarge,
    ZeroAmount,
    AmountTooLarge,
    UnsupportedNetwork,
}

impl fmt::Display for PaymentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadMerchantId => "merchant identifier must be a non-empty dotted name",
            Self::BadCurrencyCode => "currency code must be three uppercase ASCII letters",
            Self::ZeroAmountBound => "payment amount bound must be positive",
            Self::AmountBoundTooLarge => "payment amount bound exceeds the supported bound",
            Self::ZeroAmount => "payment amount must be positive",
            Self::AmountTooLarge => "payment amount exceeds the negotiated bound",
            Self::UnsupportedNetwork => "payment network is not in the negotiated allowlist",
        })
    }
}

impl std::error::Error for PaymentError {}

pub const MAX_MERCHANT_ID_BYTES: usize = 64;
pub const MAX_MINOR_UNITS: u64 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentNetwork {
    Visa,
    MasterCard,
    Amex,
}

impl PaymentNetwork {
    pub fn code(self) -> &'static str {
        match self {
            Self::Visa => "visa",
            Self::MasterCard => "mastercard",
            Self::Amex => "amex",
        }
    }
}

/// A validated Apple Pay payment intent: bounded amount, fixed ISO currency
/// code, and a network allowlist. This type validates the request; it never
/// contacts a payment network or claims funds moved.
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentRequest {
    merchant_id: String,
    currency_code: String,
    max_minor_units: u64,
}

impl PaymentRequest {
    pub fn new(
        merchant_id: String,
        currency_code: &str,
        max_minor_units: u64,
    ) -> Result<Self, PaymentError> {
        if merchant_id.is_empty()
            || merchant_id.len() > MAX_MERCHANT_ID_BYTES
            || !merchant_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(PaymentError::BadMerchantId);
        }
        let bytes = currency_code.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(|b| b.is_ascii_uppercase()) {
            return Err(PaymentError::BadCurrencyCode);
        }
        if max_minor_units == 0 {
            return Err(PaymentError::ZeroAmountBound);
        }
        if max_minor_units > MAX_MINOR_UNITS {
            return Err(PaymentError::AmountBoundTooLarge);
        }
        Ok(Self {
            merchant_id,
            currency_code: currency_code.to_owned(),
            max_minor_units,
        })
    }

    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    pub fn currency_code(&self) -> &str {
        &self.currency_code
    }

    pub fn max_minor_units(&self) -> u64 {
        self.max_minor_units
    }

    /// Check one payment against the amount bound and network allowlist.
    pub fn authorize(&self, network: PaymentNetwork, minor_units: u64) -> Result<(), PaymentError> {
        if minor_units == 0 {
            return Err(PaymentError::ZeroAmount);
        }
        if minor_units > self.max_minor_units {
            return Err(PaymentError::AmountTooLarge);
        }
        // Default allowlist: the two card networks the recipe supports.
        match network {
            PaymentNetwork::Visa | PaymentNetwork::MasterCard => Ok(()),
            PaymentNetwork::Amex => Err(PaymentError::UnsupportedNetwork),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn biometric_gate_counts_attempts_and_expires() {
        let mut gate = BiometricGate::new(3, 30_000).unwrap();
        assert_eq!(gate.max_attempts(), 3);
        assert_eq!(gate.attempt(false), Ok(AuthOutcome::Retry));
        assert_eq!(gate.attempt(false), Ok(AuthOutcome::Retry));
        assert_eq!(gate.attempt(true), Ok(AuthOutcome::Admitted));
        assert_eq!(gate.attempt(true), Err(AuthError::AlreadyResolved));

        let mut exhausted = BiometricGate::new(2, 30_000).unwrap();
        assert_eq!(exhausted.attempt(false), Ok(AuthOutcome::Retry));
        assert_eq!(exhausted.attempt(false), Err(AuthError::AttemptsExhausted));

        assert_eq!(BiometricGate::new(0, 30_000).err(), Some(AuthError::ZeroAttempts));
        assert_eq!(
            BiometricGate::new(9, 30_000).err(),
            Some(AuthError::AttemptsTooLarge)
        );
        assert_eq!(
            BiometricGate::new(3, MAX_AUTH_TIMEOUT_MS + 1).err(),
            Some(AuthError::TimeoutTooLarge)
        );
        assert_eq!(
            BiometricGate::new(3, 0).err(),
            Some(AuthError::ZeroTimeout)
        );
    }

    #[test]
    fn payment_request_validates_currency_amount_and_networks() {
        let request =
            PaymentRequest::new("com.goya.merchant".into(), "USD", 100_000).unwrap();
        assert_eq!(request.currency_code(), "USD");
        assert_eq!(request.max_minor_units(), 100_000);
        assert_eq!(request.authorize(PaymentNetwork::Visa, 42_500), Ok(()));
        assert_eq!(
            request.authorize(PaymentNetwork::Visa, 100_001),
            Err(PaymentError::AmountTooLarge)
        );
        assert_eq!(
            request.authorize(PaymentNetwork::Amex, 1),
            Err(PaymentError::UnsupportedNetwork)
        );
        assert_eq!(
            request.authorize(PaymentNetwork::Visa, 0),
            Err(PaymentError::ZeroAmount)
        );

        assert_eq!(
            PaymentRequest::new(String::new(), "USD", 100).err(),
            Some(PaymentError::BadMerchantId)
        );
        assert_eq!(
            PaymentRequest::new("bad merchant!".into(), "USD", 100).err(),
            Some(PaymentError::BadMerchantId)
        );
        assert_eq!(
            PaymentRequest::new("m".repeat(MAX_MERCHANT_ID_BYTES + 1), "USD", 100).err(),
            Some(PaymentError::BadMerchantId)
        );
        assert_eq!(
            PaymentRequest::new("com.goya.merchant".into(), "usd", 100).err(),
            Some(PaymentError::BadCurrencyCode)
        );
        assert_eq!(
            PaymentRequest::new("com.goya.merchant".into(), "USDD", 100).err(),
            Some(PaymentError::BadCurrencyCode)
        );
        assert_eq!(
            PaymentRequest::new("com.goya.merchant".into(), "USD", 0).err(),
            Some(PaymentError::ZeroAmountBound)
        );
        assert_eq!(
            PaymentRequest::new("com.goya.merchant".into(), "USD", MAX_MINOR_UNITS + 1).err(),
            Some(PaymentError::AmountBoundTooLarge)
        );
    }

    #[test]
    fn vault_scopes_and_bounds_items() {
        let vault = KeychainVault::new("com.example.app".into(), 64, 2).unwrap();
        assert_eq!(vault.namespace(), "com.example.app");
        vault.set("token", b"abc").unwrap();
        vault.set("token", b"xyz").unwrap();
        assert_eq!(vault.get("token").unwrap(), Some(b"xyz".to_vec()));
        vault.set("other", b"1").unwrap();
        assert_eq!(vault.set("third", b"2"), Err(KeychainError::VaultFull));
        assert_eq!(vault.delete("missing"), Err(KeychainError::UnknownKey));
        vault.delete("token").unwrap();
        assert_eq!(vault.get("token").unwrap(), None);
        assert_eq!(vault.len().unwrap(), 1);
    }

    #[test]
    fn vault_rejects_bad_keys_and_oversized_values() {
        let vault = KeychainVault::new("ns".into(), 4, 8).unwrap();
        assert_eq!(vault.set("", b"a"), Err(KeychainError::EmptyKey));
        assert_eq!(
            vault.set(&"k".repeat(257), b"a"),
            Err(KeychainError::KeyTooLong)
        );
        assert_eq!(vault.set("k", &[0u8; 5]), Err(KeychainError::ValueTooLarge));
        assert_eq!(
            KeychainVault::new(String::new(), 4, 8).err(),
            Some(KeychainError::EmptyKey)
        );
        assert_eq!(
            KeychainVault::new("ns".into(), 0, 8).err(),
            Some(KeychainError::ValueTooLarge)
        );
        assert_eq!(
            KeychainVault::new("ns".into(), 4, 0).err(),
            Some(KeychainError::VaultFull)
        );
    }

    #[test]
    fn vault_is_safe_for_concurrent_writers() {
        let vault = Arc::new(KeychainVault::new("ns".into(), 64, MAX_ITEMS).unwrap());
        let handles: Vec<_> = (0..4)
            .map(|producer| {
                let vault = Arc::clone(&vault);
                thread::spawn(move || {
                    for index in 0..500 {
                        let key = format!("k{producer}-{index}");
                        vault.set(&key, &[producer as u8, index as u8]).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(vault.len().unwrap(), 2000);
    }

    #[test]
    fn dispatch_plan_checks_every_bound() {
        let plan = MetalDispatchPlan::new(64, 32, 4096).unwrap();
        assert_eq!(plan.total_threads(), 2048);
        assert_eq!(
            MetalDispatchPlan::new(0, 32, 4096),
            Err(MetalError::ZeroThreadsPerGroup)
        );
        assert_eq!(
            MetalDispatchPlan::new(63, 32, 4096),
            Err(MetalError::ThreadsPerGroupNotPowerOfTwo)
        );
        assert_eq!(
            MetalDispatchPlan::new(2048, 32, 4096),
            Err(MetalError::ThreadsPerGroupTooLarge)
        );
        assert_eq!(
            MetalDispatchPlan::new(64, 0, 4096),
            Err(MetalError::ZeroGroups)
        );
        assert_eq!(
            MetalDispatchPlan::new(64, MAX_GROUPS + 1, 4096),
            Err(MetalError::GroupsTooLarge)
        );
        assert_eq!(
            MetalDispatchPlan::new(64, 32, 0),
            Err(MetalError::ZeroBufferBytes)
        );
        assert_eq!(
            MetalDispatchPlan::new(64, 32, MAX_BUFFER_BYTES + 1),
            Err(MetalError::BufferTooLarge)
        );
        // `MAX_GROUPS * MAX_THREADS_PER_GROUP` fits a `u32`, so the overflow
        // guard stays defensive; the full valid bound is accepted.
        assert!(MetalDispatchPlan::new(MAX_THREADS_PER_GROUP, MAX_GROUPS, 4096).is_ok());
    }
}
