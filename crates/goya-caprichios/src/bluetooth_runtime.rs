//! Deterministic process-local runtime for the Blue Moon Bluetooth
//! caprichios.
//!
//! [`CentralSession`] models the bounded accounting of a BLE central scan and
//! connection lifecycle: advertisement intake with duplicate suppression, a
//! peripheral state machine that keeps detection, recognition, connection and
//! operation as distinct states, and bounded reconnection with exponential
//! backoff. [`GattSession`] models bounded GATT command accounting: typed
//! characteristic policies, reads and validated writes with acknowledged and
//! unacknowledged modes, and a bounded pending-acknowledgement ledger.
//! [`StreamSession`] models bounded notification intake: declared transports,
//! fail-closed backpressure, deterministic subscription cancellation, and
//! indication confirmations. None of these types drive a radio, contact a
//! peripheral, or prove that a host Bluetooth stack performed any of these
//! operations; hosts running on real hardware report their real facilities
//! through the caprichio runtime capabilities instead.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BluetoothError {
    NoServiceUuids,
    EmptyServiceUuid,
    ServiceUuidTooLong,
    TooManyServiceUuids,
    ZeroScanBound,
    ScanBoundTooLarge,
    ZeroPeripheralBound,
    PeripheralBoundTooLarge,
    ZeroConnectionTimeout,
    ConnectionTimeoutTooLarge,
    ReconnectBoundTooLarge,
    EmptyPeripheralId,
    PeripheralIdTooLong,
    InvalidRssi,
    PeripheralBoundReached,
    UnknownPeripheral,
    WrongState,
    NoRetryRemaining,
    NoCharacteristics,
    TooManyCharacteristics,
    DuplicateCharacteristic,
    ZeroValueBound,
    ValueTooLarge,
    ZeroCommandTimeout,
    CommandTimeoutTooLarge,
    UnknownCommand,
    CommandBoundReached,
    UnsupportedOperation,
    ZeroNotificationBytes,
    NotificationBytesTooLarge,
    ZeroBufferedNotifications,
    BufferedNotificationsTooLarge,
    UnknownSubscription,
    PayloadTooLarge,
    BufferExhausted,
    UnknownOrdinal,
    MalformedPayload,
    ZeroAuthorisationBound,
    AuthorisationBoundTooLarge,
    AuthorisationBoundReached,
    AlreadyAuthorised,
    PermissionUndetermined,
    PermissionDenied,
    NotAuthorised,
    GenericUsageDescription,
    UsageDescriptionTooLong,
    AdvertisementTooLarge,
    ZeroAssociationBound,
    AssociationBoundTooLarge,
    AssociationBoundReached,
    ZeroSubscriptionBound,
    SubscriptionBoundTooLarge,
    TooManySubscriptions,
}

impl fmt::Display for BluetoothError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoServiceUuids => "at least one service UUID filter is required",
            Self::EmptyServiceUuid => "service UUID filter must not be empty",
            Self::ServiceUuidTooLong => "service UUID filter exceeds the supported bound",
            Self::TooManyServiceUuids => {
                "service UUID filter count exceeds the supported bound"
            }
            Self::ZeroScanBound => "scan bound must be positive",
            Self::ScanBoundTooLarge => "scan bound exceeds the supported bound",
            Self::ZeroPeripheralBound => "peripheral bound must be positive",
            Self::PeripheralBoundTooLarge => "peripheral bound exceeds the supported bound",
            Self::ZeroConnectionTimeout => "connection timeout must be positive",
            Self::ConnectionTimeoutTooLarge => {
                "connection timeout exceeds the supported bound"
            }
            Self::ReconnectBoundTooLarge => "reconnect attempt bound exceeds the supported bound",
            Self::EmptyPeripheralId => "peripheral identifier must not be empty",
            Self::PeripheralIdTooLong => "peripheral identifier exceeds the supported bound",
            Self::InvalidRssi => "RSSI must be within the Bluetooth range of -127 to 0 dBm",
            Self::PeripheralBoundReached => {
                "session holds the negotiated maximum peripheral count"
            }
            Self::UnknownPeripheral => "no peripheral exists under that identifier",
            Self::WrongState => {
                "the resource is not in the state the operation requires; detection, \
                 recognition, connection and operation are distinct states, and an \
                 inactive subscription cannot receive"
            }
            Self::NoRetryRemaining => "reconnect attempt bound was reached",
            Self::NoCharacteristics => "at least one declared characteristic is required",
            Self::TooManyCharacteristics => {
                "declared characteristic count exceeds the supported bound"
            }
            Self::DuplicateCharacteristic => {
                "a characteristic UUID was declared more than once"
            }
            Self::ZeroValueBound => "characteristic value bound must be positive",
            Self::ValueTooLarge => "value exceeds the characteristic's negotiated bound",
            Self::ZeroCommandTimeout => "command timeout must be positive",
            Self::CommandTimeoutTooLarge => "command timeout exceeds the supported bound",
            Self::UnknownCommand => "no such command is pending",
            Self::CommandBoundReached => {
                "session holds the negotiated maximum pending commands"
            }
            Self::UnsupportedOperation => {
                "the characteristic's negotiated policy does not permit the operation"
            }
            Self::ZeroNotificationBytes => "notification payload bound must be positive",
            Self::NotificationBytesTooLarge => {
                "notification payload bound exceeds the supported bound"
            }
            Self::ZeroBufferedNotifications => "buffered notification bound must be positive",
            Self::BufferedNotificationsTooLarge => {
                "buffered notification bound exceeds the supported bound"
            }
            Self::UnknownSubscription => "no declared characteristic exists under that UUID",
            Self::PayloadTooLarge => "payload exceeds the negotiated transport limit",
            Self::BufferExhausted => {
                "buffer holds the negotiated maximum notifications; apply backpressure"
            }
            Self::UnknownOrdinal => "no pending confirmation exists for that ordinal",
            Self::MalformedPayload => "payload does not match the negotiated encoding",
            Self::ZeroAuthorisationBound => "authorised-peripheral bound must be positive",
            Self::AuthorisationBoundTooLarge => {
                "authorised-peripheral bound exceeds the supported bound"
            }
            Self::AuthorisationBoundReached => {
                "guard holds the negotiated maximum authorised peripherals"
            }
            Self::AlreadyAuthorised => "peripheral is already authorised",
            Self::PermissionUndetermined => {
                "the Bluetooth permission has not been determined; fail closed"
            }
            Self::PermissionDenied => {
                "the Bluetooth permission was denied; the decision fails closed"
            }
            Self::NotAuthorised => {
                "peripheral was not explicitly selected by the application"
            }
            Self::GenericUsageDescription => {
                "privacy usage description is empty, generic or a placeholder"
            }
            Self::UsageDescriptionTooLong => {
                "privacy usage description exceeds the supported bound"
            }
            Self::AdvertisementTooLarge => {
                "advertisement payload exceeds the legacy BLE advertisement bound"
            }
            Self::ZeroAssociationBound => "association bound must be positive",
            Self::AssociationBoundTooLarge => {
                "association bound exceeds the supported bound"
            }
            Self::AssociationBoundReached => {
                "ledger holds the negotiated maximum associations"
            }
            Self::ZeroSubscriptionBound => "per-association subscription bound must be positive",
            Self::SubscriptionBoundTooLarge => {
                "per-association subscription bound exceeds the supported bound"
            }
            Self::TooManySubscriptions => {
                "restoration subscription list exceeds the negotiated per-association bound"
            }
        })
    }
}

impl std::error::Error for BluetoothError {}

pub const MAX_SCAN_MS: u32 = 300_000;
pub const MAX_SERVICE_UUIDS: usize = 8;
pub const MAX_SERVICE_UUID_BYTES: usize = 64;
pub const MAX_PERIPHERALS: u16 = 32;
pub const MAX_CONNECTION_TIMEOUT_MS: u32 = 60_000;
pub const MAX_RECONNECT_ATTEMPTS: u8 = 8;
pub const MAX_PERIPHERAL_ID_BYTES: usize = 64;
pub const INITIAL_RECONNECT_DELAY_MS: u32 = 100;
pub const MAX_RECONNECT_DELAY_MS: u32 = 5_000;

/// RSSI observation bounds from the Bluetooth Core Specification.
pub const RSSI_MIN_DBM: i8 = -127;
pub const RSSI_MAX_DBM: i8 = 0;

/// Peripheral lifecycle states. Detection, recognition, connection and
/// operation are distinct states and are never interchangeable: a detected
/// peripheral is not connected, and a connected peripheral is not yet
/// operational until its services have been enumerated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeripheralState {
    /// An advertisement was observed for the peripheral.
    Detected,
    /// An advertisement matched one of the session's declared service UUIDs.
    Recognised,
    /// The session accepted a connection to a recognised peripheral.
    Connected,
    /// The connection's services have been enumerated.
    Operational,
    /// The connection ended after being established.
    Disconnected,
}

impl fmt::Display for PeripheralState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Detected => "detected",
            Self::Recognised => "recognised",
            Self::Connected => "connected",
            Self::Operational => "operational",
            Self::Disconnected => "disconnected",
        })
    }
}

/// Result of one advertisement report into the scan session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanOutcome {
    /// A new peripheral entered the session as detected.
    Discovered,
    /// The advertisement matched a declared service UUID filter, or promoted
    /// an earlier detection to recognised.
    Recognised,
    /// A duplicate report for an already-tracked peripheral; the session
    /// suppresses the duplicate and keeps the latest RSSI.
    Duplicate,
}

/// One bounded reconnection plan: the attempt ordinal and the deterministic
/// backoff to wait before the next attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectPlan {
    attempt: u8,
    backoff_ms: u32,
}

impl ReconnectPlan {
    pub fn attempt(&self) -> u8 {
        self.attempt
    }

    pub fn backoff_ms(&self) -> u32 {
        self.backoff_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PeripheralEntry {
    state: PeripheralState,
    rssi: i8,
    recognised: bool,
    reconnects: u8,
    session_id: Option<String>,
}

/// Bounded BLE central scan and connection lifecycle accounting. The session
/// accepts advertisement reports and connection transitions from the host;
/// it never drives the radio itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CentralSession {
    service_uuids: Vec<String>,
    max_scan_ms: u32,
    max_peripherals: u16,
    connection_timeout_ms: u32,
    max_reconnect_attempts: u8,
    peripherals: BTreeMap<String, PeripheralEntry>,
    connections: u64,
}

impl CentralSession {
    pub fn new(
        service_uuids: Vec<String>,
        max_scan_ms: u32,
        max_peripherals: u16,
        connection_timeout_ms: u32,
        max_reconnect_attempts: u8,
    ) -> Result<Self, BluetoothError> {
        if service_uuids.is_empty() {
            return Err(BluetoothError::NoServiceUuids);
        }
        if service_uuids.len() > MAX_SERVICE_UUIDS {
            return Err(BluetoothError::TooManyServiceUuids);
        }
        if service_uuids.iter().any(|uuid| uuid.is_empty()) {
            return Err(BluetoothError::EmptyServiceUuid);
        }
        if service_uuids
            .iter()
            .any(|uuid| uuid.len() > MAX_SERVICE_UUID_BYTES)
        {
            return Err(BluetoothError::ServiceUuidTooLong);
        }
        if max_scan_ms == 0 {
            return Err(BluetoothError::ZeroScanBound);
        }
        if max_scan_ms > MAX_SCAN_MS {
            return Err(BluetoothError::ScanBoundTooLarge);
        }
        if max_peripherals == 0 {
            return Err(BluetoothError::ZeroPeripheralBound);
        }
        if max_peripherals > MAX_PERIPHERALS {
            return Err(BluetoothError::PeripheralBoundTooLarge);
        }
        if connection_timeout_ms == 0 {
            return Err(BluetoothError::ZeroConnectionTimeout);
        }
        if connection_timeout_ms > MAX_CONNECTION_TIMEOUT_MS {
            return Err(BluetoothError::ConnectionTimeoutTooLarge);
        }
        if max_reconnect_attempts > MAX_RECONNECT_ATTEMPTS {
            return Err(BluetoothError::ReconnectBoundTooLarge);
        }
        Ok(Self {
            service_uuids,
            max_scan_ms,
            max_peripherals,
            connection_timeout_ms,
            max_reconnect_attempts,
            peripherals: BTreeMap::new(),
            connections: 0,
        })
    }

    pub fn service_uuids(&self) -> &[String] {
        &self.service_uuids
    }

    pub fn max_scan_ms(&self) -> u32 {
        self.max_scan_ms
    }

    pub fn max_peripherals(&self) -> u16 {
        self.max_peripherals
    }

    pub fn connection_timeout_ms(&self) -> u32 {
        self.connection_timeout_ms
    }

    pub fn max_reconnect_attempts(&self) -> u8 {
        self.max_reconnect_attempts
    }

    /// Number of peripherals currently tracked by the session.
    pub fn len(&self) -> usize {
        self.peripherals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peripherals.is_empty()
    }

    pub fn peripheral_state(&self, peripheral_id: &str) -> Result<PeripheralState, BluetoothError> {
        self.entry(peripheral_id)
            .map(|entry| entry.state)
    }

    fn entry(&self, peripheral_id: &str) -> Result<&PeripheralEntry, BluetoothError> {
        self.peripherals
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)
    }

    fn validate_peripheral_id(peripheral_id: &str) -> Result<(), BluetoothError> {
        if peripheral_id.is_empty() {
            return Err(BluetoothError::EmptyPeripheralId);
        }
        if peripheral_id.len() > MAX_PERIPHERAL_ID_BYTES {
            return Err(BluetoothError::PeripheralIdTooLong);
        }
        Ok(())
    }

    /// Report one advertisement. Duplicate reports are suppressed: the latest
    /// RSSI is kept and the outcome is [`ScanOutcome::Duplicate`] unless the
    /// advertisement newly matches a declared service UUID, which promotes a
    /// detected peripheral to recognised.
    pub fn report_advertisement(
        &mut self,
        peripheral_id: &str,
        rssi: i8,
        advertised_uuids: &[&str],
    ) -> Result<ScanOutcome, BluetoothError> {
        Self::validate_peripheral_id(peripheral_id)?;
        if !(RSSI_MIN_DBM..=RSSI_MAX_DBM).contains(&rssi) {
            return Err(BluetoothError::InvalidRssi);
        }
        let recognised = self
            .service_uuids
            .iter()
            .any(|filter| advertised_uuids.iter().any(|uuid| uuid == filter));
        if let Some(entry) = self.peripherals.get_mut(peripheral_id) {
            entry.rssi = rssi;
            if recognised && !entry.recognised {
                entry.recognised = true;
                if entry.state == PeripheralState::Detected {
                    entry.state = PeripheralState::Recognised;
                }
                return Ok(ScanOutcome::Recognised);
            }
            return Ok(ScanOutcome::Duplicate);
        }
        if self.peripherals.len() == usize::from(self.max_peripherals) {
            return Err(BluetoothError::PeripheralBoundReached);
        }
        let state = if recognised {
            PeripheralState::Recognised
        } else {
            PeripheralState::Detected
        };
        self.peripherals.insert(
            peripheral_id.to_owned(),
            PeripheralEntry {
                state,
                rssi,
                recognised,
                reconnects: 0,
                session_id: None,
            },
        );
        Ok(if recognised {
            ScanOutcome::Recognised
        } else {
            ScanOutcome::Discovered
        })
    }

    /// Connect to a recognised peripheral and return its stable
    /// application-level session identifier. Detected peripherals are not
    /// connectable: the recipe authorises only peripherals whose
    /// advertisements matched a declared service UUID.
    pub fn connect(&mut self, peripheral_id: &str) -> Result<String, BluetoothError> {
        Self::validate_peripheral_id(peripheral_id)?;
        let entry = self.peripherals.get_mut(peripheral_id);
        let entry = entry.ok_or(BluetoothError::UnknownPeripheral)?;
        if entry.state != PeripheralState::Recognised {
            return Err(BluetoothError::WrongState);
        }
        self.connections += 1;
        let session_id = format!("conn-{}-{}", peripheral_id, self.connections);
        entry.state = PeripheralState::Connected;
        entry.session_id = Some(session_id.clone());
        Ok(session_id)
    }

    /// The application-level session identifier of the current connection,
    /// empty until a connection exists.
    pub fn session_id(&self, peripheral_id: &str) -> Result<Option<String>, BluetoothError> {
        Ok(self.entry(peripheral_id)?.session_id.clone())
    }

    /// Observed RSSI, kept as the latest report for a tracked peripheral.
    pub fn rssi(&self, peripheral_id: &str) -> Result<i8, BluetoothError> {
        Ok(self.entry(peripheral_id)?.rssi)
    }

    /// Mark a connected peripheral operational once its services have been
    /// enumerated. A connected peripheral is never implicitly operational.
    pub fn mark_operational(&mut self, peripheral_id: &str) -> Result<(), BluetoothError> {
        Self::validate_peripheral_id(peripheral_id)?;
        let entry = self
            .peripherals
            .get_mut(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?;
        if entry.state != PeripheralState::Connected {
            return Err(BluetoothError::WrongState);
        }
        entry.state = PeripheralState::Operational;
        Ok(())
    }

    /// Observe a link loss for an established connection.
    pub fn observe_disconnect(&mut self, peripheral_id: &str) -> Result<(), BluetoothError> {
        Self::validate_peripheral_id(peripheral_id)?;
        let entry = self
            .peripherals
            .get_mut(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?;
        if !matches!(
            entry.state,
            PeripheralState::Connected | PeripheralState::Operational
        ) {
            return Err(BluetoothError::WrongState);
        }
        entry.state = PeripheralState::Disconnected;
        entry.session_id = None;
        Ok(())
    }

    /// Plan one bounded reconnection for a disconnected peripheral. The
    /// backoff doubles per attempt from [`INITIAL_RECONNECT_DELAY_MS`] and
    /// saturates at [`MAX_RECONNECT_DELAY_MS`].
    pub fn reconnect(&mut self, peripheral_id: &str) -> Result<ReconnectPlan, BluetoothError> {
        Self::validate_peripheral_id(peripheral_id)?;
        let entry = self
            .peripherals
            .get_mut(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?;
        if entry.state != PeripheralState::Disconnected {
            return Err(BluetoothError::WrongState);
        }
        if entry.reconnects >= self.max_reconnect_attempts {
            return Err(BluetoothError::NoRetryRemaining);
        }
        let shift = u32::from(entry.reconnects);
        let backoff_ms = INITIAL_RECONNECT_DELAY_MS
            .saturating_mul(1u32 << shift)
            .min(MAX_RECONNECT_DELAY_MS);
        entry.reconnects += 1;
        self.connections += 1;
        let session_id = format!("conn-{}-{}", peripheral_id, self.connections);
        entry.state = PeripheralState::Connected;
        entry.session_id = Some(session_id);
        Ok(ReconnectPlan {
            attempt: entry.reconnects,
            backoff_ms,
        })
    }
}

pub const MAX_CHARACTERISTICS: usize = 64;
pub const MAX_VALUE_BYTES: usize = 512;
pub const MAX_GATT_TIMEOUT_MS: u32 = 60_000;
pub const MAX_PENDING_COMMANDS: usize = 256;

/// Operations a declared characteristic permits. A read-only characteristic
/// never accepts writes and vice versa; the policy is profile-defined and
/// fail-closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GattAccess {
    Read,
    Write,
    ReadWrite,
}

impl GattAccess {
    pub fn permits_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub fn permits_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read_write",
        }
    }
}

/// Profile-defined constraints for one declared characteristic: its UUID,
/// permitted operations, and a value bound respected by every write and by
/// every host-supplied observed value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacteristicPolicy {
    uuid: String,
    access: GattAccess,
    max_value_bytes: usize,
}

impl CharacteristicPolicy {
    pub fn new(
        uuid: String,
        access: GattAccess,
        max_value_bytes: usize,
    ) -> Result<Self, BluetoothError> {
        if uuid.is_empty() {
            return Err(BluetoothError::EmptyServiceUuid);
        }
        if uuid.len() > MAX_SERVICE_UUID_BYTES {
            return Err(BluetoothError::ServiceUuidTooLong);
        }
        if max_value_bytes == 0 {
            return Err(BluetoothError::ZeroValueBound);
        }
        if max_value_bytes > MAX_VALUE_BYTES {
            return Err(BluetoothError::ValueTooLarge);
        }
        Ok(Self {
            uuid,
            access,
            max_value_bytes,
        })
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn access(&self) -> GattAccess {
        self.access
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }
}

/// Write acknowledgement modes. An acknowledged write awaits an explicit
/// host-reported acknowledgement; an unacknowledged write resolves at
/// issuance and is never confirmed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    Acknowledged,
    Unacknowledged,
}

impl WriteMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Acknowledged => "acknowledged",
            Self::Unacknowledged => "unacknowledged",
        }
    }
}

/// Receipt of a planned read request. The returned value is supplied by the
/// host through `validate_value`; this session never produces one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadReceipt {
    command_id: String,
    uuid: String,
    max_value_bytes: usize,
}

impl ReadReceipt {
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }
}

/// Report of one issued write. Issuance is transport accounting only: a
/// successful write is never reported as physical execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteReport {
    command_id: String,
    uuid: String,
    mode: WriteMode,
    timeout_ms: u32,
}

impl WriteReport {
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn mode(&self) -> WriteMode {
        self.mode
    }

    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingCommand {
    uuid: String,
    mode: WriteMode,
}

/// Bounded GATT command accounting for declared characteristics. The session
/// plans reads and writes against profile-defined policies; the host drives
/// the actual characteristic operations and reports outcomes back.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GattSession {
    characteristics: BTreeMap<String, CharacteristicPolicy>,
    max_command_timeout_ms: u32,
    pending: BTreeMap<String, PendingCommand>,
    commands: u64,
}

impl GattSession {
    pub fn new(
        characteristics: Vec<CharacteristicPolicy>,
        max_command_timeout_ms: u32,
    ) -> Result<Self, BluetoothError> {
        if characteristics.is_empty() {
            return Err(BluetoothError::NoCharacteristics);
        }
        if characteristics.len() > MAX_CHARACTERISTICS {
            return Err(BluetoothError::TooManyCharacteristics);
        }
        let mut set = BTreeMap::new();
        for policy in characteristics {
            if set.insert(policy.uuid().to_owned(), policy).is_some() {
                return Err(BluetoothError::DuplicateCharacteristic);
            }
        }
        if max_command_timeout_ms == 0 {
            return Err(BluetoothError::ZeroCommandTimeout);
        }
        if max_command_timeout_ms > MAX_GATT_TIMEOUT_MS {
            return Err(BluetoothError::CommandTimeoutTooLarge);
        }
        Ok(Self {
            characteristics: set,
            max_command_timeout_ms,
            pending: BTreeMap::new(),
            commands: 0,
        })
    }

    pub fn max_command_timeout_ms(&self) -> u32 {
        self.max_command_timeout_ms
    }

    pub fn characteristics(&self) -> usize {
        self.characteristics.len()
    }

    fn policy(&self, uuid: &str) -> Result<&CharacteristicPolicy, BluetoothError> {
        self.characteristics
            .get(uuid)
            .ok_or(BluetoothError::UnknownSubscription)
    }

    /// Permitted operations of one declared characteristic.
    pub fn access(&self, uuid: &str) -> Result<GattAccess, BluetoothError> {
        Ok(self.policy(uuid)?.access)
    }

    /// Value bound of one declared characteristic.
    pub fn max_value_bytes(&self, uuid: &str) -> Result<usize, BluetoothError> {
        Ok(self.policy(uuid)?.max_value_bytes)
    }

    fn next_command_id(&mut self) -> String {
        self.commands += 1;
        format!("cmd-{}", self.commands)
    }

    /// Plan a read of one declared characteristic. The policy must permit
    /// reads; the observed value arrives from the host and is checked by
    /// `validate_value`.
    pub fn read(&mut self, uuid: &str) -> Result<ReadReceipt, BluetoothError> {
        let (owned_uuid, max_value_bytes) = {
            let policy = self.policy(uuid)?;
            if !policy.access.permits_read() {
                return Err(BluetoothError::UnsupportedOperation);
            }
            (policy.uuid().to_owned(), policy.max_value_bytes)
        };
        Ok(ReadReceipt {
            command_id: self.next_command_id(),
            uuid: owned_uuid,
            max_value_bytes,
        })
    }

    /// Issue a write of one declared characteristic under the requested mode.
    /// Acknowledged writes enter the bounded pending ledger until the host
    /// reports the acknowledgement or the command is cancelled; an
    /// unacknowledged write resolves at issuance and is never confirmed.
    pub fn write(
        &mut self,
        uuid: &str,
        mode: WriteMode,
        value: &[u8],
    ) -> Result<WriteReport, BluetoothError> {
        let (owned_uuid, max_value_bytes) = {
            let policy = self.policy(uuid)?;
            if !policy.access.permits_write() {
                return Err(BluetoothError::UnsupportedOperation);
            }
            (policy.uuid().to_owned(), policy.max_value_bytes)
        };
        if value.len() > max_value_bytes {
            return Err(BluetoothError::ValueTooLarge);
        }
        if mode == WriteMode::Acknowledged && self.pending.len() == MAX_PENDING_COMMANDS {
            return Err(BluetoothError::CommandBoundReached);
        }
        let command_id = self.next_command_id();
        if mode == WriteMode::Acknowledged {
            self.pending.insert(
                command_id.clone(),
                PendingCommand {
                    uuid: owned_uuid.clone(),
                    mode,
                },
            );
        }
        Ok(WriteReport {
            command_id,
            uuid: owned_uuid,
            mode,
            timeout_ms: self.max_command_timeout_ms,
        })
    }

    /// Record the host-reported acknowledgement of one pending command.
    pub fn acknowledge(&mut self, command_id: &str) -> Result<(), BluetoothError> {
        self.pending
            .remove(command_id)
            .map(|_| ())
            .ok_or(BluetoothError::UnknownCommand)
    }

    /// Cancel one pending command. A resolved command is no longer pending
    /// and cancels with `UnknownCommand`.
    pub fn cancel(&mut self, command_id: &str) -> Result<(), BluetoothError> {
        self.acknowledge(command_id)
    }

    /// Number of commands awaiting acknowledgement.
    pub fn pending_acknowledgements(&self) -> usize {
        self.pending.len()
    }

    /// Validate a host-supplied observed value against a characteristic's
    /// profile-defined bound (malformed-payload mitigation, fail closed).
    pub fn validate_value(&self, uuid: &str, value: &[u8]) -> Result<(), BluetoothError> {
        let policy = self.policy(uuid)?;
        if value.len() > policy.max_value_bytes {
            return Err(BluetoothError::ValueTooLarge);
        }
        Ok(())
    }
}

/// Decode a little-endian `int16` characteristic value, the encoding of the
/// directive's illustrative environmental temperature characteristic.
/// Scaling to engineering units is application-level.
pub fn decode_i16_le(payload: &[u8]) -> Result<i16, BluetoothError> {
    let bytes: [u8; 2] = payload
        .try_into()
        .map_err(|_| BluetoothError::MalformedPayload)?;
    Ok(i16::from_le_bytes(bytes))
}

pub const MAX_STREAM_CHARACTERISTICS: usize = 16;
pub const MAX_NOTIFICATION_BYTES: usize = 512;
pub const MAX_BUFFERED_NOTIFICATIONS: u16 = 256;

/// Transport of one declared characteristic subscription. Indications are
/// acknowledged at the attribute layer; notifications are not, so the session
/// never claims delivery for them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationTransport {
    Notification,
    Indication,
}

impl NotificationTransport {
    pub fn label(self) -> &'static str {
        match self {
            Self::Notification => "notification",
            Self::Indication => "indication",
        }
    }
}

/// One declared subscribable characteristic and its fixed transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamCharacteristic {
    uuid: String,
    transport: NotificationTransport,
}

impl StreamCharacteristic {
    pub fn new(uuid: String, transport: NotificationTransport) -> Result<Self, BluetoothError> {
        if uuid.is_empty() {
            return Err(BluetoothError::EmptyServiceUuid);
        }
        if uuid.len() > MAX_SERVICE_UUID_BYTES {
            return Err(BluetoothError::ServiceUuidTooLong);
        }
        Ok(Self { uuid, transport })
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn transport(&self) -> NotificationTransport {
        self.transport
    }
}

/// Result of one accepted notification intake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiveOutcome {
    ordinal: u64,
    buffered: usize,
}

impl ReceiveOutcome {
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub fn buffered(&self) -> usize {
        self.buffered
    }
}

/// One buffered notification in FIFO intake order. The payload is exactly as
/// reported by the host; typed decoding is applied on take.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferedNotification {
    ordinal: u64,
    uuid: String,
    payload: Vec<u8>,
}

impl BufferedNotification {
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SubscriptionEntry {
    transport: NotificationTransport,
    active: bool,
    received: u64,
    dropped: u64,
}

/// Bounded characteristic-notification intake for declared characteristics
/// with fixed transports. The buffer is bounded and applies fail-closed
/// backpressure: an over-full buffer rejects the notification and counts the
/// drop instead of evicting or growing unbounded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamSession {
    subscriptions: BTreeMap<String, SubscriptionEntry>,
    buffer: VecDeque<BufferedNotification>,
    pending_confirmations: BTreeMap<u64, String>,
    max_notification_bytes: usize,
    max_buffered_notifications: u16,
    received_total: u64,
    dropped_total: u64,
    cancelled_total: u64,
    ordinals: u64,
}

impl StreamSession {
    pub fn new(
        characteristics: Vec<StreamCharacteristic>,
        max_notification_bytes: usize,
        max_buffered_notifications: u16,
    ) -> Result<Self, BluetoothError> {
        if characteristics.is_empty() {
            return Err(BluetoothError::NoCharacteristics);
        }
        if characteristics.len() > MAX_STREAM_CHARACTERISTICS {
            return Err(BluetoothError::TooManyCharacteristics);
        }
        let mut subscriptions = BTreeMap::new();
        for characteristic in characteristics {
            let entry = SubscriptionEntry {
                transport: characteristic.transport,
                active: false,
                received: 0,
                dropped: 0,
            };
            if subscriptions
                .insert(characteristic.uuid().to_owned(), entry)
                .is_some()
            {
                return Err(BluetoothError::DuplicateCharacteristic);
            }
        }
        if max_notification_bytes == 0 {
            return Err(BluetoothError::ZeroNotificationBytes);
        }
        if max_notification_bytes > MAX_NOTIFICATION_BYTES {
            return Err(BluetoothError::NotificationBytesTooLarge);
        }
        if max_buffered_notifications == 0 {
            return Err(BluetoothError::ZeroBufferedNotifications);
        }
        if max_buffered_notifications > MAX_BUFFERED_NOTIFICATIONS {
            return Err(BluetoothError::BufferedNotificationsTooLarge);
        }
        Ok(Self {
            subscriptions,
            buffer: VecDeque::new(),
            pending_confirmations: BTreeMap::new(),
            max_notification_bytes,
            max_buffered_notifications,
            received_total: 0,
            dropped_total: 0,
            cancelled_total: 0,
            ordinals: 0,
        })
    }

    pub fn max_notification_bytes(&self) -> usize {
        self.max_notification_bytes
    }

    pub fn max_buffered_notifications(&self) -> u16 {
        self.max_buffered_notifications
    }

    pub fn characteristics(&self) -> usize {
        self.subscriptions.len()
    }

    fn subscription(&self, uuid: &str) -> Result<&SubscriptionEntry, BluetoothError> {
        self.subscriptions
            .get(uuid)
            .ok_or(BluetoothError::UnknownSubscription)
    }

    /// The declared transport of one characteristic.
    pub fn transport(&self, uuid: &str) -> Result<NotificationTransport, BluetoothError> {
        Ok(self.subscription(uuid)?.transport)
    }

    /// Activate the declared subscription of one characteristic. The
    /// transport is fixed by the declaration; a host must still verify the
    /// characteristic actually supports it before enabling a real one.
    pub fn subscribe(&mut self, uuid: &str) -> Result<(), BluetoothError> {
        let entry = self.subscriptions.get_mut(uuid);
        let entry = entry.ok_or(BluetoothError::UnknownSubscription)?;
        if entry.active {
            return Err(BluetoothError::WrongState);
        }
        entry.active = true;
        Ok(())
    }

    /// Accept one host-reported notification payload for an active
    /// subscription. Over-limit payloads fail closed; a full buffer rejects
    /// the notification with [`BluetoothError::BufferExhausted`] and counts
    /// the drop.
    pub fn receive(
        &mut self,
        uuid: &str,
        payload: &[u8],
    ) -> Result<ReceiveOutcome, BluetoothError> {
        let entry = self.subscriptions.get_mut(uuid);
        let entry = entry.ok_or(BluetoothError::UnknownSubscription)?;
        if !entry.active {
            return Err(BluetoothError::WrongState);
        }
        if payload.len() > self.max_notification_bytes {
            return Err(BluetoothError::PayloadTooLarge);
        }
        if self.buffer.len() == usize::from(self.max_buffered_notifications) {
            entry.dropped += 1;
            self.dropped_total += 1;
            return Err(BluetoothError::BufferExhausted);
        }
        self.ordinals += 1;
        let ordinal = self.ordinals;
        self.buffer.push_back(BufferedNotification {
            ordinal,
            uuid: uuid.to_owned(),
            payload: payload.to_vec(),
        });
        if entry.transport == NotificationTransport::Indication {
            self.pending_confirmations
                .insert(ordinal, uuid.to_owned());
        }
        entry.received += 1;
        self.received_total += 1;
        Ok(ReceiveOutcome {
            ordinal,
            buffered: self.buffer.len(),
        })
    }

    /// Take the next buffered notification in intake order.
    pub fn take(&mut self) -> Option<BufferedNotification> {
        self.buffer.pop_front()
    }

    /// Record the attribute-layer confirmation of one indication. Only
    /// indication transports create pending confirmations; notifications
    /// never do.
    pub fn confirm(&mut self, ordinal: u64) -> Result<(), BluetoothError> {
        self.pending_confirmations
            .remove(&ordinal)
            .map(|_| ())
            .ok_or(BluetoothError::UnknownOrdinal)
    }

    /// Number of indications awaiting confirmation.
    pub fn pending_confirmations(&self) -> usize {
        self.pending_confirmations.len()
    }

    /// Deactivate one subscription and deterministically discard its
    /// buffered notifications and pending confirmations.
    pub fn cancel(&mut self, uuid: &str) -> Result<(), BluetoothError> {
        let entry = self.subscriptions.get_mut(uuid);
        let entry = entry.ok_or(BluetoothError::UnknownSubscription)?;
        if !entry.active {
            return Err(BluetoothError::WrongState);
        }
        entry.active = false;
        self.buffer.retain(|notification| {
            if notification.uuid == uuid {
                self.pending_confirmations.remove(&notification.ordinal);
                false
            } else {
                true
            }
        });
        self.cancelled_total += 1;
        Ok(())
    }

    /// Total notifications accepted across all subscriptions.
    pub fn received(&self) -> u64 {
        self.received_total
    }

    /// Total notifications rejected because the buffer was full.
    pub fn dropped(&self) -> u64 {
        self.dropped_total
    }

    /// Total notifications discarded by subscription cancellation.
    pub fn cancelled(&self) -> u64 {
        self.cancelled_total
    }

    /// Notifications currently buffered.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Notifications accepted for one subscription, cumulative across
    /// re-subscriptions.
    pub fn received_from(&self, uuid: &str) -> Result<u64, BluetoothError> {
        Ok(self.subscription(uuid)?.received)
    }

    /// Notifications dropped for one subscription.
    pub fn dropped_from(&self, uuid: &str) -> Result<u64, BluetoothError> {
        Ok(self.subscription(uuid)?.dropped)
    }
}

pub const MAX_AUTHORISED_PERIPHERALS: u16 = 16;
pub const MAX_USAGE_DESCRIPTION_BYTES: usize = 256;
pub const MAX_ADVERTISEMENT_BYTES: usize = 31;
pub const MAX_ASSOCIATIONS: u16 = 16;
pub const MAX_SUBSCRIPTIONS_PER_ASSOCIATION: u16 = 8;

/// Application-level Bluetooth permission states, mirroring the states a
/// host can report for the app's Bluetooth authorisation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionState {
    NotDetermined,
    Denied,
    Granted,
}

impl fmt::Display for PermissionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotDetermined => "not_determined",
            Self::Denied => "denied",
            Self::Granted => "granted",
        })
    }
}

/// Placeholders that fail Blue Moon Guard's least-privilege check. Compared
/// against the trimmed, lowercased description; the directive forbids empty
/// or generic explanations in production builds.
pub const GENERIC_USAGE_DESCRIPTIONS: &[&str] = &[
    "",
    "bluetooth",
    "bluetooth usage",
    "uses bluetooth",
    "app uses bluetooth",
    "this app uses bluetooth",
    "bluetooth permission",
    "permission required",
    "required",
    "needed",
    "n/a",
    "todo",
    "tbd",
    "placeholder",
];

/// Validate an application-specific privacy usage description. Fails closed
/// on empties, generic placeholders, punctuation-only text and over-bound
/// descriptions.
pub fn validate_usage_description(description: &str) -> Result<(), BluetoothError> {
    if description.len() > MAX_USAGE_DESCRIPTION_BYTES {
        return Err(BluetoothError::UsageDescriptionTooLong);
    }
    let trimmed = description.trim();
    if GENERIC_USAGE_DESCRIPTIONS.contains(&trimmed.to_ascii_lowercase().as_str()) {
        return Err(BluetoothError::GenericUsageDescription);
    }
    if !trimmed.bytes().any(|byte| byte.is_ascii_alphanumeric()) {
        return Err(BluetoothError::GenericUsageDescription);
    }
    Ok(())
}

/// One granted connection decision. Explicit application selection and a
/// granted permission are both required; neither advertised names nor
/// service UUIDs ever constitute identity proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionDecision {
    peripheral_id: String,
    explicit_selection: bool,
}

impl ConnectionDecision {
    pub fn peripheral_id(&self) -> &str {
        &self.peripheral_id
    }

    pub fn explicit_selection(&self) -> bool {
        self.explicit_selection
    }
}

/// Bounded application-level Bluetooth authorisation: the mandatory security
/// contract of the Blue Moon family. Permissions are inspected per decision,
/// accessories are authorised only through explicit selection, and every
/// decision and advertisement-payload validation is counted for audit.
/// Guard records policy decisions; it never verifies cryptographic device
/// identity and never grants anything the host did not report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardSession {
    max_authorised_peripherals: u16,
    authorised: BTreeSet<String>,
    decisions_granted: u64,
    decisions_denied: u64,
    advertisements_validated: u64,
    advertisements_rejected: u64,
}

impl GuardSession {
    pub fn new(max_authorised_peripherals: u16) -> Result<Self, BluetoothError> {
        if max_authorised_peripherals == 0 {
            return Err(BluetoothError::ZeroAuthorisationBound);
        }
        if max_authorised_peripherals > MAX_AUTHORISED_PERIPHERALS {
            return Err(BluetoothError::AuthorisationBoundTooLarge);
        }
        Ok(Self {
            max_authorised_peripherals,
            authorised: BTreeSet::new(),
            decisions_granted: 0,
            decisions_denied: 0,
            advertisements_validated: 0,
            advertisements_rejected: 0,
        })
    }

    pub fn max_authorised_peripherals(&self) -> u16 {
        self.max_authorised_peripherals
    }

    /// Explicitly authorise one peripheral. Re-selection is idempotent; the
    /// authorised set is bounded and never grows past the negotiated bound.
    pub fn authorise(&mut self, peripheral_id: &str) -> Result<(), BluetoothError> {
        CentralSession::validate_peripheral_id(peripheral_id)?;
        if self.authorised.contains(peripheral_id) {
            return Ok(());
        }
        if self.authorised.len() == usize::from(self.max_authorised_peripherals) {
            return Err(BluetoothError::AuthorisationBoundReached);
        }
        self.authorised.insert(peripheral_id.to_owned());
        Ok(())
    }

    /// Revoke one explicit authorisation.
    pub fn revoke(&mut self, peripheral_id: &str) -> Result<(), BluetoothError> {
        CentralSession::validate_peripheral_id(peripheral_id)?;
        if self.authorised.remove(peripheral_id) {
            Ok(())
        } else {
            Err(BluetoothError::UnknownPeripheral)
        }
    }

    /// Whether one peripheral is currently explicitly authorised.
    pub fn is_authorised(&self, peripheral_id: &str) -> bool {
        self.authorised.contains(peripheral_id)
    }

    /// Number of explicitly authorised peripherals.
    pub fn authorised_count(&self) -> usize {
        self.authorised.len()
    }

    /// Decide one connection attempt. Fails closed on an undetermined or
    /// denied permission, and on any peripheral the application did not
    /// explicitly select. Every decision is counted for audit, granted or
    /// denied.
    pub fn check_connection(
        &mut self,
        peripheral_id: &str,
        permission: PermissionState,
    ) -> Result<ConnectionDecision, BluetoothError> {
        CentralSession::validate_peripheral_id(peripheral_id)?;
        let decision = match permission {
            PermissionState::Granted => {
                if !self.authorised.contains(peripheral_id) {
                    self.decisions_denied += 1;
                    return Err(BluetoothError::NotAuthorised);
                }
                ConnectionDecision {
                    peripheral_id: peripheral_id.to_owned(),
                    explicit_selection: true,
                }
            }
            PermissionState::NotDetermined => {
                self.decisions_denied += 1;
                return Err(BluetoothError::PermissionUndetermined);
            }
            PermissionState::Denied => {
                self.decisions_denied += 1;
                return Err(BluetoothError::PermissionDenied);
            }
        };
        self.decisions_granted += 1;
        Ok(decision)
    }

    /// Validate one advertisement payload against the legacy BLE
    /// advertisement bound (bounded-parsing mitigation). Accepted and
    /// rejected payloads are both counted.
    pub fn validate_advertisement_payload(&mut self, payload: &[u8]) -> Result<(), BluetoothError> {
        if payload.len() > MAX_ADVERTISEMENT_BYTES {
            self.advertisements_rejected += 1;
            return Err(BluetoothError::AdvertisementTooLarge);
        }
        self.advertisements_validated += 1;
        Ok(())
    }

    /// Connections granted, cumulative.
    pub fn decisions_granted(&self) -> u64 {
        self.decisions_granted
    }

    /// Connection decisions denied, cumulative, by any fail-closed reason.
    pub fn decisions_denied(&self) -> u64 {
        self.decisions_denied
    }

    /// Advertisement payloads accepted within the bound.
    pub fn advertisements_validated(&self) -> u64 {
        self.advertisements_validated
    }

    /// Advertisement payloads rejected over the bound.
    pub fn advertisements_rejected(&self) -> u64 {
        self.advertisements_rejected
    }
}

/// Restoration intent recorded for one associated peripheral: whether the
/// application wants an automatic reconnection or an on-demand one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssociationIntent {
    AutoReconnect,
    OnDemand,
}

impl fmt::Display for AssociationIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AutoReconnect => "auto_reconnect",
            Self::OnDemand => "on_demand",
        })
    }
}

/// Why an association is currently suspended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspensionReason {
    BluetoothDisabled,
    AppSuspended,
}

impl fmt::Display for SuspensionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BluetoothDisabled => "bluetooth_disabled",
            Self::AppSuspended => "app_suspended",
        })
    }
}

/// Lifecycle state of one persisted association.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssociationState {
    Active,
    Suspended(SuspensionReason),
}

impl fmt::Display for AssociationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Suspended(reason) => write!(f, "suspended({reason})"),
        }
    }
}

/// Result of reconciling one association with an observed device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reconciliation {
    /// The persisted intent was restored to active.
    Restored,
    /// No change: already active, or the device was not observed.
    Unchanged,
}

/// The application-level restoration plan for one associated peripheral.
/// Planning restoration is not an outcome: no relaunch, no reconnection and
/// no restoration success is promised by this model. Apple's state
/// restoration is conditional, and the ledger models that restriction
/// explicitly rather than claiming indefinite execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestorationIntent {
    peripheral_id: String,
    intent: AssociationIntent,
    subscriptions: Vec<String>,
}

impl RestorationIntent {
    pub fn peripheral_id(&self) -> &str {
        &self.peripheral_id
    }

    pub fn intent(&self) -> AssociationIntent {
        self.intent
    }

    pub fn subscriptions(&self) -> &[String] {
        &self.subscriptions
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Association {
    intent: AssociationIntent,
    subscriptions: Vec<String>,
    state: AssociationState,
    revision: u64,
}

/// Bounded persisted-association ledger: remembers authorised peripheral
/// identifiers, user-approved associations with their restoration intent and
/// compatible subscriptions, tracks suspension (Bluetooth disabled, app
/// suspended) and reconciles persisted intent with observed devices. The
/// ledger records intent; it never promises relaunch or restoration success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuityLedger {
    associations: BTreeMap<String, Association>,
    max_associations: u16,
    max_subscriptions: u16,
    restorations_planned: u64,
    reconciliations: u64,
}

impl ContinuityLedger {
    pub fn new(
        max_associations: u16,
        max_subscriptions: u16,
    ) -> Result<Self, BluetoothError> {
        if max_associations == 0 {
            return Err(BluetoothError::ZeroAssociationBound);
        }
        if max_associations > MAX_ASSOCIATIONS {
            return Err(BluetoothError::AssociationBoundTooLarge);
        }
        if max_subscriptions > MAX_SUBSCRIPTIONS_PER_ASSOCIATION {
            return Err(BluetoothError::SubscriptionBoundTooLarge);
        }
        Ok(Self {
            associations: BTreeMap::new(),
            max_associations,
            max_subscriptions,
            restorations_planned: 0,
            reconciliations: 0,
        })
    }

    pub fn max_associations(&self) -> u16 {
        self.max_associations
    }

    pub fn max_subscriptions(&self) -> u16 {
        self.max_subscriptions
    }

    /// Number of persisted associations.
    pub fn len(&self) -> usize {
        self.associations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.associations.is_empty()
    }

    /// Record or update one user-approved association with its restoration
    /// intent and compatible subscriptions. Updates keep the lifecycle state
    /// and bump the revision.
    pub fn record(
        &mut self,
        peripheral_id: &str,
        intent: AssociationIntent,
        subscriptions: &[String],
    ) -> Result<(), BluetoothError> {
        CentralSession::validate_peripheral_id(peripheral_id)?;
        if subscriptions.len() > usize::from(self.max_subscriptions) {
            return Err(BluetoothError::TooManySubscriptions);
        }
        for subscription in subscriptions {
            if subscription.is_empty() {
                return Err(BluetoothError::EmptyServiceUuid);
            }
            if subscription.len() > MAX_SERVICE_UUID_BYTES {
                return Err(BluetoothError::ServiceUuidTooLong);
            }
        }
        if let Some(association) = self.associations.get_mut(peripheral_id) {
            association.intent = intent;
            association.subscriptions = subscriptions.to_vec();
            association.revision += 1;
            return Ok(());
        }
        if self.associations.len() == usize::from(self.max_associations) {
            return Err(BluetoothError::AssociationBoundReached);
        }
        self.associations.insert(
            peripheral_id.to_owned(),
            Association {
                intent,
                subscriptions: subscriptions.to_vec(),
                state: AssociationState::Active,
                revision: 1,
            },
        );
        Ok(())
    }

    pub fn association_state(&self, peripheral_id: &str) -> Result<AssociationState, BluetoothError> {
        Ok(self
            .associations
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?
            .state)
    }

    pub fn association_intent(&self, peripheral_id: &str) -> Result<AssociationIntent, BluetoothError> {
        Ok(self
            .associations
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?
            .intent)
    }

    pub fn subscriptions(&self, peripheral_id: &str) -> Result<&[String], BluetoothError> {
        Ok(self
            .associations
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?
            .subscriptions
            .as_slice())
    }

    /// Persisted revision, bumped on every recorded update and successful
    /// reconciliation.
    pub fn revision(&self, peripheral_id: &str) -> Result<u64, BluetoothError> {
        Ok(self
            .associations
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?
            .revision)
    }

    /// Observe the host Bluetooth adapter being disabled: every active
    /// association suspends with `BluetoothDisabled`; already suspended
    /// associations keep their first reason.
    pub fn observe_bluetooth_disabled(&mut self) {
        for association in self.associations.values_mut() {
            if association.state == AssociationState::Active {
                association.state = AssociationState::Suspended(SuspensionReason::BluetoothDisabled);
            }
        }
    }

    /// Observe the application being suspended: every active association
    /// suspends with `AppSuspended`; already suspended associations keep
    /// their first reason.
    pub fn observe_app_suspended(&mut self) {
        for association in self.associations.values_mut() {
            if association.state == AssociationState::Active {
                association.state = AssociationState::Suspended(SuspensionReason::AppSuspended);
            }
        }
    }

    /// Plan the restoration of one persisted association's connection intent
    /// and compatible subscriptions. Planning is accounting only: no
    /// reconnection, relaunch or restoration success is claimed.
    pub fn restore_intent(
        &mut self,
        peripheral_id: &str,
    ) -> Result<RestorationIntent, BluetoothError> {
        let association = self
            .associations
            .get(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?;
        let plan = RestorationIntent {
            peripheral_id: peripheral_id.to_owned(),
            intent: association.intent,
            subscriptions: association.subscriptions.clone(),
        };
        self.restorations_planned += 1;
        Ok(plan)
    }

    /// Reconcile one association with an observed device. Only an observed
    /// device restores a suspended association to active; an unobserved one
    /// keeps its suspension.
    pub fn reconcile(
        &mut self,
        peripheral_id: &str,
        observed: bool,
    ) -> Result<Reconciliation, BluetoothError> {
        let association = self
            .associations
            .get_mut(peripheral_id)
            .ok_or(BluetoothError::UnknownPeripheral)?;
        if association.state == AssociationState::Active || !observed {
            return Ok(Reconciliation::Unchanged);
        }
        association.state = AssociationState::Active;
        association.revision += 1;
        self.reconciliations += 1;
        Ok(Reconciliation::Restored)
    }

    /// Forget one persisted association entirely.
    pub fn forget(&mut self, peripheral_id: &str) -> Result<(), BluetoothError> {
        self.associations
            .remove(peripheral_id)
            .map(|_| ())
            .ok_or(BluetoothError::UnknownPeripheral)
    }

    /// Restoration plans produced, cumulative.
    pub fn restorations_planned(&self) -> u64 {
        self.restorations_planned
    }

    /// Reconciliations that restored a suspended association, cumulative.
    pub fn reconciliations(&self) -> u64 {
        self.reconciliations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> CentralSession {
        CentralSession::new(
            vec!["181A".to_owned(), "180F".to_owned()],
            10_000,
            4,
            10_000,
            3,
        )
        .unwrap()
    }

    #[test]
    fn session_validates_every_declaration_bound() {
        assert_eq!(
            CentralSession::new(Vec::new(), 10_000, 4, 10_000, 3),
            Err(BluetoothError::NoServiceUuids)
        );
        assert_eq!(
            CentralSession::new(vec![String::new()], 10_000, 4, 10_000, 3),
            Err(BluetoothError::EmptyServiceUuid)
        );
        assert_eq!(
            CentralSession::new(vec!["u".repeat(MAX_SERVICE_UUID_BYTES + 1)], 10_000, 4, 10_000, 3),
            Err(BluetoothError::ServiceUuidTooLong)
        );
        assert_eq!(
            CentralSession::new(vec!["a".to_owned(); MAX_SERVICE_UUIDS + 1], 10_000, 4, 10_000, 3),
            Err(BluetoothError::TooManyServiceUuids)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 0, 4, 10_000, 3),
            Err(BluetoothError::ZeroScanBound)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], MAX_SCAN_MS + 1, 4, 10_000, 3),
            Err(BluetoothError::ScanBoundTooLarge)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 10_000, 0, 10_000, 3),
            Err(BluetoothError::ZeroPeripheralBound)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 10_000, MAX_PERIPHERALS + 1, 10_000, 3),
            Err(BluetoothError::PeripheralBoundTooLarge)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 10_000, 4, 0, 3),
            Err(BluetoothError::ZeroConnectionTimeout)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 10_000, 4, MAX_CONNECTION_TIMEOUT_MS + 1, 3),
            Err(BluetoothError::ConnectionTimeoutTooLarge)
        );
        assert_eq!(
            CentralSession::new(vec!["181A".to_owned()], 10_000, 4, 10_000, MAX_RECONNECT_ATTEMPTS + 1),
            Err(BluetoothError::ReconnectBoundTooLarge)
        );
    }

    #[test]
    fn advertisements_are_classified_and_duplicates_suppressed() {
        let mut session = session();
        assert_eq!(
            session.report_advertisement("AA:BB", -59, &["181A"]),
            Ok(ScanOutcome::Recognised)
        );
        assert_eq!(
            session.report_advertisement("CC:DD", -71, &["FF00"]),
            Ok(ScanOutcome::Discovered)
        );
        assert_eq!(
            session.report_advertisement("AA:BB", -62, &["181A"]),
            Ok(ScanOutcome::Duplicate)
        );
        assert_eq!(session.len(), 2);
        assert_eq!(
            session.peripheral_state("AA:BB"),
            Ok(PeripheralState::Recognised)
        );
        assert_eq!(
            session.peripheral_state("CC:DD"),
            Ok(PeripheralState::Detected)
        );
        assert_eq!(session.rssi("AA:BB"), Ok(-62));
    }

    #[test]
    fn a_late_matching_advertisement_promotes_detection() {
        let mut session = session();
        assert_eq!(
            session.report_advertisement("AA:BB", -59, &["FF00"]),
            Ok(ScanOutcome::Discovered)
        );
        assert_eq!(
            session.report_advertisement("AA:BB", -58, &["180F"]),
            Ok(ScanOutcome::Recognised)
        );
        assert_eq!(
            session.peripheral_state("AA:BB"),
            Ok(PeripheralState::Recognised)
        );
        // A second match is a duplicate, not a new recognition.
        assert_eq!(
            session.report_advertisement("AA:BB", -57, &["180F"]),
            Ok(ScanOutcome::Duplicate)
        );
    }

    #[test]
    fn the_peripheral_set_is_bounded() {
        let mut session = CentralSession::new(
            vec!["181A".to_owned()],
            10_000,
            2,
            10_000,
            0,
        )
        .unwrap();
        session.report_advertisement("AA:BB", -59, &[]).unwrap();
        session.report_advertisement("CC:DD", -60, &[]).unwrap();
        assert_eq!(
            session.report_advertisement("EE:FF", -61, &[]),
            Err(BluetoothError::PeripheralBoundReached)
        );
        assert_eq!(session.len(), 2);
    }

    #[test]
    fn reports_reject_unknown_identifiers_and_invalid_rssi() {
        let mut session = session();
        assert_eq!(
            session.report_advertisement("", -59, &[]),
            Err(BluetoothError::EmptyPeripheralId)
        );
        assert_eq!(
            session.report_advertisement(&"p".repeat(MAX_PERIPHERAL_ID_BYTES + 1), -59, &[]),
            Err(BluetoothError::PeripheralIdTooLong)
        );
        assert_eq!(
            session.report_advertisement("AA:BB", 1, &[]),
            Err(BluetoothError::InvalidRssi)
        );
        assert_eq!(
            session.report_advertisement("AA:BB", -128, &[]),
            Err(BluetoothError::InvalidRssi)
        );
        assert_eq!(
            session.peripheral_state("AA:BB"),
            Err(BluetoothError::UnknownPeripheral)
        );
    }

    #[test]
    fn connection_lifecycle_keeps_states_distinct() {
        let mut session = session();
        session
            .report_advertisement("AA:BB", -59, &["181A"])
            .unwrap();
        // A detected peripheral is not connectable.
        session
            .report_advertisement("CC:DD", -70, &["FF00"])
            .unwrap();
        assert_eq!(session.connect("CC:DD"), Err(BluetoothError::WrongState));
        assert_eq!(session.connect("ZZ:ZZ"), Err(BluetoothError::UnknownPeripheral));
        assert_eq!(session.mark_operational("AA:BB"), Err(BluetoothError::WrongState));
        assert_eq!(session.observe_disconnect("AA:BB"), Err(BluetoothError::WrongState));

        let first = session.connect("AA:BB").unwrap();
        assert_eq!(first, "conn-AA:BB-1");
        assert_eq!(
            session.session_id("AA:BB"),
            Ok(Some("conn-AA:BB-1".to_owned()))
        );
        assert_eq!(session.connect("AA:BB"), Err(BluetoothError::WrongState));

        session.mark_operational("AA:BB").unwrap();
        assert_eq!(
            session.peripheral_state("AA:BB"),
            Ok(PeripheralState::Operational)
        );
        assert_eq!(session.mark_operational("AA:BB"), Err(BluetoothError::WrongState));

        session.observe_disconnect("AA:BB").unwrap();
        assert_eq!(
            session.peripheral_state("AA:BB"),
            Ok(PeripheralState::Disconnected)
        );
        assert_eq!(session.session_id("AA:BB"), Ok(None));
        assert_eq!(session.connect("AA:BB"), Err(BluetoothError::WrongState));
    }

    #[test]
    fn reconnection_is_bounded_with_doubling_backoff() {
        let mut session = session();
        session
            .report_advertisement("AA:BB", -59, &["181A"])
            .unwrap();
        assert_eq!(session.reconnect("AA:BB"), Err(BluetoothError::WrongState));
        session.connect("AA:BB").unwrap();
        session.observe_disconnect("AA:BB").unwrap();
        assert_eq!(
            session.reconnect("AA:BB"),
            Ok(ReconnectPlan {
                attempt: 1,
                backoff_ms: 100
            })
        );
        session.observe_disconnect("AA:BB").unwrap();
        assert_eq!(
            session.reconnect("AA:BB"),
            Ok(ReconnectPlan {
                attempt: 2,
                backoff_ms: 200
            })
        );
        session.observe_disconnect("AA:BB").unwrap();
        assert_eq!(
            session.reconnect("AA:BB"),
            Ok(ReconnectPlan {
                attempt: 3,
                backoff_ms: 400
            })
        );
        session.observe_disconnect("AA:BB").unwrap();
        assert_eq!(session.reconnect("AA:BB"), Err(BluetoothError::NoRetryRemaining));
    }

    #[test]
    fn backoff_saturates_at_the_supported_maximum() {
        let mut session = CentralSession::new(
            vec!["181A".to_owned()],
            10_000,
            1,
            10_000,
            MAX_RECONNECT_ATTEMPTS,
        )
        .unwrap();
        session
            .report_advertisement("AA:BB", -59, &["181A"])
            .unwrap();
        session.connect("AA:BB").unwrap();
        for _ in 0..7 {
            session.observe_disconnect("AA:BB").unwrap();
            session.reconnect("AA:BB").unwrap();
        }
        session.observe_disconnect("AA:BB").unwrap();
        let capped = session.reconnect("AA:BB").unwrap();
        assert_eq!(capped.attempt(), 8);
        assert_eq!(capped.backoff_ms(), MAX_RECONNECT_DELAY_MS);
    }

    fn gatt_session() -> GattSession {
        GattSession::new(
            vec![
                CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::ReadWrite, 512).unwrap(),
                CharacteristicPolicy::new("2A37".to_owned(), GattAccess::Write, 64).unwrap(),
            ],
            30_000,
        )
        .unwrap()
    }

    #[test]
    fn gatt_session_validates_every_declaration_bound() {
        assert_eq!(
            GattSession::new(Vec::new(), 30_000),
            Err(BluetoothError::NoCharacteristics)
        );
        assert_eq!(
            GattSession::new(
                (0..=MAX_CHARACTERISTICS)
                    .map(|index| CharacteristicPolicy::new(
                        format!("{index:04}"),
                        GattAccess::Read,
                        16
                    )
                    .unwrap())
                    .collect(),
                30_000
            ),
            Err(BluetoothError::TooManyCharacteristics)
        );
        assert_eq!(
            GattSession::new(
                vec![
                    CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, 16).unwrap(),
                    CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Write, 16).unwrap(),
                ],
                30_000
            ),
            Err(BluetoothError::DuplicateCharacteristic)
        );
        assert_eq!(
            CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, 0),
            Err(BluetoothError::ZeroValueBound)
        );
        assert_eq!(
            CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, MAX_VALUE_BYTES + 1),
            Err(BluetoothError::ValueTooLarge)
        );
        assert_eq!(
            CharacteristicPolicy::new(String::new(), GattAccess::Read, 16),
            Err(BluetoothError::EmptyServiceUuid)
        );
        assert_eq!(
            GattSession::new(
                vec![CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, 16).unwrap()],
                0
            ),
            Err(BluetoothError::ZeroCommandTimeout)
        );
        assert_eq!(
            GattSession::new(
                vec![CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, 16).unwrap()],
                MAX_GATT_TIMEOUT_MS + 1
            ),
            Err(BluetoothError::CommandTimeoutTooLarge)
        );
    }

    #[test]
    fn gatt_policies_are_fail_closed_per_operation() {
        let mut session = gatt_session();
        assert_eq!(session.access("2A6E"), Ok(GattAccess::ReadWrite));
        assert_eq!(session.max_value_bytes("2A6E"), Ok(512));
        assert_eq!(session.characteristics(), 2);
        // Read-only path: a write-only characteristic rejects reads.
        assert_eq!(session.read("2A37"), Err(BluetoothError::UnsupportedOperation));
        assert_eq!(session.read("FFFF"), Err(BluetoothError::UnknownSubscription));
        let receipt = session.read("2A6E").unwrap();
        assert_eq!(receipt.uuid(), "2A6E");
        assert_eq!(receipt.max_value_bytes(), 512);
        assert!(receipt.command_id().starts_with("cmd-"));
        // Write-only path: a read rejects writes.
        assert_eq!(
            session.write("2A6E", WriteMode::Acknowledged, &[9]),
            Ok(WriteReport {
                command_id: "cmd-2".to_owned(),
                uuid: "2A6E".to_owned(),
                mode: WriteMode::Acknowledged,
                timeout_ms: 30_000,
            })
        );
    }

    #[test]
    fn acknowledged_writes_are_ledger_bounded_and_resolved_explicitly() {
        let mut session = GattSession::new(
            vec![CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::ReadWrite, 1).unwrap()],
            30_000,
        )
        .unwrap();
        assert_eq!(session.pending_acknowledgements(), 0);
        let first = session.write("2A6E", WriteMode::Acknowledged, &[1]).unwrap();
        let second = session.write("2A6E", WriteMode::Acknowledged, &[2]).unwrap();
        assert_eq!(session.pending_acknowledgements(), 2);
        // Values over the characteristic bound fail closed.
        assert_eq!(
            session.write("2A6E", WriteMode::Acknowledged, &[1, 2]),
            Err(BluetoothError::ValueTooLarge)
        );
        assert_eq!(session.pending_acknowledgements(), 2);
        session.cancel(first.command_id()).unwrap();
        assert_eq!(session.pending_acknowledgements(), 1);
        assert_eq!(session.cancel(first.command_id()), Err(BluetoothError::UnknownCommand));
        session.acknowledge(second.command_id()).unwrap();
        assert_eq!(session.pending_acknowledgements(), 0);
        assert_eq!(
            session.acknowledge("cmd-missing"),
            Err(BluetoothError::UnknownCommand)
        );
    }

    #[test]
    fn unacknowledged_writes_resolve_at_issuance() {
        let mut session = gatt_session();
        let report = session
            .write("2A37", WriteMode::Unacknowledged, &[7])
            .unwrap();
        assert_eq!(report.mode(), WriteMode::Unacknowledged);
        assert_eq!(session.pending_acknowledgements(), 0);
        assert_eq!(
            session.acknowledge(report.command_id()),
            Err(BluetoothError::UnknownCommand)
        );
        // Validating observed values is independent of the ledger.
        session.validate_value("2A37", &[0u8; 64]).unwrap();
        assert_eq!(
            session.validate_value("2A37", &[0u8; 65]),
            Err(BluetoothError::ValueTooLarge)
        );
        assert_eq!(
            session.validate_value("FFFF", &[0u8; 1]),
            Err(BluetoothError::UnknownSubscription)
        );
    }

    #[test]
    fn decode_i16_le_rejects_malformed_payloads() {
        assert_eq!(decode_i16_le(&[0x22, 0x0B]), Ok(2850));
        assert_eq!(decode_i16_le(&[0x22]), Err(BluetoothError::MalformedPayload));
        assert_eq!(
            decode_i16_le(&[0x22, 0x0B, 0x00]),
            Err(BluetoothError::MalformedPayload)
        );
    }

    fn stream_session() -> StreamSession {
        StreamSession::new(
            vec![
                StreamCharacteristic::new("2A6E".to_owned(), NotificationTransport::Notification)
                    .unwrap(),
                StreamCharacteristic::new("2A37".to_owned(), NotificationTransport::Indication)
                    .unwrap(),
            ],
            256,
            2,
        )
        .unwrap()
    }

    #[test]
    fn stream_session_validates_every_declaration_bound() {
        assert_eq!(
            StreamSession::new(Vec::new(), 256, 2),
            Err(BluetoothError::NoCharacteristics)
        );
        assert_eq!(
            StreamSession::new(
                (0..=MAX_STREAM_CHARACTERISTICS)
                    .map(|index| StreamCharacteristic::new(
                        format!("{index:04}"),
                        NotificationTransport::Notification
                    )
                    .unwrap())
                    .collect(),
                256,
                2
            ),
            Err(BluetoothError::TooManyCharacteristics)
        );
        assert_eq!(
            StreamSession::new(
                vec![
                    StreamCharacteristic::new("2A6E".to_owned(), NotificationTransport::Notification)
                        .unwrap(),
                    StreamCharacteristic::new("2A6E".to_owned(), NotificationTransport::Indication)
                        .unwrap(),
                ],
                256,
                2
            ),
            Err(BluetoothError::DuplicateCharacteristic)
        );
        assert_eq!(
            StreamCharacteristic::new(String::new(), NotificationTransport::Notification),
            Err(BluetoothError::EmptyServiceUuid)
        );
        assert_eq!(
            StreamSession::new(
                vec![StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    NotificationTransport::Notification
                )
                .unwrap()],
                0,
                2
            ),
            Err(BluetoothError::ZeroNotificationBytes)
        );
        assert_eq!(
            StreamSession::new(
                vec![StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    NotificationTransport::Notification
                )
                .unwrap()],
                MAX_NOTIFICATION_BYTES + 1,
                2
            ),
            Err(BluetoothError::NotificationBytesTooLarge)
        );
        assert_eq!(
            StreamSession::new(
                vec![StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    NotificationTransport::Notification
                )
                .unwrap()],
                256,
                0
            ),
            Err(BluetoothError::ZeroBufferedNotifications)
        );
        assert_eq!(
            StreamSession::new(
                vec![StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    NotificationTransport::Notification
                )
                .unwrap()],
                256,
                MAX_BUFFERED_NOTIFICATIONS + 1
            ),
            Err(BluetoothError::BufferedNotificationsTooLarge)
        );
    }

    #[test]
    fn intake_is_bounded_ordered_and_fail_closed_under_backpressure() {
        let mut session = stream_session();
        assert_eq!(session.characteristics(), 2);
        assert_eq!(session.transport("2A6E"), Ok(NotificationTransport::Notification));
        assert_eq!(session.receive("2A6E", &[1]), Err(BluetoothError::WrongState));
        session.subscribe("2A6E").unwrap();
        assert_eq!(session.subscribe("2A6E"), Err(BluetoothError::WrongState));
        assert_eq!(
            session.receive("FFFF", &[1]),
            Err(BluetoothError::UnknownSubscription)
        );
        assert_eq!(
            session.receive("2A6E", &[0u8; 257]),
            Err(BluetoothError::PayloadTooLarge)
        );

        let first = session.receive("2A6E", &[1, 2]).unwrap();
        let second = session.receive("2A6E", &[3, 4]).unwrap();
        assert_eq!(first.ordinal(), 1);
        assert_eq!(second.ordinal(), 2);
        assert_eq!(second.buffered(), 2);
        // The buffer is full: fail closed and count the drop.
        assert_eq!(
            session.receive("2A6E", &[5, 6]),
            Err(BluetoothError::BufferExhausted)
        );
        assert_eq!(session.dropped(), 1);
        assert_eq!(session.dropped_from("2A6E"), Ok(1));
        assert_eq!(session.buffered(), 2);

        // Intake order is preserved.
        let taken = session.take().unwrap();
        assert_eq!(taken.ordinal(), 1);
        assert_eq!(taken.uuid(), "2A6E");
        assert_eq!(taken.payload(), &[1, 2]);
        let taken = session.take().unwrap();
        assert_eq!(taken.ordinal(), 2);
        assert!(session.take().is_none());
        assert_eq!(session.received(), 2);
    }

    #[test]
    fn only_indications_have_confirmations_and_cancellation_is_deterministic() {
        let mut session = stream_session();
        session.subscribe("2A6E").unwrap();
        session.subscribe("2A37").unwrap();
        session.receive("2A6E", &[1]).unwrap();
        let indication = session.receive("2A37", &[2]).unwrap();
        assert_eq!(session.pending_confirmations(), 1);
        // Notifications never create confirmations.
        assert_eq!(
            session.confirm(indication.ordinal() - 1),
            Err(BluetoothError::UnknownOrdinal)
        );
        session.confirm(indication.ordinal()).unwrap();
        assert_eq!(session.pending_confirmations(), 0);
        assert_eq!(session.confirm(indication.ordinal()), Err(BluetoothError::UnknownOrdinal));

        // Drain the buffer so later intakes fit the bound of two.
        assert!(session.take().is_some());
        assert!(session.take().is_some());
        assert!(session.take().is_none());

        // Cancellation discards buffered items and pending confirmations.
        let pending = session.receive("2A37", &[3]).unwrap();
        assert_eq!(session.pending_confirmations(), 1);
        session.cancel("2A37").unwrap();
        assert_eq!(session.cancel("2A37"), Err(BluetoothError::WrongState));
        assert_eq!(session.buffered(), 0);
        assert_eq!(session.pending_confirmations(), 0);
        assert_eq!(session.cancelled(), 1);
        assert_eq!(
            session.confirm(pending.ordinal()),
            Err(BluetoothError::UnknownOrdinal)
        );
        assert_eq!(session.receive("2A37", &[4]), Err(BluetoothError::WrongState));
        // An inactive subscription can be re-subscribed.
        session.subscribe("2A37").unwrap();
        assert!(session.receive("2A37", &[4]).is_ok());
    }

    #[test]
    fn usage_descriptions_reject_empty_generic_and_overbound_text() {
        assert!(validate_usage_description(
            "Connects to your temperature sensor to show readings."
        )
        .is_ok());
        for generic in [
            "",
            "   ",
            "Bluetooth",
            "USES BLUETOOTH",
            "This app uses bluetooth",
            "required",
            "TODO",
            "!!!",
            "---",
        ] {
            assert_eq!(
                validate_usage_description(generic),
                Err(BluetoothError::GenericUsageDescription)
            );
        }
        assert_eq!(
            validate_usage_description(&"d".repeat(MAX_USAGE_DESCRIPTION_BYTES + 1)),
            Err(BluetoothError::UsageDescriptionTooLong)
        );
        assert!(validate_usage_description(&"d".repeat(MAX_USAGE_DESCRIPTION_BYTES)).is_ok());
    }

    #[test]
    fn guard_validates_its_bound_and_ledger() {
        assert_eq!(
            GuardSession::new(0),
            Err(BluetoothError::ZeroAuthorisationBound)
        );
        assert_eq!(
            GuardSession::new(MAX_AUTHORISED_PERIPHERALS + 1),
            Err(BluetoothError::AuthorisationBoundTooLarge)
        );
        let mut guard = GuardSession::new(2).unwrap();
        assert_eq!(guard.max_authorised_peripherals(), 2);
        assert_eq!(
            guard.authorise(""),
            Err(BluetoothError::EmptyPeripheralId)
        );
        assert_eq!(
            guard.authorise(&"p".repeat(MAX_PERIPHERAL_ID_BYTES + 1)),
            Err(BluetoothError::PeripheralIdTooLong)
        );
        guard.authorise("AA:BB").unwrap();
        assert!(guard.is_authorised("AA:BB"));
        assert_eq!(guard.authorised_count(), 1);
        // Re-selection is idempotent and does not consume the bound.
        guard.authorise("AA:BB").unwrap();
        assert_eq!(guard.authorised_count(), 1);
        guard.authorise("CC:DD").unwrap();
        assert_eq!(
            guard.authorise("EE:FF"),
            Err(BluetoothError::AuthorisationBoundReached)
        );
        guard.revoke("CC:DD").unwrap();
        assert!(!guard.is_authorised("CC:DD"));
        assert_eq!(guard.revoke("CC:DD"), Err(BluetoothError::UnknownPeripheral));
    }

    #[test]
    fn connection_decisions_fail_closed_and_are_audited() {
        let mut guard = GuardSession::new(2).unwrap();
        guard.authorise("AA:BB").unwrap();
        let decision = guard
            .check_connection("AA:BB", PermissionState::Granted)
            .unwrap();
        assert_eq!(decision.peripheral_id(), "AA:BB");
        assert!(decision.explicit_selection());
        assert_eq!(guard.decisions_granted(), 1);
        // Undetermined, denied and unselected all fail closed, each audited.
        assert_eq!(
            guard.check_connection("AA:BB", PermissionState::NotDetermined),
            Err(BluetoothError::PermissionUndetermined)
        );
        assert_eq!(
            guard.check_connection("AA:BB", PermissionState::Denied),
            Err(BluetoothError::PermissionDenied)
        );
        assert_eq!(
            guard.check_connection("CC:DD", PermissionState::Granted),
            Err(BluetoothError::NotAuthorised)
        );
        assert_eq!(
            guard.check_connection("", PermissionState::Granted),
            Err(BluetoothError::EmptyPeripheralId)
        );
        assert_eq!(guard.decisions_granted(), 1);
        assert_eq!(guard.decisions_denied(), 3);
    }

    #[test]
    fn advertisement_payloads_are_bounded_and_counted() {
        let mut guard = GuardSession::new(1).unwrap();
        guard.validate_advertisement_payload(&[0u8; MAX_ADVERTISEMENT_BYTES]).unwrap();
        assert_eq!(
            guard.validate_advertisement_payload(&[0u8; MAX_ADVERTISEMENT_BYTES + 1]),
            Err(BluetoothError::AdvertisementTooLarge)
        );
        assert_eq!(guard.advertisements_validated(), 1);
        assert_eq!(guard.advertisements_rejected(), 1);
    }

    #[test]
    fn continuity_validates_its_bounds() {
        assert_eq!(
            ContinuityLedger::new(0, 2),
            Err(BluetoothError::ZeroAssociationBound)
        );
        assert_eq!(
            ContinuityLedger::new(MAX_ASSOCIATIONS + 1, 2),
            Err(BluetoothError::AssociationBoundTooLarge)
        );
        assert_eq!(
            ContinuityLedger::new(2, MAX_SUBSCRIPTIONS_PER_ASSOCIATION + 1),
            Err(BluetoothError::SubscriptionBoundTooLarge)
        );
        let mut ledger = ContinuityLedger::new(2, 2).unwrap();
        assert_eq!(
            ledger.record("AA:BB", AssociationIntent::AutoReconnect, &[String::new()]),
            Err(BluetoothError::EmptyServiceUuid)
        );
        assert_eq!(
            ledger.record(
                "AA:BB",
                AssociationIntent::AutoReconnect,
                &["u".repeat(MAX_SERVICE_UUID_BYTES + 1)]
            ),
            Err(BluetoothError::ServiceUuidTooLong)
        );
        assert_eq!(
            ledger.record(
                "AA:BB",
                AssociationIntent::AutoReconnect,
                &["181A".to_owned(), "180F".to_owned(), "180D".to_owned()]
            ),
            Err(BluetoothError::TooManySubscriptions)
        );
    }

    #[test]
    fn associations_record_suspend_and_reconcile() {
        let mut ledger = ContinuityLedger::new(2, 2).unwrap();
        ledger
            .record("AA:BB", AssociationIntent::AutoReconnect, &["181A".to_owned()])
            .unwrap();
        assert_eq!(
            ledger.association_state("AA:BB"),
            Ok(AssociationState::Active)
        );
        assert_eq!(ledger.association_intent("AA:BB"), Ok(AssociationIntent::AutoReconnect));
        assert_eq!(ledger.subscriptions("AA:BB"), Ok(&["181A".to_owned()][..]));
        assert_eq!(ledger.revision("AA:BB"), Ok(1));
        // Updates keep state and bump the revision.
        ledger
            .record("AA:BB", AssociationIntent::OnDemand, &[])
            .unwrap();
        assert_eq!(ledger.association_intent("AA:BB"), Ok(AssociationIntent::OnDemand));
        assert_eq!(ledger.subscriptions("AA:BB"), Ok(&[][..]));
        assert_eq!(ledger.revision("AA:BB"), Ok(2));

        // Bluetooth disabled suspends active associations only.
        ledger.observe_bluetooth_disabled();
        assert_eq!(
            ledger.association_state("AA:BB"),
            Ok(AssociationState::Suspended(SuspensionReason::BluetoothDisabled))
        );
        ledger.observe_app_suspended();
        assert_eq!(
            ledger.association_state("AA:BB"),
            Ok(AssociationState::Suspended(SuspensionReason::BluetoothDisabled))
        );

        // Planning restoration reads the persisted intent; it is a plan,
        // not an outcome.
        let plan = ledger.restore_intent("AA:BB").unwrap();
        assert_eq!(plan.peripheral_id(), "AA:BB");
        assert_eq!(plan.intent(), AssociationIntent::OnDemand);
        assert_eq!(ledger.restorations_planned(), 1);
        assert_eq!(ledger.restore_intent("CC:DD"), Err(BluetoothError::UnknownPeripheral));

        // Reconciliation restores only observed devices.
        assert_eq!(
            ledger.reconcile("AA:BB", false),
            Ok(Reconciliation::Unchanged)
        );
        assert_eq!(
            ledger.association_state("AA:BB"),
            Ok(AssociationState::Suspended(SuspensionReason::BluetoothDisabled))
        );
        assert_eq!(ledger.reconcile("AA:BB", true), Ok(Reconciliation::Restored));
        assert_eq!(ledger.association_state("AA:BB"), Ok(AssociationState::Active));
        assert_eq!(ledger.revision("AA:BB"), Ok(3));
        assert_eq!(ledger.reconcile("AA:BB", true), Ok(Reconciliation::Unchanged));
        assert_eq!(ledger.reconciliations(), 1);
        assert_eq!(ledger.reconcile("ZZ:ZZ", true), Err(BluetoothError::UnknownPeripheral));

        // The association set is bounded and forget is destructive.
        ledger
            .record("CC:DD", AssociationIntent::AutoReconnect, &[])
            .unwrap();
        assert_eq!(
            ledger.record("EE:FF", AssociationIntent::AutoReconnect, &[]),
            Err(BluetoothError::AssociationBoundReached)
        );
        assert_eq!(ledger.len(), 2);
        ledger.forget("CC:DD").unwrap();
        assert_eq!(ledger.forget("CC:DD"), Err(BluetoothError::UnknownPeripheral));
        assert_eq!(ledger.len(), 1);
    }
}
