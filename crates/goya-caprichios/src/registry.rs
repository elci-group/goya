//! Deterministic negotiation for implemented recipe contracts.

use crate::apple_pay_caprichio::{
    ApplePayArtifact, ApplePayDeclaration, RuntimeCapabilities as ApplePayHost,
    Target as ApplePayTarget,
};
use crate::accelerator_caprichio::{
    AcceleratorArtifact, AcceleratorDeclaration, AcceleratorDiagnostic,
    AcceleratorRuntimeCapabilities as AcceleratorModelHost,
    AcceleratorTarget as AcceleratorModelTarget,
};
use crate::batching_caprichio::{
    BatchArtifact, BatchDeclaration, RuntimeCapabilities as BatchRuntime, Target as BatchTarget,
};
use crate::bluetooth_access_caprichio::{
    BluetoothAccessArtifact, BluetoothAccessDeclaration, RuntimeCapabilities as BluetoothAccessHost,
    Target as BluetoothAccessTarget,
};
use crate::bluetooth_caprichio::{
    BluetoothCentralArtifact, BluetoothCentralDeclaration, RuntimeCapabilities as BluetoothHost,
    Target as BluetoothTarget,
};
use crate::bluetooth_continuity_caprichio::{
    BluetoothContinuityArtifact, BluetoothContinuityDeclaration,
    RuntimeCapabilities as BluetoothContinuityHost, Target as BluetoothContinuityTarget,
};
use crate::bluetooth_control_caprichio::{
    BluetoothGattArtifact, BluetoothGattDeclaration, RuntimeCapabilities as BluetoothGattHost,
    Target as BluetoothGattTarget,
};
use crate::bluetooth_guard_caprichio::{
    BluetoothGuardArtifact, BluetoothGuardDeclaration, RuntimeCapabilities as BluetoothGuardHost,
    Target as BluetoothGuardTarget,
};
use crate::bluetooth_profile_caprichio::{
    BluetoothProfileArtifact, BluetoothProfileDeclaration, RuntimeCapabilities as BluetoothProfileHost,
    Target as BluetoothProfileTarget,
};
use crate::bluetooth_stream_caprichio::{
    BluetoothStreamArtifact, BluetoothStreamDeclaration, RuntimeCapabilities as BluetoothStreamHost,
    Target as BluetoothStreamTarget,
};
use crate::google_pay_caprichio::{
    GooglePayArtifact, GooglePayDeclaration, RuntimeCapabilities as GooglePayHost,
    Target as GooglePayTarget,
};
use crate::samsung_pay_caprichio::{
    RuntimeCapabilities as SamsungPayHost, SamsungPayArtifact, SamsungPayDeclaration,
    Target as SamsungPayTarget,
};
use crate::windows_hello_caprichio::{
    RuntimeCapabilities as WindowsHelloHost, Target as WindowsHelloTarget,
    WindowsHelloArtifact, WindowsHelloDeclaration,
};
use crate::catalog;
use crate::checksum_caprichio::{
    Artifact as ChecksumArtifact, Declaration as ChecksumDeclaration, HostProfile as ChecksumHost,
};
use crate::compression_caprichio::{
    Artifact as CompressionArtifact, Declaration as CompressionDeclaration,
    HostProfile as CompressionHost, Target as CompressionTarget,
};
use crate::compute_caprichio::{
    QueueArtifact, QueueDeclaration, RuntimeCapabilities as ComputeRuntime, Target as ComputeTarget,
};
use crate::csv_caprichio::{
    Artifact as CsvArtifact, Declaration as CsvDeclaration, HostProfile as CsvHost,
    Target as CsvTarget,
};
use crate::data_caprichio::{
    Artifact as DataArtifact, Declaration as DataDeclaration, HostProfile as DataHost,
};
use crate::deadline_caprichio::{
    DeadlineArtifact, DeadlineDeclaration, RuntimeCapabilities as DeadlineRuntime,
    Target as DeadlineTarget,
};
use crate::filesystem_caprichio::{
    FileReadArtifact, HostProfile as FileHost, ReadDeclaration, Target as FileTarget,
};
use crate::face_id_caprichio::{
    FaceIdArtifact, FaceIdDeclaration, RuntimeCapabilities as FaceIdHost, Target as FaceIdTarget,
};
use crate::touch_id_caprichio::{
    RuntimeCapabilities as TouchIdHost, Target as TouchIdTarget, TouchIdArtifact,
    TouchIdDeclaration,
};
use crate::http_caprichio::{
    HttpArtifact, HttpDeclaration, RuntimeCapabilities as HttpRuntime, Target as HttpTarget,
};
use crate::input_validation_caprichio::{
    Artifact as InputValidationArtifact, Declaration as InputValidationDeclaration,
    HostProfile as InputValidationHost, Target as InputValidationTarget,
};
use crate::keychain_caprichio::{
    KeychainArtifact, KeychainDeclaration, RuntimeCapabilities as KeychainHost,
    Target as KeychainTarget,
};
use crate::memory_caprichio::{
    MemoryArtifact, MemoryDeclaration, RuntimeCapabilities as MemoryRuntime, Target as MemoryTarget,
};
use crate::metal_caprichio::{
    DispatchArtifact as MetalArtifact, DispatchDeclaration as MetalDeclaration,
    RuntimeCapabilities as MetalRuntime, Target as MetalTarget,
};
use crate::parallelism_caprichio::{
    ParallelismArtifact, ParallelismDeclaration, RuntimeCapabilities as ParallelismRuntime,
    Target as ParallelismTarget,
};
use crate::priority_caprichio::{
    Artifact as PriorityArtifact, Declaration as PriorityDeclaration, HostProfile as PriorityHost,
    Target as PriorityTarget,
};
use crate::progress_caprichio::{
    ProgressArtifact, ProgressDeclaration, RuntimeCapabilities as ProgressRuntime,
    Target as ProgressTarget,
};
use crate::retry_caprichio::{
    RetryArtifact, RetryDeclaration, RuntimeCapabilities as RetryRuntime, Target as RetryTarget,
};
use crate::storage::{
    RuntimeCapabilities as StorageRuntime, StorageArtifact, StorageDeclaration,
    Target as StorageTarget,
};
use crate::stream_caprichio::{
    RuntimeCapabilities as StreamRuntime, StreamArtifact, StreamDeclaration, Target as StreamTarget,
};
use crate::{NetworkDeclaration, RuntimeCapabilities as NetworkRuntime, Target as NetworkTarget};
use std::collections::BTreeSet;
use std::fmt;

/// A typed request. Capability IDs remain strings so unknown and future IDs
/// can be rejected explicitly instead of being silently ignored.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Request {
    pub capabilities: Vec<String>,
    pub network: Option<NetworkDeclaration>,
    pub storage: Option<StorageDeclaration>,
    pub memory: Option<MemoryDeclaration>,
    pub compute: Option<QueueDeclaration>,
    pub data: Option<DataDeclaration>,
    pub progress: Option<ProgressDeclaration>,
    pub checksum: Option<ChecksumDeclaration>,
    pub retry: Option<RetryDeclaration>,
    pub filesystem: Option<ReadDeclaration>,
    pub http: Option<HttpDeclaration>,
    pub stream: Option<StreamDeclaration>,
    pub compression: Option<CompressionDeclaration>,
    pub batching: Option<BatchDeclaration>,
    pub deadline: Option<DeadlineDeclaration>,
    pub csv: Option<CsvDeclaration>,
    pub priority: Option<PriorityDeclaration>,
    pub parallelism: Option<ParallelismDeclaration>,
    pub input_validation: Option<InputValidationDeclaration>,
    pub keychain: Option<KeychainDeclaration>,
    pub metal_compute: Option<MetalDeclaration>,
    pub touch_id: Option<TouchIdDeclaration>,
    pub face_id: Option<FaceIdDeclaration>,
    pub apple_pay: Option<ApplePayDeclaration>,
    pub windows_hello: Option<WindowsHelloDeclaration>,
    pub google_pay: Option<GooglePayDeclaration>,
    pub samsung_pay: Option<SamsungPayDeclaration>,
    pub bluetooth_central: Option<BluetoothCentralDeclaration>,
    pub bluetooth_gatt: Option<BluetoothGattDeclaration>,
    pub bluetooth_stream: Option<BluetoothStreamDeclaration>,
    pub bluetooth_guard: Option<BluetoothGuardDeclaration>,
    pub bluetooth_continuity: Option<BluetoothContinuityDeclaration>,
    pub bluetooth_access: Option<BluetoothAccessDeclaration>,
    pub bluetooth_profile: Option<BluetoothProfileDeclaration>,
    pub accelerator_model: Option<AcceleratorDeclaration>,
}

/// Host facts are supplied by the caller; the registry does not infer support
/// from an operating system or target label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub network_target: NetworkTarget,
    pub network_runtime: NetworkRuntime,
    pub storage_target: StorageTarget,
    pub storage_runtime: StorageRuntime,
    pub memory_target: MemoryTarget,
    pub memory_runtime: MemoryRuntime,
    pub compute_target: ComputeTarget,
    pub compute_runtime: ComputeRuntime,
    pub data_host: DataHost,
    pub progress_target: ProgressTarget,
    pub progress_runtime: ProgressRuntime,
    pub checksum_host: ChecksumHost,
    pub retry_target: RetryTarget,
    pub retry_runtime: RetryRuntime,
    pub filesystem_target: FileTarget,
    pub filesystem_host: FileHost,
    pub http_target: HttpTarget,
    pub http_runtime: HttpRuntime,
    pub stream_target: StreamTarget,
    pub stream_runtime: StreamRuntime,
    pub compression_target: CompressionTarget,
    pub compression_host: CompressionHost,
    pub batching_target: BatchTarget,
    pub batching_runtime: BatchRuntime,
    pub deadline_target: DeadlineTarget,
    pub deadline_runtime: DeadlineRuntime,
    pub csv_target: CsvTarget,
    pub csv_host: CsvHost,
    pub priority_target: PriorityTarget,
    pub priority_host: PriorityHost,
    pub parallelism_target: ParallelismTarget,
    pub parallelism_runtime: ParallelismRuntime,
    pub input_validation_target: InputValidationTarget,
    pub input_validation_host: InputValidationHost,
    pub keychain_target: KeychainTarget,
    pub keychain_host: KeychainHost,
    pub metal_target: MetalTarget,
    pub metal_runtime: MetalRuntime,
    pub touch_id_target: TouchIdTarget,
    pub touch_id_host: TouchIdHost,
    pub face_id_target: FaceIdTarget,
    pub face_id_host: FaceIdHost,
    pub apple_pay_target: ApplePayTarget,
    pub apple_pay_host: ApplePayHost,
    pub windows_hello_target: WindowsHelloTarget,
    pub windows_hello_host: WindowsHelloHost,
    pub google_pay_target: GooglePayTarget,
    pub google_pay_host: GooglePayHost,
    pub samsung_pay_target: SamsungPayTarget,
    pub samsung_pay_host: SamsungPayHost,
    pub bluetooth_central_target: BluetoothTarget,
    pub bluetooth_central_host: BluetoothHost,
    pub bluetooth_gatt_target: BluetoothGattTarget,
    pub bluetooth_gatt_host: BluetoothGattHost,
    pub bluetooth_stream_target: BluetoothStreamTarget,
    pub bluetooth_stream_host: BluetoothStreamHost,
    pub bluetooth_guard_target: BluetoothGuardTarget,
    pub bluetooth_guard_host: BluetoothGuardHost,
    pub bluetooth_continuity_target: BluetoothContinuityTarget,
    pub bluetooth_continuity_host: BluetoothContinuityHost,
    pub bluetooth_access_target: BluetoothAccessTarget,
    pub bluetooth_access_host: BluetoothAccessHost,
    pub bluetooth_profile_target: BluetoothProfileTarget,
    pub bluetooth_profile_host: BluetoothProfileHost,
    pub accelerator_model_target: AcceleratorModelTarget,
    pub accelerator_model_host: AcceleratorModelHost,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Artifact {
    Network(crate::Artifact),
    Storage(StorageArtifact),
    Memory(MemoryArtifact),
    Compute(QueueArtifact),
    Data(DataArtifact),
    Progress(ProgressArtifact),
    Checksum(ChecksumArtifact),
    Retry(RetryArtifact),
    Filesystem(FileReadArtifact),
    Http(HttpArtifact),
    Stream(StreamArtifact),
    Compression(CompressionArtifact),
    Batching(BatchArtifact),
    Deadline(DeadlineArtifact),
    Csv(CsvArtifact),
    Priority(PriorityArtifact),
    Parallelism(ParallelismArtifact),
    InputValidation(InputValidationArtifact),
    Keychain(KeychainArtifact),
    Metal(MetalArtifact),
    TouchId(TouchIdArtifact),
    FaceId(FaceIdArtifact),
    ApplePay(ApplePayArtifact),
    WindowsHello(WindowsHelloArtifact),
    GooglePay(GooglePayArtifact),
    SamsungPay(SamsungPayArtifact),
    Bluetooth(BluetoothCentralArtifact),
    BluetoothGatt(BluetoothGattArtifact),
    BluetoothStream(BluetoothStreamArtifact),
    BluetoothGuard(BluetoothGuardArtifact),
    BluetoothContinuity(BluetoothContinuityArtifact),
    BluetoothAccess(BluetoothAccessArtifact),
    BluetoothProfile(BluetoothProfileArtifact),
    AcceleratorModel(AcceleratorArtifact),
}

impl Artifact {
    pub fn contract(&self) -> &'static str {
        match self {
            Self::Network(a) => a.contract,
            Self::Storage(a) => a.contract,
            Self::Memory(a) => a.contract,
            Self::Compute(a) => a.contract,
            Self::Data(a) => a.contract,
            Self::Progress(a) => a.contract,
            Self::Checksum(a) => a.contract,
            Self::Retry(a) => a.contract,
            Self::Filesystem(a) => a.contract,
            Self::Http(a) => a.contract,
            Self::Stream(a) => a.contract,
            Self::Compression(a) => a.contract,
            Self::Batching(a) => a.contract,
            Self::Deadline(a) => a.contract,
            Self::Csv(a) => a.contract,
            Self::Priority(a) => a.contract,
            Self::Parallelism(a) => a.contract,
            Self::InputValidation(a) => a.contract,
            Self::Keychain(a) => a.contract,
            Self::Metal(a) => a.contract,
            Self::TouchId(a) => a.contract,
            Self::FaceId(a) => a.contract,
            Self::ApplePay(a) => a.contract,
            Self::WindowsHello(a) => a.contract,
            Self::GooglePay(a) => a.contract,
            Self::SamsungPay(a) => a.contract,
            Self::Bluetooth(a) => a.contract,
            Self::BluetoothGatt(a) => a.contract,
            Self::BluetoothStream(a) => a.contract,
            Self::BluetoothGuard(a) => a.contract,
            Self::BluetoothContinuity(a) => a.contract,
            Self::BluetoothAccess(a) => a.contract,
            Self::BluetoothProfile(a) => a.contract,
            Self::AcceleratorModel(a) => a.contract,
        }
    }

    pub fn recipe(&self) -> &'static str {
        match self {
            Self::Network(a) => a.recipe,
            Self::Storage(a) => a.recipe,
            Self::Memory(a) => a.recipe,
            Self::Compute(_) => "process-local-bounded-fifo-queue",
            Self::Data(a) => a.recipe,
            Self::Progress(_) => "process-local-fixed-total-progress",
            Self::Checksum(a) => a.recipe,
            Self::Retry(_) => "process-local-bounded-retry-policy",
            Self::Filesystem(a) => a.recipe,
            Self::Http(_) => "browser-fetch-cors-limited-http",
            Self::Stream(_) => "process-local-bounded-byte-stream",
            Self::Compression(a) => a.recipe,
            Self::Batching(_) => "process-local-bounded-batch-builder",
            Self::Deadline(_) => "cooperative-process-local-monotonic-deadline",
            Self::Csv(a) => a.recipe,
            Self::Priority(a) => a.recipe,
            Self::Parallelism(_) => "bounded-process-local-parallel-executor",
            Self::InputValidation(a) => a.recipe,
            Self::Keychain(_) => "apple-keychain-namespace-bounded-kv",
            Self::Metal(_) => "apple-silicon-metal-checked-dispatch",
            Self::TouchId(_) => "apple-touch-id-bounded-attempt-gate",
            Self::FaceId(_) => "apple-face-id-bounded-attempt-gate",
            Self::ApplePay(_) => "apple-pay-bounded-payment-request",
            Self::WindowsHello(_) => "windows-hello-bounded-attempt-gate",
            Self::GooglePay(_) => "google-pay-bounded-payment-request",
            Self::SamsungPay(_) => "samsung-pay-bounded-payment-request",
            Self::Bluetooth(_) => "apple-ble-central-bounded-scan",
            Self::BluetoothGatt(_) => "apple-ble-gatt-bounded-commands",
            Self::BluetoothStream(_) => "apple-ble-stream-bounded-notifications",
            Self::BluetoothGuard(_) => "apple-ble-guard-bounded-authorisation",
            Self::BluetoothContinuity(_) => "apple-ble-continuity-bounded-restoration",
            Self::BluetoothAccess(_) => "apple-ble-access-bounded-mapping",
            Self::BluetoothProfile(_) => "apple-ble-profile-bounded-classification",
            Self::AcceleratorModel(_) => "per-model-device-accelerator",
        }
    }

    pub fn recipe_version(&self) -> &'static str {
        match self {
            Self::Network(a) => a.recipe_version,
            Self::Storage(a) => a.recipe_version,
            Self::Memory(a) => a.recipe_version,
            Self::Compute(a) => a.recipe_version,
            Self::Data(a) => a.recipe_version,
            Self::Progress(a) => a.recipe_version,
            Self::Checksum(a) => a.recipe_version,
            Self::Retry(a) => a.recipe_version,
            Self::Filesystem(a) => a.recipe_version,
            Self::Http(a) => a.recipe_version,
            Self::Stream(a) => a.recipe_version,
            Self::Compression(a) => a.recipe_version,
            Self::Batching(a) => a.recipe_version,
            Self::Deadline(a) => a.recipe_version,
            Self::Csv(a) => a.recipe_version,
            Self::Priority(a) => a.recipe_version,
            Self::Parallelism(a) => a.recipe_version,
            Self::InputValidation(a) => a.recipe_version,
            Self::Keychain(a) => a.recipe_version,
            Self::Metal(a) => a.recipe_version,
            Self::TouchId(a) => a.recipe_version,
            Self::FaceId(a) => a.recipe_version,
            Self::ApplePay(a) => a.recipe_version,
            Self::WindowsHello(a) => a.recipe_version,
            Self::GooglePay(a) => a.recipe_version,
            Self::SamsungPay(a) => a.recipe_version,
            Self::Bluetooth(a) => a.recipe_version,
            Self::BluetoothGatt(a) => a.recipe_version,
            Self::BluetoothStream(a) => a.recipe_version,
            Self::BluetoothGuard(a) => a.recipe_version,
            Self::BluetoothContinuity(a) => a.recipe_version,
            Self::BluetoothAccess(a) => a.recipe_version,
            Self::BluetoothProfile(a) => a.recipe_version,
            Self::AcceleratorModel(a) => a.recipe_version,
        }
    }

    pub fn canonical_ir(&self) -> &str {
        match self {
            Self::Network(a) => &a.canonical_ir,
            Self::Storage(a) => &a.canonical_ir,
            Self::Memory(a) => &a.canonical_ir,
            Self::Compute(a) => &a.canonical_ir,
            Self::Data(a) => &a.canonical_ir,
            Self::Progress(a) => &a.canonical_ir,
            Self::Checksum(a) => &a.canonical_ir,
            Self::Retry(a) => &a.canonical_ir,
            Self::Filesystem(a) => &a.canonical_ir,
            Self::Http(a) => &a.canonical_ir,
            Self::Stream(a) => &a.canonical_ir,
            Self::Compression(a) => &a.canonical_ir,
            Self::Batching(a) => &a.canonical_ir,
            Self::Deadline(a) => &a.canonical_ir,
            Self::Csv(a) => &a.canonical_ir,
            Self::Priority(a) => &a.canonical_ir,
            Self::Parallelism(a) => &a.canonical_ir,
            Self::InputValidation(a) => &a.canonical_ir,
            Self::Keychain(a) => &a.canonical_ir,
            Self::Metal(a) => &a.canonical_ir,
            Self::TouchId(a) => &a.canonical_ir,
            Self::FaceId(a) => &a.canonical_ir,
            Self::ApplePay(a) => &a.canonical_ir,
            Self::WindowsHello(a) => &a.canonical_ir,
            Self::GooglePay(a) => &a.canonical_ir,
            Self::SamsungPay(a) => &a.canonical_ir,
            Self::Bluetooth(a) => &a.canonical_ir,
            Self::BluetoothGatt(a) => &a.canonical_ir,
            Self::BluetoothStream(a) => &a.canonical_ir,
            Self::BluetoothGuard(a) => &a.canonical_ir,
            Self::BluetoothContinuity(a) => &a.canonical_ir,
            Self::BluetoothAccess(a) => &a.canonical_ir,
            Self::BluetoothProfile(a) => &a.canonical_ir,
            Self::AcceleratorModel(a) => &a.canonical_ir,
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Self::Network(a) => &a.source,
            Self::Storage(a) => &a.source,
            Self::Memory(a) => &a.source,
            Self::Compute(a) => &a.source,
            Self::Data(a) => &a.source,
            Self::Progress(a) => &a.source,
            Self::Checksum(a) => &a.source,
            Self::Retry(a) => &a.source,
            Self::Filesystem(a) => &a.source,
            Self::Http(a) => &a.source,
            Self::Stream(a) => &a.source,
            Self::Compression(a) => &a.source,
            Self::Batching(a) => &a.source,
            Self::Deadline(a) => &a.source,
            Self::Csv(a) => &a.source,
            Self::Priority(a) => &a.source,
            Self::Parallelism(a) => &a.source,
            Self::InputValidation(a) => &a.source,
            Self::Keychain(a) => &a.source,
            Self::Metal(a) => &a.source,
            Self::TouchId(a) => &a.source,
            Self::FaceId(a) => &a.source,
            Self::ApplePay(a) => &a.source,
            Self::WindowsHello(a) => &a.source,
            Self::GooglePay(a) => &a.source,
            Self::SamsungPay(a) => &a.source,
            Self::Bluetooth(a) => &a.source,
            Self::BluetoothGatt(a) => &a.source,
            Self::BluetoothStream(a) => &a.source,
            Self::BluetoothGuard(a) => &a.source,
            Self::BluetoothContinuity(a) => &a.source,
            Self::BluetoothAccess(a) => &a.source,
            Self::BluetoothProfile(a) => &a.source,
            Self::AcceleratorModel(a) => &a.source,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegotiatedCapability {
    pub id: &'static str,
    pub artifact: Artifact,
}

/// Exact contracts with a registered, implemented lowering. Draft catalog IDs
/// are intentionally excluded from this list.
pub const fn supported_contracts() -> &'static [&'static str] {
    &[
        crate::batching_caprichio::CONTRACT,
        crate::deadline_caprichio::CONTRACT,
        crate::parallelism_caprichio::CONTRACT,
        crate::priority_caprichio::CONTRACT,
        crate::progress_caprichio::CONTRACT,
        crate::retry_caprichio::CONTRACT,
        crate::compute_caprichio::CONTRACT,
        crate::data_caprichio::CONTRACT,
        crate::checksum_caprichio::CONTRACT,
        crate::compression_caprichio::CONTRACT,
        crate::csv_caprichio::CONTRACT,
        crate::stream_caprichio::CONTRACT,
        crate::filesystem_caprichio::CONTRACT,
        crate::face_id_caprichio::CONTRACT,
        crate::touch_id_caprichio::CONTRACT,
        crate::windows_hello_caprichio::CONTRACT,
        crate::bluetooth_access_caprichio::CONTRACT,
        crate::bluetooth_caprichio::CONTRACT,
        crate::bluetooth_continuity_caprichio::CONTRACT,
        crate::bluetooth_control_caprichio::CONTRACT,
        crate::bluetooth_guard_caprichio::CONTRACT,
        crate::bluetooth_profile_caprichio::CONTRACT,
        crate::bluetooth_stream_caprichio::CONTRACT,
        crate::metal_caprichio::CONTRACT,
        crate::apple_pay_caprichio::CONTRACT,
        crate::google_pay_caprichio::CONTRACT,
        crate::samsung_pay_caprichio::CONTRACT,
        crate::keychain_caprichio::CONTRACT,
        crate::input_validation_caprichio::CONTRACT,
        crate::memory_caprichio::CONTRACT,
        crate::CONTRACT,
        crate::http_caprichio::CONTRACT,
        crate::storage::CONTRACT,
    ]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NegotiationError {
    UnknownCapability(String),
    DuplicateCapability(String),
    UnsupportedCapability(String),
    MissingDeclaration { capability: &'static str },
    UnexpectedDeclaration { kind: &'static str },
    Network(crate::Diagnostic),
    Storage(crate::storage::Diagnostic),
    Memory(crate::memory_caprichio::Diagnostic),
    Compute(crate::compute_caprichio::Diagnostic),
    Data(crate::data_caprichio::Diagnostic),
    Progress(crate::progress_caprichio::Diagnostic),
    Checksum(crate::checksum_caprichio::Diagnostic),
    Retry(crate::retry_caprichio::Diagnostic),
    Filesystem(crate::filesystem_caprichio::Diagnostic),
    Http(crate::http_caprichio::Diagnostic),
    Stream(crate::stream_caprichio::Diagnostic),
    Compression(crate::compression_caprichio::Diagnostic),
    Batching(crate::batching_caprichio::Diagnostic),
    Deadline(crate::deadline_caprichio::Diagnostic),
    Csv(crate::csv_caprichio::Diagnostic),
    Priority(crate::priority_caprichio::Diagnostic),
    Parallelism(crate::parallelism_caprichio::Diagnostic),
    InputValidation(crate::input_validation_caprichio::Diagnostic),
    Keychain(crate::keychain_caprichio::Diagnostic),
    Metal(crate::metal_caprichio::Diagnostic),
    TouchId(crate::touch_id_caprichio::Diagnostic),
    FaceId(crate::face_id_caprichio::Diagnostic),
    ApplePay(crate::apple_pay_caprichio::Diagnostic),
    WindowsHello(crate::windows_hello_caprichio::Diagnostic),
    GooglePay(crate::google_pay_caprichio::Diagnostic),
    SamsungPay(crate::samsung_pay_caprichio::Diagnostic),
    Bluetooth(crate::bluetooth_caprichio::Diagnostic),
    BluetoothGatt(crate::bluetooth_control_caprichio::Diagnostic),
    BluetoothStream(crate::bluetooth_stream_caprichio::Diagnostic),
    BluetoothGuard(crate::bluetooth_guard_caprichio::Diagnostic),
    BluetoothContinuity(crate::bluetooth_continuity_caprichio::Diagnostic),
    BluetoothAccess(crate::bluetooth_access_caprichio::Diagnostic),
    BluetoothProfile(crate::bluetooth_profile_caprichio::Diagnostic),
}

impl fmt::Display for NegotiationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCapability(id) => write!(f, "unknown capability or contract ID `{id}`"),
            Self::DuplicateCapability(id) => {
                write!(f, "capability `{id}` was requested more than once")
            }
            Self::UnsupportedCapability(id) => {
                write!(f, "capability `{id}` has no conformant recipe lowering")
            }
            Self::MissingDeclaration { capability } => write!(
                f,
                "capability `{capability}` requires its typed declaration"
            ),
            Self::UnexpectedDeclaration { kind } => {
                write!(f, "{kind} declaration has no matching requested capability")
            }
            Self::Network(error) => write!(f, "network negotiation failed: {error}"),
            Self::Storage(error) => write!(f, "storage negotiation failed: {error}"),
            Self::Memory(error) => write!(f, "memory negotiation failed: {error}"),
            Self::Compute(error) => write!(f, "compute negotiation failed: {error}"),
            Self::Data(error) => write!(f, "data negotiation failed: {error}"),
            Self::Progress(error) => write!(f, "progress negotiation failed: {error}"),
            Self::Checksum(error) => write!(f, "checksum negotiation failed: {error}"),
            Self::Retry(error) => write!(f, "retry negotiation failed: {error}"),
            Self::Filesystem(error) => write!(f, "filesystem negotiation failed: {error}"),
            Self::Http(error) => write!(f, "HTTP negotiation failed: {error}"),
            Self::Stream(error) => write!(f, "stream negotiation failed: {error}"),
            Self::Compression(error) => write!(f, "compression negotiation failed: {error}"),
            Self::Batching(error) => write!(f, "batching negotiation failed: {error}"),
            Self::Deadline(error) => write!(f, "deadline negotiation failed: {error}"),
            Self::Csv(error) => write!(f, "CSV negotiation failed: {error}"),
            Self::Priority(error) => write!(f, "priority queue negotiation failed: {error}"),
            Self::Parallelism(error) => write!(f, "parallelism negotiation failed: {error}"),
            Self::InputValidation(error) => {
                write!(f, "input validation negotiation failed: {error}")
            }
            Self::Keychain(error) => write!(f, "keychain negotiation failed: {error}"),
            Self::Metal(error) => write!(f, "Metal dispatch negotiation failed: {error}"),
            Self::TouchId(error) => write!(f, "Touch ID negotiation failed: {error}"),
            Self::FaceId(error) => write!(f, "Face ID negotiation failed: {error}"),
            Self::ApplePay(error) => write!(f, "Apple Pay negotiation failed: {error}"),
            Self::WindowsHello(error) => write!(f, "Windows Hello negotiation failed: {error}"),
            Self::GooglePay(error) => write!(f, "Google Pay negotiation failed: {error}"),
            Self::SamsungPay(error) => write!(f, "Samsung Pay negotiation failed: {error}"),
            Self::Bluetooth(error) => write!(f, "Bluetooth negotiation failed: {error}"),
            Self::BluetoothGatt(error) => {
                write!(f, "Bluetooth GATT negotiation failed: {error}")
            }
            Self::BluetoothStream(error) => {
                write!(f, "Bluetooth stream negotiation failed: {error}")
            }
            Self::BluetoothGuard(error) => write!(f, "Bluetooth guard negotiation failed: {error}"),
            Self::BluetoothContinuity(error) => {
                write!(f, "Bluetooth continuity negotiation failed: {error}")
            }
            Self::BluetoothAccess(error) => write!(f, "Bluetooth access negotiation failed: {error}"),
            Self::BluetoothProfile(error) => {
                write!(f, "Bluetooth profile negotiation failed: {error}")
            }
        }
    }
}

impl std::error::Error for NegotiationError {}

/// Negotiate requested contract IDs in lexical order. Only the exact IDs listed by
/// [`supported_contracts`] are supported. Draft catalog IDs are not evidence of
/// conformance and return `UnsupportedCapability` until an implementation proves
/// the contract.
pub fn negotiate(
    request: &Request,
    host: HostProfile,
) -> Result<Vec<NegotiatedCapability>, NegotiationError> {
    let mut ordered = BTreeSet::new();
    for id in &request.capabilities {
        if !ordered.insert(id.as_str()) {
            return Err(NegotiationError::DuplicateCapability(id.clone()));
        }
    }

    for id in &ordered {
        if *id != crate::CONTRACT
            && *id != crate::storage::CONTRACT
            && *id != crate::memory_caprichio::CONTRACT
            && *id != crate::compute_caprichio::CONTRACT
            && *id != crate::data_caprichio::CONTRACT
            && *id != crate::progress_caprichio::CONTRACT
            && *id != crate::checksum_caprichio::CONTRACT
            && *id != crate::retry_caprichio::CONTRACT
            && *id != crate::filesystem_caprichio::CONTRACT
            && *id != crate::http_caprichio::CONTRACT
            && *id != crate::stream_caprichio::CONTRACT
            && *id != crate::compression_caprichio::CONTRACT
            && *id != crate::batching_caprichio::CONTRACT
            && *id != crate::deadline_caprichio::CONTRACT
            && *id != crate::csv_caprichio::CONTRACT
            && *id != crate::priority_caprichio::CONTRACT
            && *id != crate::parallelism_caprichio::CONTRACT
            && *id != crate::input_validation_caprichio::CONTRACT
            && *id != crate::keychain_caprichio::CONTRACT
            && *id != crate::metal_caprichio::CONTRACT
            && *id != crate::touch_id_caprichio::CONTRACT
            && *id != crate::face_id_caprichio::CONTRACT
            && *id != crate::apple_pay_caprichio::CONTRACT
            && *id != crate::windows_hello_caprichio::CONTRACT
            && *id != crate::google_pay_caprichio::CONTRACT
            && *id != crate::samsung_pay_caprichio::CONTRACT
            && *id != crate::bluetooth_caprichio::CONTRACT
            && *id != crate::bluetooth_control_caprichio::CONTRACT
            && *id != crate::bluetooth_stream_caprichio::CONTRACT
            && *id != crate::bluetooth_guard_caprichio::CONTRACT
            && *id != crate::bluetooth_continuity_caprichio::CONTRACT
            && *id != crate::bluetooth_access_caprichio::CONTRACT
            && *id != crate::bluetooth_profile_caprichio::CONTRACT
        {
            if catalog::find(id).is_some() {
                return Err(NegotiationError::UnsupportedCapability((*id).to_owned()));
            }
            return Err(NegotiationError::UnknownCapability((*id).to_owned()));
        }
    }

    let needs_network = ordered.contains(crate::CONTRACT);
    let needs_storage = ordered.contains(crate::storage::CONTRACT);
    let needs_memory = ordered.contains(crate::memory_caprichio::CONTRACT);
    let needs_compute = ordered.contains(crate::compute_caprichio::CONTRACT);
    let needs_data = ordered.contains(crate::data_caprichio::CONTRACT);
    let needs_progress = ordered.contains(crate::progress_caprichio::CONTRACT);
    let needs_checksum = ordered.contains(crate::checksum_caprichio::CONTRACT);
    let needs_retry = ordered.contains(crate::retry_caprichio::CONTRACT);
    let needs_filesystem = ordered.contains(crate::filesystem_caprichio::CONTRACT);
    let needs_http = ordered.contains(crate::http_caprichio::CONTRACT);
    let needs_stream = ordered.contains(crate::stream_caprichio::CONTRACT);
    let needs_compression = ordered.contains(crate::compression_caprichio::CONTRACT);
    let needs_batching = ordered.contains(crate::batching_caprichio::CONTRACT);
    let needs_deadline = ordered.contains(crate::deadline_caprichio::CONTRACT);
    let needs_csv = ordered.contains(crate::csv_caprichio::CONTRACT);
    let needs_priority = ordered.contains(crate::priority_caprichio::CONTRACT);
    let needs_parallelism = ordered.contains(crate::parallelism_caprichio::CONTRACT);
    let needs_input_validation = ordered.contains(crate::input_validation_caprichio::CONTRACT);
    let needs_keychain = ordered.contains(crate::keychain_caprichio::CONTRACT);
    let needs_metal = ordered.contains(crate::metal_caprichio::CONTRACT);
    let needs_touch_id = ordered.contains(crate::touch_id_caprichio::CONTRACT);
    let needs_face_id = ordered.contains(crate::face_id_caprichio::CONTRACT);
    let needs_apple_pay = ordered.contains(crate::apple_pay_caprichio::CONTRACT);
    let needs_windows_hello = ordered.contains(crate::windows_hello_caprichio::CONTRACT);
    let needs_google_pay = ordered.contains(crate::google_pay_caprichio::CONTRACT);
    let needs_samsung_pay = ordered.contains(crate::samsung_pay_caprichio::CONTRACT);
    let needs_bluetooth_central = ordered.contains(crate::bluetooth_caprichio::CONTRACT);
    let needs_bluetooth_gatt = ordered.contains(crate::bluetooth_control_caprichio::CONTRACT);
    let needs_bluetooth_stream = ordered.contains(crate::bluetooth_stream_caprichio::CONTRACT);
    let needs_bluetooth_guard = ordered.contains(crate::bluetooth_guard_caprichio::CONTRACT);
    let needs_bluetooth_continuity =
        ordered.contains(crate::bluetooth_continuity_caprichio::CONTRACT);
    let needs_bluetooth_access = ordered.contains(crate::bluetooth_access_caprichio::CONTRACT);
    let needs_bluetooth_profile = ordered.contains(crate::bluetooth_profile_caprichio::CONTRACT);
    if request.network.is_some() && !needs_network {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "network" });
    }
    if request.storage.is_some() && !needs_storage {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "storage" });
    }
    if request.memory.is_some() && !needs_memory {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "memory" });
    }
    if request.compute.is_some() && !needs_compute {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "compute" });
    }
    if request.data.is_some() && !needs_data {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "data" });
    }
    if request.progress.is_some() && !needs_progress {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "progress" });
    }
    if request.checksum.is_some() && !needs_checksum {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "checksum" });
    }
    if request.retry.is_some() && !needs_retry {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "retry" });
    }
    if request.filesystem.is_some() && !needs_filesystem {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "filesystem" });
    }
    if request.http.is_some() && !needs_http {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "http" });
    }
    if request.stream.is_some() && !needs_stream {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "stream" });
    }
    if request.compression.is_some() && !needs_compression {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "compression",
        });
    }
    if request.batching.is_some() && !needs_batching {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "batching" });
    }
    if request.deadline.is_some() && !needs_deadline {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "deadline" });
    }
    if request.csv.is_some() && !needs_csv {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "csv" });
    }
    if request.priority.is_some() && !needs_priority {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "priority" });
    }
    if request.parallelism.is_some() && !needs_parallelism {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "parallelism",
        });
    }
    if request.input_validation.is_some() && !needs_input_validation {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "input_validation",
        });
    }
    if request.keychain.is_some() && !needs_keychain {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "keychain" });
    }
    if request.metal_compute.is_some() && !needs_metal {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "metal_compute",
        });
    }
    if request.touch_id.is_some() && !needs_touch_id {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "touch_id" });
    }
    if request.face_id.is_some() && !needs_face_id {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "face_id" });
    }
    if request.apple_pay.is_some() && !needs_apple_pay {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "apple_pay",
        });
    }
    if request.windows_hello.is_some() && !needs_windows_hello {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "windows_hello",
        });
    }
    if request.google_pay.is_some() && !needs_google_pay {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "google_pay",
        });
    }
    if request.samsung_pay.is_some() && !needs_samsung_pay {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "samsung_pay",
        });
    }
    if request.bluetooth_central.is_some() && !needs_bluetooth_central {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_central",
        });
    }
    if request.bluetooth_gatt.is_some() && !needs_bluetooth_gatt {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_gatt",
        });
    }
    if request.bluetooth_stream.is_some() && !needs_bluetooth_stream {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_stream",
        });
    }
    if request.bluetooth_guard.is_some() && !needs_bluetooth_guard {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_guard",
        });
    }
    if request.bluetooth_continuity.is_some() && !needs_bluetooth_continuity {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_continuity",
        });
    }
    if request.bluetooth_access.is_some() && !needs_bluetooth_access {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_access",
        });
    }
    if request.bluetooth_profile.is_some() && !needs_bluetooth_profile {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "bluetooth_profile",
        });
    }

    let mut result = Vec::with_capacity(ordered.len());
    for id in ordered {
        let artifact = match id {
            crate::CONTRACT => {
                let declaration = request
                    .network
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::CONTRACT,
                    })?;
                let ir = declaration.normalize().map_err(NegotiationError::Network)?;
                Artifact::Network(
                    ir.lower_checked(host.network_target, host.network_runtime)
                        .map_err(NegotiationError::Network)?,
                )
            }
            crate::storage::CONTRACT => {
                let declaration =
                    request
                        .storage
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::storage::CONTRACT,
                        })?;
                let ir = declaration.normalize().map_err(NegotiationError::Storage)?;
                Artifact::Storage(
                    ir.lower(host.storage_target, host.storage_runtime)
                        .map_err(NegotiationError::Storage)?,
                )
            }
            crate::memory_caprichio::CONTRACT => {
                let declaration = request.memory.ok_or(NegotiationError::MissingDeclaration {
                    capability: crate::memory_caprichio::CONTRACT,
                })?;
                let ir = declaration.normalize().map_err(NegotiationError::Memory)?;
                Artifact::Memory(
                    ir.lower(host.memory_target, host.memory_runtime)
                        .map_err(NegotiationError::Memory)?,
                )
            }
            crate::compute_caprichio::CONTRACT => {
                let declaration = request
                    .compute
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::compute_caprichio::CONTRACT,
                    })?;
                let ir = declaration.normalize().map_err(NegotiationError::Compute)?;
                Artifact::Compute(
                    ir.lower_checked(host.compute_target, host.compute_runtime)
                        .map_err(NegotiationError::Compute)?,
                )
            }
            crate::data_caprichio::CONTRACT => {
                let declaration = request.data.ok_or(NegotiationError::MissingDeclaration {
                    capability: crate::data_caprichio::CONTRACT,
                })?;
                let ir = declaration.normalize().map_err(NegotiationError::Data)?;
                Artifact::Data(
                    ir.lower_checked(host.data_host)
                        .map_err(NegotiationError::Data)?,
                )
            }
            crate::progress_caprichio::CONTRACT => {
                let declaration = request
                    .progress
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::progress_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Progress)?;
                Artifact::Progress(
                    ir.lower_checked(host.progress_target, host.progress_runtime)
                        .map_err(NegotiationError::Progress)?,
                )
            }
            crate::checksum_caprichio::CONTRACT => {
                let declaration = request
                    .checksum
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::checksum_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Checksum)?;
                Artifact::Checksum(
                    ir.lower_checked(host.checksum_host)
                        .map_err(NegotiationError::Checksum)?,
                )
            }
            crate::retry_caprichio::CONTRACT => {
                let declaration = request.retry.ok_or(NegotiationError::MissingDeclaration {
                    capability: crate::retry_caprichio::CONTRACT,
                })?;
                let ir = declaration.normalize().map_err(NegotiationError::Retry)?;
                Artifact::Retry(
                    ir.lower_checked(host.retry_target, host.retry_runtime)
                        .map_err(NegotiationError::Retry)?,
                )
            }
            crate::filesystem_caprichio::CONTRACT => {
                let declaration =
                    request
                        .filesystem
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::filesystem_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Filesystem)?;
                Artifact::Filesystem(
                    ir.lower_checked(host.filesystem_target, host.filesystem_host)
                        .map_err(NegotiationError::Filesystem)?,
                )
            }
            crate::http_caprichio::CONTRACT => {
                let declaration =
                    request
                        .http
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::http_caprichio::CONTRACT,
                        })?;
                let ir = declaration.normalize().map_err(NegotiationError::Http)?;
                Artifact::Http(
                    ir.lower_checked(host.http_target, host.http_runtime)
                        .map_err(NegotiationError::Http)?,
                )
            }
            crate::stream_caprichio::CONTRACT => {
                let declaration = request.stream.ok_or(NegotiationError::MissingDeclaration {
                    capability: crate::stream_caprichio::CONTRACT,
                })?;
                let ir = declaration.normalize().map_err(NegotiationError::Stream)?;
                Artifact::Stream(
                    ir.lower_checked(host.stream_target, host.stream_runtime)
                        .map_err(NegotiationError::Stream)?,
                )
            }
            crate::compression_caprichio::CONTRACT => {
                let declaration =
                    request
                        .compression
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::compression_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Compression)?;
                Artifact::Compression(
                    ir.lower_checked(host.compression_target, host.compression_host)
                        .map_err(NegotiationError::Compression)?,
                )
            }
            crate::batching_caprichio::CONTRACT => {
                let declaration = request
                    .batching
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::batching_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Batching)?;
                Artifact::Batching(
                    ir.lower_checked(host.batching_target, host.batching_runtime)
                        .map_err(NegotiationError::Batching)?,
                )
            }
            crate::deadline_caprichio::CONTRACT => {
                let declaration = request
                    .deadline
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::deadline_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Deadline)?;
                Artifact::Deadline(
                    ir.lower_checked(host.deadline_target, host.deadline_runtime)
                        .map_err(NegotiationError::Deadline)?,
                )
            }
            crate::csv_caprichio::CONTRACT => {
                let declaration = request.csv.ok_or(NegotiationError::MissingDeclaration {
                    capability: crate::csv_caprichio::CONTRACT,
                })?;
                let ir = declaration.normalize().map_err(NegotiationError::Csv)?;
                Artifact::Csv(
                    ir.lower_checked(host.csv_target, host.csv_host)
                        .map_err(NegotiationError::Csv)?,
                )
            }
            crate::priority_caprichio::CONTRACT => {
                let declaration = request
                    .priority
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::priority_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Priority)?;
                Artifact::Priority(
                    ir.lower_checked(host.priority_target, host.priority_host)
                        .map_err(NegotiationError::Priority)?,
                )
            }
            crate::parallelism_caprichio::CONTRACT => {
                let declaration =
                    request
                        .parallelism
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::parallelism_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Parallelism)?;
                Artifact::Parallelism(
                    ir.lower_checked(host.parallelism_target, host.parallelism_runtime)
                        .map_err(NegotiationError::Parallelism)?,
                )
            }
            crate::input_validation_caprichio::CONTRACT => {
                let declaration = request.input_validation.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::input_validation_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::InputValidation)?;
                Artifact::InputValidation(
                    ir.lower_checked(host.input_validation_target, host.input_validation_host)
                        .map_err(NegotiationError::InputValidation)?,
                )
            }
            crate::keychain_caprichio::CONTRACT => {
                let declaration = request.keychain.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::keychain_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Keychain)?;
                Artifact::Keychain(
                    ir.lower_checked(host.keychain_target, host.keychain_host)
                        .map_err(NegotiationError::Keychain)?,
                )
            }
            crate::metal_caprichio::CONTRACT => {
                let declaration = request
                    .metal_compute
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::metal_caprichio::CONTRACT,
                    })?;
                let ir = declaration.normalize().map_err(NegotiationError::Metal)?;
                Artifact::Metal(
                    ir.lower_checked(host.metal_target, host.metal_runtime)
                        .map_err(NegotiationError::Metal)?,
                )
            }
            crate::touch_id_caprichio::CONTRACT => {
                let declaration = request
                    .touch_id
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::touch_id_caprichio::CONTRACT,
                    })?;
                let ir = declaration.normalize().map_err(NegotiationError::TouchId)?;
                Artifact::TouchId(
                    ir.lower_checked(host.touch_id_target, host.touch_id_host)
                        .map_err(NegotiationError::TouchId)?,
                )
            }
            crate::face_id_caprichio::CONTRACT => {
                let declaration =
                    request
                        .face_id
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::face_id_caprichio::CONTRACT,
                        })?;
                let ir = declaration.normalize().map_err(NegotiationError::FaceId)?;
                Artifact::FaceId(
                    ir.lower_checked(host.face_id_target, host.face_id_host)
                        .map_err(NegotiationError::FaceId)?,
                )
            }
            crate::apple_pay_caprichio::CONTRACT => {
                let declaration = request.apple_pay.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::apple_pay_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration.normalize().map_err(NegotiationError::ApplePay)?;
                Artifact::ApplePay(
                    ir.lower_checked(host.apple_pay_target, host.apple_pay_host)
                        .map_err(NegotiationError::ApplePay)?,
                )
            }
            crate::windows_hello_caprichio::CONTRACT => {
                let declaration = request
                    .windows_hello
                    .ok_or(NegotiationError::MissingDeclaration {
                        capability: crate::windows_hello_caprichio::CONTRACT,
                    })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::WindowsHello)?;
                Artifact::WindowsHello(
                    ir.lower_checked(host.windows_hello_target, host.windows_hello_host)
                        .map_err(NegotiationError::WindowsHello)?,
                )
            }
            crate::google_pay_caprichio::CONTRACT => {
                let declaration = request.google_pay.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::google_pay_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::GooglePay)?;
                Artifact::GooglePay(
                    ir.lower_checked(host.google_pay_target, host.google_pay_host)
                        .map_err(NegotiationError::GooglePay)?,
                )
            }
            crate::samsung_pay_caprichio::CONTRACT => {
                let declaration = request.samsung_pay.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::samsung_pay_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::SamsungPay)?;
                Artifact::SamsungPay(
                    ir.lower_checked(host.samsung_pay_target, host.samsung_pay_host)
                        .map_err(NegotiationError::SamsungPay)?,
                )
            }
            crate::bluetooth_caprichio::CONTRACT => {
                let declaration = request.bluetooth_central.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration.normalize().map_err(NegotiationError::Bluetooth)?;
                Artifact::Bluetooth(
                    ir.lower_checked(host.bluetooth_central_target, host.bluetooth_central_host)
                        .map_err(NegotiationError::Bluetooth)?,
                )
            }
            crate::bluetooth_control_caprichio::CONTRACT => {
                let declaration = request.bluetooth_gatt.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_control_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothGatt)?;
                Artifact::BluetoothGatt(
                    ir.lower_checked(host.bluetooth_gatt_target, host.bluetooth_gatt_host)
                        .map_err(NegotiationError::BluetoothGatt)?,
                )
            }
            crate::bluetooth_stream_caprichio::CONTRACT => {
                let declaration = request.bluetooth_stream.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_stream_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothStream)?;
                Artifact::BluetoothStream(
                    ir.lower_checked(host.bluetooth_stream_target, host.bluetooth_stream_host)
                        .map_err(NegotiationError::BluetoothStream)?,
                )
            }
            crate::bluetooth_guard_caprichio::CONTRACT => {
                let declaration = request.bluetooth_guard.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_guard_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothGuard)?;
                Artifact::BluetoothGuard(
                    ir.lower_checked(host.bluetooth_guard_target, host.bluetooth_guard_host)
                        .map_err(NegotiationError::BluetoothGuard)?,
                )
            }
            crate::bluetooth_continuity_caprichio::CONTRACT => {
                let declaration = request.bluetooth_continuity.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_continuity_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothContinuity)?;
                Artifact::BluetoothContinuity(
                    ir.lower_checked(
                        host.bluetooth_continuity_target,
                        host.bluetooth_continuity_host,
                    )
                    .map_err(NegotiationError::BluetoothContinuity)?,
                )
            }
            crate::bluetooth_access_caprichio::CONTRACT => {
                let declaration = request.bluetooth_access.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_access_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothAccess)?;
                Artifact::BluetoothAccess(
                    ir.lower_checked(host.bluetooth_access_target, host.bluetooth_access_host)
                        .map_err(NegotiationError::BluetoothAccess)?,
                )
            }
            crate::bluetooth_profile_caprichio::CONTRACT => {
                let declaration = request.bluetooth_profile.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::bluetooth_profile_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::BluetoothProfile)?;
                Artifact::BluetoothProfile(
                    ir.lower_checked(host.bluetooth_profile_target, host.bluetooth_profile_host)
                        .map_err(NegotiationError::BluetoothProfile)?,
                )
            }
            _ => unreachable!("request IDs validated before lowering"),
        };
        let id = match artifact {
            Artifact::Network(_) => crate::CONTRACT,
            Artifact::Storage(_) => crate::storage::CONTRACT,
            Artifact::Memory(_) => crate::memory_caprichio::CONTRACT,
            Artifact::Compute(_) => crate::compute_caprichio::CONTRACT,
            Artifact::Data(_) => crate::data_caprichio::CONTRACT,
            Artifact::Progress(_) => crate::progress_caprichio::CONTRACT,
            Artifact::Checksum(_) => crate::checksum_caprichio::CONTRACT,
            Artifact::Retry(_) => crate::retry_caprichio::CONTRACT,
            Artifact::Filesystem(_) => crate::filesystem_caprichio::CONTRACT,
            Artifact::Http(_) => crate::http_caprichio::CONTRACT,
            Artifact::Stream(_) => crate::stream_caprichio::CONTRACT,
            Artifact::Compression(_) => crate::compression_caprichio::CONTRACT,
            Artifact::Batching(_) => crate::batching_caprichio::CONTRACT,
            Artifact::Deadline(_) => crate::deadline_caprichio::CONTRACT,
            Artifact::Csv(_) => crate::csv_caprichio::CONTRACT,
            Artifact::Priority(_) => crate::priority_caprichio::CONTRACT,
            Artifact::Parallelism(_) => crate::parallelism_caprichio::CONTRACT,
            Artifact::InputValidation(_) => crate::input_validation_caprichio::CONTRACT,
            Artifact::Keychain(_) => crate::keychain_caprichio::CONTRACT,
            Artifact::Metal(_) => crate::metal_caprichio::CONTRACT,
            Artifact::TouchId(_) => crate::touch_id_caprichio::CONTRACT,
            Artifact::FaceId(_) => crate::face_id_caprichio::CONTRACT,
            Artifact::ApplePay(_) => crate::apple_pay_caprichio::CONTRACT,
            Artifact::WindowsHello(_) => crate::windows_hello_caprichio::CONTRACT,
            Artifact::GooglePay(_) => crate::google_pay_caprichio::CONTRACT,
            Artifact::SamsungPay(_) => crate::samsung_pay_caprichio::CONTRACT,
            Artifact::Bluetooth(_) => crate::bluetooth_caprichio::CONTRACT,
            Artifact::BluetoothGatt(_) => crate::bluetooth_control_caprichio::CONTRACT,
            Artifact::BluetoothStream(_) => crate::bluetooth_stream_caprichio::CONTRACT,
            Artifact::BluetoothGuard(_) => crate::bluetooth_guard_caprichio::CONTRACT,
            Artifact::BluetoothContinuity(_) => crate::bluetooth_continuity_caprichio::CONTRACT,
            Artifact::BluetoothAccess(_) => crate::bluetooth_access_caprichio::CONTRACT,
            Artifact::BluetoothProfile(_) => crate::bluetooth_profile_caprichio::CONTRACT,
            // Accelerator contracts are per device model; the artifact carries
            // its own static contract ID.
            Artifact::AcceleratorModel(ref a) => a.contract,
        };
        result.push(NegotiatedCapability { id, artifact });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Durability;

    fn host() -> HostProfile {
        HostProfile {
            network_target: NetworkTarget::Browser,
            network_runtime: NetworkRuntime {
                fetch: true,
                abort_controller: true,
                url: true,
                uint8_array: true,
                timers: true,
            },
            storage_target: StorageTarget::Browser,
            storage_runtime: StorageRuntime { indexed_db: true },
            memory_target: MemoryTarget::ProcessLocal,
            memory_runtime: MemoryRuntime {
                memory_runtime: true,
                pointer_width_bits: usize::BITS,
            },
            compute_target: ComputeTarget::ProcessLocal,
            compute_runtime: ComputeRuntime {
                process_local_memory: true,
                standard_mutex: true,
                pointer_width_bits: usize::BITS as u8,
            },
            data_host: DataHost {
                pointer_width: crate::data_caprichio::PointerWidth::Bits64,
                standard_allocator: true,
                byte_slices: true,
            },
            progress_target: ProgressTarget::ProcessLocal,
            progress_runtime: ProgressRuntime {
                process_local_memory: true,
            },
            checksum_host: ChecksumHost {
                standard_library: true,
                byte_slices: true,
                u64_arithmetic: true,
            },
            retry_target: RetryTarget::ProcessLocal,
            retry_runtime: RetryRuntime {
                process_local_memory: true,
                standard_duration: true,
            },
            filesystem_target: FileTarget::ProcessLocalRust,
            filesystem_host: FileHost {
                standard_library: true,
                file_read: true,
                pointer_width_bits: usize::BITS as u8,
            },
            http_target: crate::http_caprichio::Target::Browser,
            http_runtime: crate::http_caprichio::RuntimeCapabilities {
                fetch: true,
                url: true,
                response_headers: true,
            },
            stream_target: StreamTarget::ProcessLocalRust,
            stream_runtime: StreamRuntime {
                process_local_memory: true,
                standard_mutex: true,
                pointer_width_bits: usize::BITS as u8,
            },
            compression_target: CompressionTarget::ProcessLocalRust,
            compression_host: CompressionHost {
                pointer_width_bits: usize::BITS as u8,
                standard_allocator: true,
                byte_slices: true,
            },
            batching_target: BatchTarget::ProcessLocalRust,
            batching_runtime: BatchRuntime {
                process_local_memory: true,
                standard_mutex: true,
                pointer_width_bits: usize::BITS as u8,
            },
            deadline_target: DeadlineTarget::ProcessLocalRust,
            deadline_runtime: DeadlineRuntime {
                process_local_memory: true,
                monotonic_clock: true,
                checked_instant_add: true,
            },
            csv_target: CsvTarget::ProcessLocalRust,
            csv_host: CsvHost {
                pointer_width_bits: usize::BITS as u8,
                standard_library: true,
                utf8_strings: true,
            },
            priority_target: PriorityTarget::ProcessLocalRust,
            priority_host: PriorityHost {
                pointer_width_bits: usize::BITS as u8,
                standard_library: true,
                standard_mutex: true,
                fallible_reservation: true,
            },
            parallelism_target: ParallelismTarget::ProcessLocalRust,
            parallelism_runtime: ParallelismRuntime {
                process_local_memory: true,
                standard_threads: true,
                panic_unwind: true,
                target_pointer_width_bits: usize::BITS as u8,
            },
            input_validation_target: InputValidationTarget::ProcessLocalRust,
            input_validation_host: InputValidationHost {
                pointer_width_bits: usize::BITS as u8,
                standard_library: true,
                utf8_strings: true,
            },
            keychain_target: KeychainTarget::Ios,
            keychain_host: KeychainHost {
                keychain_services: true,
                hardware_backed_keys: true,
                utf8_strings: true,
            },
            metal_target: MetalTarget::AppleSilicon,
            metal_runtime: MetalRuntime {
                metal_device: true,
                command_queue: true,
                unified_memory: true,
                msl_version_3: true,
            },
            touch_id_target: TouchIdTarget::Ios,
            touch_id_host: TouchIdHost {
                touch_id_sensor: true,
                local_authentication: true,
                secure_enclave_match: true,
                utf8_strings: true,
            },
            face_id_target: FaceIdTarget::Ios,
            face_id_host: FaceIdHost {
                face_id_sensor: true,
                camera_available: true,
                local_authentication: true,
                secure_enclave_match: true,
                utf8_strings: true,
            },
            apple_pay_target: ApplePayTarget::Ios,
            apple_pay_host: ApplePayHost {
                pass_kit: true,
                secure_element_credential: true,
                network_session: true,
                utf8_strings: true,
            },
            windows_hello_target: WindowsHelloTarget::Windows,
            windows_hello_host: WindowsHelloHost {
                hello_sensor: true,
                hello_provider: true,
                tpm_backed_match: true,
                utf8_strings: true,
            },
            google_pay_target: GooglePayTarget::Android,
            google_pay_host: GooglePayHost {
                google_pay_services: true,
                secure_element_credential: true,
                network_session: true,
                utf8_strings: true,
            },
            samsung_pay_target: SamsungPayTarget::SamsungAndroid,
            samsung_pay_host: SamsungPayHost {
                samsung_pay_services: true,
                samsung_device: true,
                secure_element_credential: true,
                network_session: true,
                utf8_strings: true,
            },
            bluetooth_central_target: BluetoothTarget::Ios,
            bluetooth_central_host: BluetoothHost {
                core_bluetooth_central: true,
                bluetooth_powered: true,
                bluetooth_permission: true,
                utf8_strings: true,
            },
            bluetooth_gatt_target: BluetoothGattTarget::Ios,
            bluetooth_gatt_host: BluetoothGattHost {
                core_bluetooth_gatt: true,
                bluetooth_powered: true,
                bluetooth_permission: true,
                utf8_strings: true,
            },
            bluetooth_stream_target: BluetoothStreamTarget::Ios,
            bluetooth_stream_host: BluetoothStreamHost {
                core_bluetooth_gatt: true,
                characteristic_notifications: true,
                characteristic_indications: true,
                bluetooth_powered: true,
                bluetooth_permission: true,
                utf8_strings: true,
            },
            bluetooth_guard_target: BluetoothGuardTarget::Ios,
            bluetooth_guard_host: BluetoothGuardHost {
                bluetooth_permission_query: true,
                bluetooth_authorisation_store: true,
                utf8_strings: true,
            },
            bluetooth_continuity_target: BluetoothContinuityTarget::Ios,
            bluetooth_continuity_host: BluetoothContinuityHost {
                bluetooth_state_restoration: true,
                persistent_storage: true,
                bluetooth_power_monitoring: true,
                utf8_strings: true,
            },
            bluetooth_access_target: BluetoothAccessTarget::Ios,
            bluetooth_access_host: BluetoothAccessHost {
                accessibility_event_surface: true,
                accessible_labels_supported: true,
                utf8_strings: true,
            },
            bluetooth_profile_target: BluetoothProfileTarget::Ios,
            bluetooth_profile_host: BluetoothProfileHost {
                utf8_strings: true,
            },
        }
    }

    fn network() -> NetworkDeclaration {
        NetworkDeclaration {
            timeout_ms: 2500,
            concurrency: 3,
            retries: 0,
            offline_queue: false,
        }
    }

    fn storage() -> StorageDeclaration {
        StorageDeclaration {
            namespace: "registry.test".into(),
            max_value_bytes: 2048,
            durability: Durability::BestEffort,
            multi_key_transactions: false,
            encryption_at_rest: false,
        }
    }

    fn memory() -> MemoryDeclaration {
        MemoryDeclaration {
            max_total_bytes: 1_048_576,
            max_buffer_bytes: 65_536,
            max_buffers: 32,
            max_owners: 8,
            shared_across_processes: false,
            persistent: false,
            hardware_backed: false,
            zero_copy: false,
        }
    }

    fn compute() -> QueueDeclaration {
        QueueDeclaration { capacity: 64 }
    }

    fn data() -> DataDeclaration {
        DataDeclaration {
            max_payload_bytes: 4096,
        }
    }

    fn progress() -> ProgressDeclaration {
        ProgressDeclaration { total: 100 }
    }

    fn checksum() -> ChecksumDeclaration {
        ChecksumDeclaration {
            algorithm: crate::checksum_caprichio::Algorithm::Fnv1a64,
            require_cryptographic_security: false,
            require_collision_resistance: false,
            require_authentication: false,
        }
    }

    fn retry() -> RetryDeclaration {
        RetryDeclaration {
            max_attempts: 4,
            initial_delay_ms: 10,
            max_delay_ms: 100,
        }
    }

    fn filesystem() -> ReadDeclaration {
        ReadDeclaration { max_bytes: 4096 }
    }

    fn http() -> HttpDeclaration {
        HttpDeclaration {
            method: crate::http_caprichio::Method::Get,
            target: "https://api.example.test/v1/items".into(),
            redirect: crate::http_caprichio::RedirectPolicy::Error,
            credentials: crate::http_caprichio::CredentialPolicy::Omit,
        }
    }

    fn stream() -> StreamDeclaration {
        StreamDeclaration {
            max_buffered_bytes: 4096,
            max_chunks: 64,
        }
    }

    fn compression() -> CompressionDeclaration {
        CompressionDeclaration {
            max_output_bytes: 4096,
            max_encoded_bytes: 8192,
        }
    }

    fn batching() -> BatchDeclaration {
        BatchDeclaration {
            max_items: 4,
            max_batch_bytes: 1024,
            max_item_bytes: 256,
        }
    }

    fn deadline() -> DeadlineDeclaration {
        DeadlineDeclaration { duration_ms: 500 }
    }

    fn csv() -> CsvDeclaration {
        CsvDeclaration {
            delimiter: b',',
            max_total_bytes: 4096,
            max_records: 20,
            max_cells: 100,
            max_fields_per_record: 10,
            max_field_bytes: 512,
        }
    }

    fn priority() -> PriorityDeclaration {
        PriorityDeclaration {
            max_items: 32,
            max_total_bytes: 4096,
            max_item_bytes: 1024,
        }
    }

    fn input_validation() -> InputValidationDeclaration {
        InputValidationDeclaration {
            min_bytes: 1,
            max_bytes: 16,
            allowed: crate::input_validation_caprichio::AllowedAscii::Digits,
        }
    }

    fn keychain() -> KeychainDeclaration {
        KeychainDeclaration {
            namespace: "com.goya.registry.test".into(),
            max_value_bytes: 4096,
            accessibility: crate::keychain_caprichio::Accessibility::WhenUnlocked,
        }
    }

    fn metal_dispatch() -> MetalDeclaration {
        MetalDeclaration {
            threads_per_group: 64,
            groups: 8,
            max_buffer_bytes: 4096,
        }
    }

    fn touch_id() -> TouchIdDeclaration {
        TouchIdDeclaration {
            max_attempts: 3,
            timeout_ms: 15_000,
            allow_passcode_fallback: true,
        }
    }

    fn face_id() -> FaceIdDeclaration {
        FaceIdDeclaration {
            max_attempts: 2,
            timeout_ms: 10_000,
            require_attention: true,
        }
    }

    fn apple_pay() -> ApplePayDeclaration {
        ApplePayDeclaration {
            merchant_id: "com.goya.registry.pay".into(),
            currency_code: "USD".into(),
            max_minor_units: 100_000,
        }
    }

    fn windows_hello() -> WindowsHelloDeclaration {
        WindowsHelloDeclaration {
            max_attempts: 3,
            timeout_ms: 20_000,
            allow_pin_fallback: true,
        }
    }

    fn google_pay() -> GooglePayDeclaration {
        GooglePayDeclaration {
            merchant_id: "com.goya.registry.pay".into(),
            currency_code: "USD".into(),
            max_minor_units: 100_000,
        }
    }

    fn samsung_pay() -> SamsungPayDeclaration {
        SamsungPayDeclaration {
            merchant_id: "com.goya.registry.pay".into(),
            currency_code: "USD".into(),
            max_minor_units: 100_000,
        }
    }

    fn bluetooth_central() -> BluetoothCentralDeclaration {
        BluetoothCentralDeclaration {
            service_uuids: vec!["181A".to_owned()],
            max_scan_ms: 10_000,
            max_peripherals: 2,
            connection_timeout_ms: 10_000,
            max_reconnect_attempts: 2,
        }
    }

    fn bluetooth_gatt() -> BluetoothGattDeclaration {
        BluetoothGattDeclaration {
            characteristics: vec![
                crate::bluetooth_runtime::CharacteristicPolicy::new(
                    "2A6E".to_owned(),
                    crate::bluetooth_runtime::GattAccess::ReadWrite,
                    512,
                )
                .unwrap(),
                crate::bluetooth_runtime::CharacteristicPolicy::new(
                    "2A37".to_owned(),
                    crate::bluetooth_runtime::GattAccess::Write,
                    64,
                )
                .unwrap(),
            ],
            max_command_timeout_ms: 30_000,
        }
    }

    fn bluetooth_stream() -> BluetoothStreamDeclaration {
        BluetoothStreamDeclaration {
            characteristics: vec![
                crate::bluetooth_runtime::StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    crate::bluetooth_runtime::NotificationTransport::Notification,
                )
                .unwrap(),
                crate::bluetooth_runtime::StreamCharacteristic::new(
                    "2A37".to_owned(),
                    crate::bluetooth_runtime::NotificationTransport::Indication,
                )
                .unwrap(),
            ],
            max_notification_bytes: 256,
            max_buffered_notifications: 2,
        }
    }

    fn bluetooth_guard() -> BluetoothGuardDeclaration {
        BluetoothGuardDeclaration {
            usage_description: "Connects to your temperature sensor to show readings."
                .to_owned(),
            max_authorised_peripherals: 4,
            enforce_explicit_selection: true,
        }
    }

    fn bluetooth_continuity() -> BluetoothContinuityDeclaration {
        BluetoothContinuityDeclaration {
            max_associations: 4,
            max_subscriptions_per_association: 2,
            persist_across_launches: true,
        }
    }

    fn bluetooth_access() -> BluetoothAccessDeclaration {
        BluetoothAccessDeclaration {
            bindings: vec![
                crate::bluetooth_runtime::InputBinding::new(
                    crate::bluetooth_runtime::InputSource::AccessibilitySwitch,
                    crate::bluetooth_runtime::InputEventKind::SinglePress,
                    crate::bluetooth_runtime::SemanticAction::Select,
                    "Primary select",
                )
                .unwrap(),
                crate::bluetooth_runtime::InputBinding::new(
                    crate::bluetooth_runtime::InputSource::AccessibilitySwitch,
                    crate::bluetooth_runtime::InputEventKind::DoublePress,
                    crate::bluetooth_runtime::SemanticAction::NavigateNext,
                    "Move to the next element",
                )
                .unwrap(),
                crate::bluetooth_runtime::InputBinding::new(
                    crate::bluetooth_runtime::InputSource::AccessibilitySwitch,
                    crate::bluetooth_runtime::InputEventKind::LongPress,
                    crate::bluetooth_runtime::SemanticAction::Back,
                    "Return to the previous screen",
                )
                .unwrap(),
            ],
            debounce_ms: 500,
            long_press_threshold_ms: 400,
            double_press_window_ms: 300,
            intercept_system_managed: false,
        }
    }

    fn bluetooth_profile() -> BluetoothProfileDeclaration {
        BluetoothProfileDeclaration {
            profile_id: "com.goya.profile.test".to_owned(),
            category: crate::bluetooth_runtime::ProfileCategory::Battery,
            version: "1.0.0".to_owned(),
            required: vec![crate::bluetooth_runtime::ProfileCharacteristic::new(
                "2A19".to_owned(),
                crate::bluetooth_runtime::ProfileProperties::new(true, false, false, false),
                1,
            )
            .unwrap()],
            optional: vec![],
        }
    }







    #[test]
    fn touch_id_contract_registers_exactly_and_fails_closed() {
        let contract = crate::touch_id_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.biometry.touch_id.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.biometry"));
        assert!(!supported_contracts().contains(&"goya.hardware.biometry.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            touch_id: Some(touch_id()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(negotiated[0].artifact.recipe(), "apple-touch-id-bounded-attempt-gate");
        assert!(negotiated[0].artifact.canonical_ir().contains("sensor=touch_id\n"));
        assert!(negotiated[0].artifact.canonical_ir().contains("fallback=device_passcode\n"));
        assert!(negotiated[0].artifact.canonical_ir().contains("data_access=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let mut mac_host = host();
        mac_host.touch_id_target = TouchIdTarget::AppleSiliconMac;
        let mac_negotiated = negotiate(&request, mac_host).unwrap();
        assert!(mac_negotiated[0]
            .artifact
            .canonical_ir()
            .contains("target=apple_silicon_mac\n"));

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            touch_id: Some(touch_id()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "touch_id" })
        );

        let mut unsupported_target = host();
        unsupported_target.touch_id_target = TouchIdTarget::Browser;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::TouchId(
                crate::touch_id_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_sensor = host();
        no_sensor.touch_id_host.touch_id_sensor = false;
        assert_eq!(
            negotiate(&request, no_sensor),
            Err(NegotiationError::TouchId(
                crate::touch_id_caprichio::Diagnostic::MissingTouchIdSensor
            ))
        );

        let draft = Request {
            capabilities: vec!["hardware.biometry".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnknownCapability(
                "hardware.biometry".into()
            ))
        );
    }

    #[test]
    fn face_id_contract_registers_exactly_ios_only_and_fails_closed() {
        let contract = crate::face_id_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.biometry.face_id.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.biometry.face"));
        assert!(!supported_contracts().contains(&"goya.hardware.biometry.face_id.v2"));

        let request = Request {
            capabilities: vec![contract.into()],
            face_id: Some(face_id()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(negotiated[0].artifact.recipe(), "apple-face-id-bounded-attempt-gate");
        assert!(negotiated[0].artifact.canonical_ir().contains("sensor=face_id\n"));
        assert!(negotiated[0].artifact.canonical_ir().contains("attention=required\n"));
        assert!(negotiated[0].artifact.canonical_ir().contains("data_access=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        // Face ID hardware is not part of any Mac configuration.
        let mut mac_host = host();
        mac_host.face_id_target = FaceIdTarget::AppleSiliconMac;
        assert_eq!(
            negotiate(&request, mac_host),
            Err(NegotiationError::FaceId(
                crate::face_id_caprichio::Diagnostic::UnsupportedTarget
            ))
        );

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            face_id: Some(face_id()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "face_id" })
        );

        let mut no_camera = host();
        no_camera.face_id_host.camera_available = false;
        assert_eq!(
            negotiate(&request, no_camera),
            Err(NegotiationError::FaceId(
                crate::face_id_caprichio::Diagnostic::MissingCamera
            ))
        );
    }

    #[test]
    fn apple_pay_contract_registers_exactly_and_fails_closed() {
        let contract = crate::apple_pay_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.payments.apple_pay.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.payments"));
        assert!(!supported_contracts().contains(&"goya.hardware.payments.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            apple_pay: Some(apple_pay()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(negotiated[0].artifact.recipe(), "apple-pay-bounded-payment-request");
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("request=apple_pay\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("funds_moved=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            apple_pay: Some(apple_pay()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "apple_pay" })
        );

        let mut unsupported_target = host();
        unsupported_target.apple_pay_target = ApplePayTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::ApplePay(
                crate::apple_pay_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_credential = host();
        no_credential.apple_pay_host.secure_element_credential = false;
        assert_eq!(
            negotiate(&request, no_credential),
            Err(NegotiationError::ApplePay(
                crate::apple_pay_caprichio::Diagnostic::MissingSecureElementCredential
            ))
        );
    }

    #[test]
    fn windows_hello_google_pay_and_samsung_pay_contracts_lower_without_panicking() {
        let windows_hello_request = Request {
            capabilities: vec![crate::windows_hello_caprichio::CONTRACT.into()],
            windows_hello: Some(windows_hello()),
            ..Request::default()
        };
        let negotiated = negotiate(&windows_hello_request, host()).unwrap();
        assert_eq!(negotiated[0].id, crate::windows_hello_caprichio::CONTRACT);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "windows-hello-bounded-attempt-gate"
        );
        assert_eq!(negotiated, negotiate(&windows_hello_request, host()).unwrap());

        let google_pay_request = Request {
            capabilities: vec![crate::google_pay_caprichio::CONTRACT.into()],
            google_pay: Some(google_pay()),
            ..Request::default()
        };
        let negotiated = negotiate(&google_pay_request, host()).unwrap();
        assert_eq!(negotiated[0].id, crate::google_pay_caprichio::CONTRACT);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "google-pay-bounded-payment-request"
        );
        assert_eq!(negotiated, negotiate(&google_pay_request, host()).unwrap());

        let samsung_pay_request = Request {
            capabilities: vec![crate::samsung_pay_caprichio::CONTRACT.into()],
            samsung_pay: Some(samsung_pay()),
            ..Request::default()
        };
        let negotiated = negotiate(&samsung_pay_request, host()).unwrap();
        assert_eq!(negotiated[0].id, crate::samsung_pay_caprichio::CONTRACT);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "samsung-pay-bounded-payment-request"
        );
        assert_eq!(negotiated, negotiate(&samsung_pay_request, host()).unwrap());

        for (contract, kind) in [
            (
                crate::windows_hello_caprichio::CONTRACT,
                "windows_hello",
            ),
            (crate::google_pay_caprichio::CONTRACT, "google_pay"),
            (crate::samsung_pay_caprichio::CONTRACT, "samsung_pay"),
        ] {
            let missing = Request {
                capabilities: vec![contract.into()],
                ..Request::default()
            };
            assert_eq!(
                negotiate(&missing, host()),
                Err(NegotiationError::MissingDeclaration {
                    capability: contract
                })
            );
            let unexpected = match kind {
                "windows_hello" => Request {
                    windows_hello: Some(windows_hello()),
                    ..Request::default()
                },
                "google_pay" => Request {
                    google_pay: Some(google_pay()),
                    ..Request::default()
                },
                _ => Request {
                    samsung_pay: Some(samsung_pay()),
                    ..Request::default()
                },
            };
            assert_eq!(
                negotiate(&unexpected, host()),
                Err(NegotiationError::UnexpectedDeclaration { kind })
            );
        }
    }

    #[test]
    fn bluetooth_central_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.central.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.bluetooth"));
        assert!(!supported_contracts().contains(&"goya.hardware.bluetooth.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_central: Some(bluetooth_central()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-central-bounded-scan"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_central_scan\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("radio_driven=none\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("peripherals_connected=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_central: Some(bluetooth_central()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_central"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_central_target = BluetoothTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Bluetooth(
                crate::bluetooth_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut unpowered = host();
        unpowered.bluetooth_central_host.bluetooth_powered = false;
        assert_eq!(
            negotiate(&request, unpowered),
            Err(NegotiationError::Bluetooth(
                crate::bluetooth_caprichio::Diagnostic::MissingBluetoothPower
            ))
        );
        let mut unpermitted = host();
        unpermitted.bluetooth_central_host.bluetooth_permission = false;
        assert_eq!(
            negotiate(&request, unpermitted),
            Err(NegotiationError::Bluetooth(
                crate::bluetooth_caprichio::Diagnostic::MissingBluetoothPermission
            ))
        );
    }

    #[test]
    fn bluetooth_gatt_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_control_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.gatt.v1");
        assert!(supported_contracts().contains(&contract));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_gatt: Some(bluetooth_gatt()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-gatt-bounded-commands"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_gatt_bounded_commands\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("characteristics=2A6E:read_write:512,2A37:write:64\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("physical_execution=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_gatt: Some(bluetooth_gatt()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_gatt"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_gatt_target = BluetoothGattTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothGatt(
                crate::bluetooth_control_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut without_gatt = host();
        without_gatt.bluetooth_gatt_host.core_bluetooth_gatt = false;
        assert_eq!(
            negotiate(&request, without_gatt),
            Err(NegotiationError::BluetoothGatt(
                crate::bluetooth_control_caprichio::Diagnostic::MissingCoreBluetoothGatt
            ))
        );
    }

    #[test]
    fn bluetooth_stream_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_stream_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.stream.v1");
        assert!(supported_contracts().contains(&contract));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_stream: Some(bluetooth_stream()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-stream-bounded-notifications"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_stream_bounded_notifications\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("characteristics=2A6E:notification,2A37:indication\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("delivery_guarantee=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_stream: Some(bluetooth_stream()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_stream"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_stream_target = BluetoothStreamTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothStream(
                crate::bluetooth_stream_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        // The mixed declaration uses both transports: either missing
        // facility fails closed.
        let mut without_notifications = host();
        without_notifications
            .bluetooth_stream_host
            .characteristic_notifications = false;
        assert_eq!(
            negotiate(&request, without_notifications),
            Err(NegotiationError::BluetoothStream(
                crate::bluetooth_stream_caprichio::Diagnostic::MissingCharacteristicNotifications
            ))
        );
        let mut without_indications = host();
        without_indications
            .bluetooth_stream_host
            .characteristic_indications = false;
        assert_eq!(
            negotiate(&request, without_indications),
            Err(NegotiationError::BluetoothStream(
                crate::bluetooth_stream_caprichio::Diagnostic::MissingCharacteristicIndications
            ))
        );
        // A notifications-only declaration does not require indications.
        let notifications_only = Request {
            capabilities: vec![contract.into()],
            bluetooth_stream: Some(BluetoothStreamDeclaration {
                characteristics: vec![crate::bluetooth_runtime::StreamCharacteristic::new(
                    "2A6E".to_owned(),
                    crate::bluetooth_runtime::NotificationTransport::Notification,
                )
                .unwrap()],
                ..bluetooth_stream()
            }),
            ..Request::default()
        };
        let mut without_indications = host();
        without_indications
            .bluetooth_stream_host
            .characteristic_indications = false;
        let result = negotiate(&notifications_only, without_indications);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn bluetooth_guard_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_guard_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.guard.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"goya.hardware.bluetooth.guard"));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_guard: Some(bluetooth_guard()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-guard-bounded-authorisation"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_guard_bounded_authorisation\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("explicit_selection=required\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("usage_keys_generated=none\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("authorisations_granted=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_guard: Some(bluetooth_guard()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_guard"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_guard_target = BluetoothGuardTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothGuard(
                crate::bluetooth_guard_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut without_query = host();
        without_query.bluetooth_guard_host.bluetooth_permission_query = false;
        assert_eq!(
            negotiate(&request, without_query),
            Err(NegotiationError::BluetoothGuard(
                crate::bluetooth_guard_caprichio::Diagnostic::MissingBluetoothPermissionQuery
            ))
        );
        let mut without_store = host();
        without_store.bluetooth_guard_host.bluetooth_authorisation_store = false;
        assert_eq!(
            negotiate(&request, without_store),
            Err(NegotiationError::BluetoothGuard(
                crate::bluetooth_guard_caprichio::Diagnostic::MissingBluetoothAuthorisationStore
            ))
        );
    }

    #[test]
    fn bluetooth_continuity_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_continuity_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.continuity.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"goya.hardware.bluetooth.continuity"));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_continuity: Some(bluetooth_continuity()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-continuity-bounded-restoration"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_continuity_bounded_restoration\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("relaunch_promised=none\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("restoration_planned=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_continuity: Some(bluetooth_continuity()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_continuity"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_continuity_target = BluetoothContinuityTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothContinuity(
                crate::bluetooth_continuity_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        // Cross-launch persistence requires restoration and storage.
        let mut without_restoration = host();
        without_restoration
            .bluetooth_continuity_host
            .bluetooth_state_restoration = false;
        assert_eq!(
            negotiate(&request, without_restoration),
            Err(NegotiationError::BluetoothContinuity(
                crate::bluetooth_continuity_caprichio::Diagnostic::MissingBluetoothStateRestoration
            ))
        );
        let mut without_storage = host();
        without_storage.bluetooth_continuity_host.persistent_storage = false;
        assert_eq!(
            negotiate(&request, without_storage),
            Err(NegotiationError::BluetoothContinuity(
                crate::bluetooth_continuity_caprichio::Diagnostic::MissingPersistentStorage
            ))
        );
        // A session-scoped declaration requires neither.
        let session_scoped = Request {
            capabilities: vec![contract.into()],
            bluetooth_continuity: Some(BluetoothContinuityDeclaration {
                persist_across_launches: false,
                ..bluetooth_continuity()
            }),
            ..Request::default()
        };
        assert!(negotiate(&session_scoped, without_restoration).is_ok());
        assert!(negotiate(&session_scoped, without_storage).is_ok());
    }

    #[test]
    fn bluetooth_access_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_access_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.access.v1");
        assert!(supported_contracts().contains(&contract));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_access: Some(bluetooth_access()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-access-bounded-mapping"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_access_bounded_mapping\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("system_managed_devices=untouched\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("protected_streams_intercepted=none\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_access: Some(bluetooth_access()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_access"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_access_target = BluetoothAccessTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothAccess(
                crate::bluetooth_access_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut without_surface = host();
        without_surface
            .bluetooth_access_host
            .accessibility_event_surface = false;
        assert_eq!(
            negotiate(&request, without_surface),
            Err(NegotiationError::BluetoothAccess(
                crate::bluetooth_access_caprichio::Diagnostic::MissingAccessibilityEventSurface
            ))
        );
        let mut without_labels = host();
        without_labels
            .bluetooth_access_host
            .accessible_labels_supported = false;
        assert_eq!(
            negotiate(&request, without_labels),
            Err(NegotiationError::BluetoothAccess(
                crate::bluetooth_access_caprichio::Diagnostic::MissingAccessibleLabels
            ))
        );
        // A declaration that intercepts system-managed streams is rejected.
        let intercepting = Request {
            capabilities: vec![contract.into()],
            bluetooth_access: Some(BluetoothAccessDeclaration {
                intercept_system_managed: true,
                ..bluetooth_access()
            }),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&intercepting, host()),
            Err(NegotiationError::BluetoothAccess(
                crate::bluetooth_access_caprichio::Diagnostic::SystemManagedDevicesProtected
            ))
        );
    }

    #[test]
    fn bluetooth_profile_contract_registers_exactly_and_fails_closed() {
        let contract = crate::bluetooth_profile_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.bluetooth.profile.v1");
        assert!(supported_contracts().contains(&contract));

        let request = Request {
            capabilities: vec![contract.into()],
            bluetooth_profile: Some(bluetooth_profile()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-ble-profile-bounded-classification"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("session=ble_profile_bounded_classification\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("devices_classified=none\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("spec_validation=category_invariants_only\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            bluetooth_profile: Some(bluetooth_profile()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "bluetooth_profile"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.bluetooth_profile_target = BluetoothProfileTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::BluetoothProfile(
                crate::bluetooth_profile_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        // A battery profile without its required 2A19 read is rejected.
        let invalid = Request {
            capabilities: vec![contract.into()],
            bluetooth_profile: Some(BluetoothProfileDeclaration {
                required: vec![crate::bluetooth_runtime::ProfileCharacteristic::new(
                    "2A19".to_owned(),
                    crate::bluetooth_runtime::ProfileProperties::new(false, true, false, false),
                    1,
                )
                .unwrap()],
                ..bluetooth_profile()
            }),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&invalid, host()),
            Err(NegotiationError::BluetoothProfile(
                crate::bluetooth_profile_caprichio::Diagnostic::CategoryInvariantViolated
            ))
        );
    }

    #[test]
    fn ios_keychain_contract_registers_exactly_and_fails_closed() {
        let contract = crate::keychain_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.secure_storage.keychain.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.keychain"));
        assert!(!supported_contracts().contains(&"goya.hardware.keychain.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            keychain: Some(keychain()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-keychain-namespace-bounded-kv"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("target=ios\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("store=apple_keychain\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let mut mac_host = host();
        mac_host.keychain_target = KeychainTarget::AppleSiliconMac;
        let mac_negotiated = negotiate(&request, mac_host).unwrap();
        assert!(mac_negotiated[0]
            .artifact
            .canonical_ir()
            .contains("target=apple_silicon_mac\n"));

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            keychain: Some(keychain()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "keychain" })
        );

        let mut unsupported_target = host();
        unsupported_target.keychain_target = KeychainTarget::Browser;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Keychain(
                crate::keychain_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_enclave = host();
        no_enclave.keychain_host.hardware_backed_keys = false;
        assert_eq!(
            negotiate(&request, no_enclave),
            Err(NegotiationError::Keychain(
                crate::keychain_caprichio::Diagnostic::MissingHardwareBackedKeys
            ))
        );

        let draft = Request {
            capabilities: vec!["hardware.keychain".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnknownCapability(
                "hardware.keychain".into()
            ))
        );
    }

    #[test]
    fn apple_silicon_metal_contract_registers_exactly_and_fails_closed() {
        let contract = crate::metal_caprichio::CONTRACT;
        assert_eq!(contract, "goya.hardware.gpu_compute.apple_silicon_metal.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"hardware.gpu_compute"));
        assert!(!supported_contracts().contains(&"goya.hardware.gpu_compute.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            metal_compute: Some(metal_dispatch()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "apple-silicon-metal-checked-dispatch"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("target=apple_silicon_metal\n"));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("scheduling=none_guaranteed\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            metal_compute: Some(metal_dispatch()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "metal_compute"
            })
        );

        let mut unsupported_target = host();
        unsupported_target.metal_target = MetalTarget::CrossPlatform;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Metal(
                crate::metal_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_queue = host();
        no_queue.metal_runtime.command_queue = false;
        assert_eq!(
            negotiate(&request, no_queue),
            Err(NegotiationError::Metal(
                crate::metal_caprichio::Diagnostic::MissingCommandQueue
            ))
        );

        let draft = Request {
            capabilities: vec!["hardware.gpu_compute".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnknownCapability(
                "hardware.gpu_compute".into()
            ))
        );
    }

    #[test]
    fn mixed_request_is_sorted_and_artifacts_keep_contract_identity() {
        let request = Request {
            capabilities: vec![
                crate::storage::CONTRACT.into(),
                crate::memory_caprichio::CONTRACT.into(),
                crate::CONTRACT.into(),
                crate::data_caprichio::CONTRACT.into(),
                crate::compute_caprichio::CONTRACT.into(),
                crate::progress_caprichio::CONTRACT.into(),
                crate::checksum_caprichio::CONTRACT.into(),
                crate::filesystem_caprichio::CONTRACT.into(),
                crate::retry_caprichio::CONTRACT.into(),
                crate::http_caprichio::CONTRACT.into(),
                crate::stream_caprichio::CONTRACT.into(),
                crate::compression_caprichio::CONTRACT.into(),
            ],
            network: Some(network()),
            storage: Some(storage()),
            memory: Some(memory()),
            compute: Some(compute()),
            data: Some(data()),
            progress: Some(progress()),
            checksum: Some(checksum()),
            retry: Some(retry()),
            filesystem: Some(filesystem()),
            http: Some(http()),
            stream: Some(stream()),
            compression: Some(compression()),
            batching: None,
            deadline: None,
            csv: None,
            priority: None,
            parallelism: None,
            input_validation: None,
            keychain: None,
            metal_compute: None,
            touch_id: None,
            face_id: None,
            apple_pay: None,
            windows_hello: None,
            google_pay: None,
            samsung_pay: None,
            bluetooth_central: None,
            bluetooth_gatt: None,
            bluetooth_stream: None,
            bluetooth_guard: None,
            bluetooth_continuity: None,
            bluetooth_access: None,
            bluetooth_profile: None,
        };
        let result = negotiate(&request, host()).unwrap();
        assert_eq!(
            result.iter().map(|v| v.id).collect::<Vec<_>>(),
            [
                crate::progress_caprichio::CONTRACT,
                crate::retry_caprichio::CONTRACT,
                crate::compute_caprichio::CONTRACT,
                crate::data_caprichio::CONTRACT,
                crate::checksum_caprichio::CONTRACT,
                crate::compression_caprichio::CONTRACT,
                crate::stream_caprichio::CONTRACT,
                crate::filesystem_caprichio::CONTRACT,
                crate::memory_caprichio::CONTRACT,
                crate::CONTRACT,
                crate::http_caprichio::CONTRACT,
                crate::storage::CONTRACT
            ]
        );
        assert_eq!(
            result[0].artifact.contract(),
            crate::progress_caprichio::CONTRACT
        );
        assert_eq!(
            result[1].artifact.contract(),
            crate::retry_caprichio::CONTRACT
        );
        assert_eq!(
            result[2].artifact.contract(),
            crate::compute_caprichio::CONTRACT
        );
        assert_eq!(
            result[3].artifact.contract(),
            crate::data_caprichio::CONTRACT
        );
        assert_eq!(
            result[4].artifact.contract(),
            crate::checksum_caprichio::CONTRACT
        );
        assert_eq!(
            result[5].artifact.contract(),
            crate::compression_caprichio::CONTRACT
        );
        assert_eq!(
            result[6].artifact.contract(),
            crate::stream_caprichio::CONTRACT
        );
        assert_eq!(
            result[7].artifact.contract(),
            crate::filesystem_caprichio::CONTRACT
        );
        assert_eq!(
            result[8].artifact.contract(),
            crate::memory_caprichio::CONTRACT
        );
        assert_eq!(result[9].artifact.contract(), crate::CONTRACT);
        assert_eq!(
            result[10].artifact.contract(),
            crate::http_caprichio::CONTRACT
        );
        assert_eq!(result[11].artifact.contract(), crate::storage::CONTRACT);
        assert!(result[8]
            .artifact
            .source()
            .contains("max_total_bytes: 1048576usize"));
        assert_eq!(result, negotiate(&request, host()).unwrap());
    }

    #[test]
    fn all_draft_catalog_capabilities_remain_unsupported() {
        for definition in catalog::definitions() {
            let draft = Request {
                capabilities: vec![definition.id.into()],
                ..Request::default()
            };
            assert_eq!(
                negotiate(&draft, host()),
                Err(NegotiationError::UnsupportedCapability(
                    definition.id.into()
                )),
                "draft {} was treated as a conformant implementation",
                definition.id
            );
        }
        let unknown = Request {
            capabilities: vec!["future.unknown".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unknown, host()),
            Err(NegotiationError::UnknownCapability("future.unknown".into()))
        );
    }

    #[test]
    fn supported_contracts_lists_only_exact_implemented_contracts_in_order() {
        assert!(supported_contracts()
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert_eq!(
            supported_contracts(),
            &[
                "goya.compute.batching.process_local.v1",
                "goya.compute.deadline.cooperative_process_local.v1",
                "goya.compute.parallelism.process_local_rust.v1",
                "goya.compute.priority_queue.process_local.v1",
                "goya.compute.progress.fixed_total_process_local.v1",
                "goya.compute.retry_policy.bounded_process_local.v1",
                "goya.compute.work_queue.bounded_process_local.v1",
                "goya.data.binary_envelope.v1",
                "goya.data.checksum.fnv1a64.v1",
                "goya.data.compression.rle.v1",
                "goya.data.csv.bounded_crlf.v1",
                "goya.data.stream.process_local_bounded.v1",
                "goya.filesystem.read_file.v1",
                "goya.hardware.biometry.face_id.v1",
                "goya.hardware.biometry.touch_id.v1",
                "goya.hardware.biometry.windows_hello.v1",
                "goya.hardware.bluetooth.access.v1",
                "goya.hardware.bluetooth.central.v1",
                "goya.hardware.bluetooth.continuity.v1",
                "goya.hardware.bluetooth.gatt.v1",
                "goya.hardware.bluetooth.guard.v1",
                "goya.hardware.bluetooth.profile.v1",
                "goya.hardware.bluetooth.stream.v1",
                "goya.hardware.gpu_compute.apple_silicon_metal.v1",
                "goya.hardware.payments.apple_pay.v1",
                "goya.hardware.payments.google_pay.v1",
                "goya.hardware.payments.samsung_pay.v1",
                "goya.hardware.secure_storage.keychain.v1",
                "goya.input.ascii_set_validation.process_local.v1",
                "goya.memory.buffers.process_local.v1",
                "goya.network.fetch.v1",
                "goya.network.http.browser_fetch_cors_limited.v1",
                "goya.storage.kv.v1",
            ]
        );
        for definition in catalog::definitions() {
            assert!(!supported_contracts().contains(&definition.id));
            let proposed = definition.proposed_contract();
            assert!(
                !supported_contracts().contains(&proposed.as_str()),
                "{proposed} collides with an implemented recipe"
            );
        }
    }

    #[test]
    fn process_local_batching_contract_is_exact_deterministic_and_fail_closed() {
        let contract = crate::batching_caprichio::CONTRACT;
        assert_eq!(contract, "goya.compute.batching.process_local.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"compute.batching"));
        assert!(!supported_contracts().contains(&"goya.compute.batching.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            batching: Some(batching()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "process-local-bounded-batch-builder"
        );
        assert!(negotiated[0]
            .artifact
            .source()
            .contains("Batcher::new(4usize, 1024usize, 256usize)"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            batching: Some(batching()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "batching" })
        );

        let mut unsupported_target = host();
        unsupported_target.batching_target = BatchTarget::CrossProcess;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Batching(
                crate::batching_caprichio::Diagnostic::UnsupportedTarget
            ))
        );

        let draft = Request {
            capabilities: vec!["compute.batching".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability(
                "compute.batching".into()
            ))
        );
    }

    #[test]
    fn cooperative_deadline_contract_is_registered_exactly_and_checks_host() {
        let contract = crate::deadline_caprichio::CONTRACT;
        assert_eq!(
            contract,
            "goya.compute.deadline.cooperative_process_local.v1"
        );
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"compute.deadline"));
        assert!(!supported_contracts().contains(&"goya.compute.deadline.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            deadline: Some(deadline()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "cooperative-process-local-monotonic-deadline"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("behavior=cooperative_check_only\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let mut no_clock = host();
        no_clock.deadline_runtime.monotonic_clock = false;
        assert_eq!(
            negotiate(&request, no_clock),
            Err(NegotiationError::Deadline(
                crate::deadline_caprichio::Diagnostic::MissingMonotonicClock
            ))
        );
        let mut unsupported_target = host();
        unsupported_target.deadline_target = DeadlineTarget::CrossProcess;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Deadline(
                crate::deadline_caprichio::Diagnostic::UnsupportedTarget
            ))
        );

        let draft = Request {
            capabilities: vec!["compute.deadlines".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability(
                "compute.deadlines".into()
            ))
        );
    }

    #[test]
    fn browser_http_contract_is_registered_exactly_and_separately_from_draft_http() {
        let contract = crate::http_caprichio::CONTRACT;
        assert_eq!(contract, "goya.network.http.browser_fetch_cors_limited.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"goya.network.http.v1"));

        let request = Request {
            capabilities: vec![contract.into()],
            http: Some(http()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            "browser-fetch-cors-limited-http"
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("response_headers=cors_exposed_only\n"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let draft = Request {
            capabilities: vec!["network.http".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability(
                "network.http".into()
            ))
        );

        let mut unsupported_target = host();
        unsupported_target.http_target = HttpTarget::Node;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Http(
                crate::http_caprichio::Diagnostic::UnsupportedTarget
            ))
        );

        let mut missing_headers = host();
        missing_headers.http_runtime.response_headers = false;
        assert_eq!(
            negotiate(&request, missing_headers),
            Err(NegotiationError::Http(
                crate::http_caprichio::Diagnostic::MissingResponseHeaders
            ))
        );

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::http_caprichio::CONTRACT
            })
        );
        let unexpected = Request {
            http: Some(http()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "http" })
        );
    }

    #[test]
    fn stream_and_compression_register_only_their_exact_contracts_and_fail_closed() {
        let stream_id = crate::stream_caprichio::CONTRACT;
        let compression_id = crate::compression_caprichio::CONTRACT;
        assert!(supported_contracts().contains(&stream_id));
        assert!(supported_contracts().contains(&compression_id));
        assert!(!supported_contracts().contains(&"goya.data.stream.v1"));
        assert!(!supported_contracts().contains(&"goya.data.compression.v1"));

        let request = Request {
            capabilities: vec![stream_id.into(), compression_id.into()],
            stream: Some(stream()),
            compression: Some(compression()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(
            negotiated.iter().map(|item| item.id).collect::<Vec<_>>(),
            [compression_id, stream_id]
        );
        assert_eq!(negotiated[0].artifact.contract(), compression_id);
        assert_eq!(
            negotiated[0].artifact.recipe(),
            crate::compression_caprichio::RECIPE
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("compression_ratio_guarantee=none\n"));
        assert_eq!(negotiated[1].artifact.contract(), stream_id);
        assert_eq!(
            negotiated[1].artifact.recipe(),
            "process-local-bounded-byte-stream"
        );
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing_stream = Request {
            capabilities: vec![stream_id.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_stream, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::stream_caprichio::CONTRACT
            })
        );
        let missing_compression = Request {
            capabilities: vec![compression_id.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_compression, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::compression_caprichio::CONTRACT
            })
        );

        let mut unsupported_stream_target = host();
        unsupported_stream_target.stream_target = StreamTarget::CrossProcess;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![stream_id.into()],
                    stream: Some(stream()),
                    ..Request::default()
                },
                unsupported_stream_target
            ),
            Err(NegotiationError::Stream(
                crate::stream_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut missing_stream_mutex = host();
        missing_stream_mutex.stream_runtime.standard_mutex = false;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![stream_id.into()],
                    stream: Some(stream()),
                    ..Request::default()
                },
                missing_stream_mutex
            ),
            Err(NegotiationError::Stream(
                crate::stream_caprichio::Diagnostic::MissingStandardMutex
            ))
        );

        let mut unsupported_compression_target = host();
        unsupported_compression_target.compression_target = CompressionTarget::Browser;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![compression_id.into()],
                    compression: Some(compression()),
                    ..Request::default()
                },
                unsupported_compression_target
            ),
            Err(NegotiationError::Compression(
                crate::compression_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut missing_compression_allocator = host();
        missing_compression_allocator
            .compression_host
            .standard_allocator = false;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![compression_id.into()],
                    compression: Some(compression()),
                    ..Request::default()
                },
                missing_compression_allocator
            ),
            Err(NegotiationError::Compression(
                crate::compression_caprichio::Diagnostic::MissingStandardAllocator
            ))
        );
    }

    #[test]
    fn csv_registers_exact_contract_and_mixed_negotiation_is_deterministic() {
        let csv_id = crate::csv_caprichio::CONTRACT;
        assert!(supported_contracts().contains(&csv_id));
        assert!(!supported_contracts().contains(&"data.csv"));
        assert!(!supported_contracts().contains(&"goya.data.csv.v1"));

        let request = Request {
            capabilities: vec![crate::compression_caprichio::CONTRACT.into(), csv_id.into()],
            compression: Some(compression()),
            csv: Some(csv()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(
            negotiated.iter().map(|item| item.id).collect::<Vec<_>>(),
            [crate::compression_caprichio::CONTRACT, csv_id]
        );
        let artifact = &negotiated[1].artifact;
        assert_eq!(artifact.contract(), csv_id);
        assert_eq!(artifact.recipe(), crate::csv_caprichio::RECIPE);
        assert_eq!(
            artifact.recipe_version(),
            crate::csv_caprichio::RECIPE_VERSION
        );
        assert!(artifact
            .canonical_ir()
            .contains("dialect=bounded-utf8-crlf-v1\n"));
        assert!(artifact.source().contains("crate::csv_caprichio::encode"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![csv_id.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration { capability: csv_id })
        );
        let unexpected = Request {
            csv: Some(csv()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "csv" })
        );

        let mut unsupported_target = host();
        unsupported_target.csv_target = CsvTarget::Browser;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![csv_id.into()],
                    csv: Some(csv()),
                    ..Request::default()
                },
                unsupported_target
            ),
            Err(NegotiationError::Csv(
                crate::csv_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_utf8 = host();
        no_utf8.csv_host.utf8_strings = false;
        assert_eq!(
            negotiate(
                &Request {
                    capabilities: vec![csv_id.into()],
                    csv: Some(csv()),
                    ..Request::default()
                },
                no_utf8
            ),
            Err(NegotiationError::Csv(
                crate::csv_caprichio::Diagnostic::MissingUtf8Strings
            ))
        );

        let draft = Request {
            capabilities: vec!["data.csv".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability("data.csv".into()))
        );
    }

    #[test]
    fn priority_queue_registers_exact_contract_and_rejects_unsupported_profiles() {
        let contract = crate::priority_caprichio::CONTRACT;
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"compute.priorities"));
        assert!(!supported_contracts().contains(&"goya.compute.priority.v1"));
        let request = Request {
            capabilities: vec![crate::deadline_caprichio::CONTRACT.into(), contract.into()],
            deadline: Some(deadline()),
            priority: Some(priority()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(
            negotiated.iter().map(|item| item.id).collect::<Vec<_>>(),
            [crate::deadline_caprichio::CONTRACT, contract]
        );
        let artifact = &negotiated[1].artifact;
        assert_eq!(artifact.contract(), contract);
        assert_eq!(artifact.recipe(), crate::priority_caprichio::RECIPE);
        assert_eq!(
            artifact.recipe_version(),
            crate::priority_caprichio::RECIPE_VERSION
        );
        assert!(artifact
            .canonical_ir()
            .contains("selection=highest_signed_priority_then_fifo\n"));
        assert!(artifact.source().contains("make_priority_queue"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            priority: Some(priority()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "priority" })
        );
        let mut unsupported_target = host();
        unsupported_target.priority_target = PriorityTarget::CrossProcess;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Priority(
                crate::priority_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_mutex = host();
        no_mutex.priority_host.standard_mutex = false;
        assert_eq!(
            negotiate(&request, no_mutex),
            Err(NegotiationError::Priority(
                crate::priority_caprichio::Diagnostic::MissingStandardMutex
            ))
        );
        let draft = Request {
            capabilities: vec!["compute.priorities".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability(
                "compute.priorities".into()
            ))
        );
    }

    #[test]
    fn ascii_input_validation_registers_only_its_narrow_contract() {
        let contract = crate::input_validation_caprichio::CONTRACT;
        assert_eq!(contract, "goya.input.ascii_set_validation.process_local.v1");
        assert!(supported_contracts().contains(&contract));
        assert!(!supported_contracts().contains(&"security.input_validation"));
        assert!(!supported_contracts().contains(&"goya.security.input_validation.v1"));
        let request = Request {
            capabilities: vec![
                crate::filesystem_caprichio::CONTRACT.into(),
                contract.into(),
            ],
            filesystem: Some(filesystem()),
            input_validation: Some(input_validation()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(
            negotiated.iter().map(|item| item.id).collect::<Vec<_>>(),
            [crate::filesystem_caprichio::CONTRACT, contract]
        );
        let artifact = &negotiated[1].artifact;
        assert_eq!(artifact.contract(), contract);
        assert_eq!(artifact.recipe(), crate::input_validation_caprichio::RECIPE);
        assert_eq!(
            artifact.recipe_version(),
            crate::input_validation_caprichio::RECIPE_VERSION
        );
        assert!(artifact.canonical_ir().contains("security_claims=none\n"));
        assert!(artifact
            .canonical_ir()
            .contains("allowed_ascii_hex=30313233343536373839\n"));
        assert!(artifact.source().contains("validate_text"));
        assert!(artifact.source().contains("MIN_TEXT_BYTES: usize = 1usize"));
        assert_eq!(negotiated, negotiate(&request, host()).unwrap());

        let missing = Request {
            capabilities: vec![contract.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: contract
            })
        );
        let unexpected = Request {
            input_validation: Some(input_validation()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&unexpected, host()),
            Err(NegotiationError::UnexpectedDeclaration {
                kind: "input_validation"
            })
        );
        let mut unsupported_target = host();
        unsupported_target.input_validation_target = InputValidationTarget::Browser;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::InputValidation(
                crate::input_validation_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_utf8 = host();
        no_utf8.input_validation_host.utf8_strings = false;
        assert_eq!(
            negotiate(&request, no_utf8),
            Err(NegotiationError::InputValidation(
                crate::input_validation_caprichio::Diagnostic::MissingUtf8Strings
            ))
        );
        let draft = Request {
            capabilities: vec!["security.input_validation".into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&draft, host()),
            Err(NegotiationError::UnsupportedCapability(
                "security.input_validation".into()
            ))
        );
    }

    #[test]
    fn rejects_duplicates_and_declaration_mismatches() {
        let duplicate = Request {
            capabilities: vec![crate::CONTRACT.into(), crate::CONTRACT.into()],
            ..Request::default()
        };
        assert!(matches!(
            negotiate(&duplicate, host()),
            Err(NegotiationError::DuplicateCapability(_))
        ));
        let missing = Request {
            capabilities: vec![crate::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::CONTRACT
            })
        );
        let extra = Request {
            network: Some(network()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "network" })
        );
        let missing_memory = Request {
            capabilities: vec![crate::memory_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_memory, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::memory_caprichio::CONTRACT
            })
        );
        let extra_memory = Request {
            memory: Some(memory()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_memory, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "memory" })
        );
        let missing_compute = Request {
            capabilities: vec![crate::compute_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_compute, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::compute_caprichio::CONTRACT
            })
        );
        let extra_data = Request {
            data: Some(data()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_data, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "data" })
        );
        let missing_data = Request {
            capabilities: vec![crate::data_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_data, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::data_caprichio::CONTRACT
            })
        );
        let extra_compute = Request {
            compute: Some(compute()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_compute, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "compute" })
        );
        let missing_progress = Request {
            capabilities: vec![crate::progress_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_progress, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::progress_caprichio::CONTRACT
            })
        );
        let extra_checksum = Request {
            checksum: Some(checksum()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_checksum, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "checksum" })
        );
        let missing_checksum = Request {
            capabilities: vec![crate::checksum_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_checksum, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::checksum_caprichio::CONTRACT
            })
        );
        let extra_progress = Request {
            progress: Some(progress()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_progress, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "progress" })
        );
        let missing_retry = Request {
            capabilities: vec![crate::retry_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_retry, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::retry_caprichio::CONTRACT
            })
        );
        let extra_retry = Request {
            retry: Some(retry()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_retry, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "retry" })
        );
        let missing_filesystem = Request {
            capabilities: vec![crate::filesystem_caprichio::CONTRACT.into()],
            ..Request::default()
        };
        assert_eq!(
            negotiate(&missing_filesystem, host()),
            Err(NegotiationError::MissingDeclaration {
                capability: crate::filesystem_caprichio::CONTRACT
            })
        );
        let extra_filesystem = Request {
            filesystem: Some(filesystem()),
            ..Request::default()
        };
        assert_eq!(
            negotiate(&extra_filesystem, host()),
            Err(NegotiationError::UnexpectedDeclaration { kind: "filesystem" })
        );
    }

    #[test]
    fn compute_and_data_contracts_require_supported_host_profiles() {
        let compute_request = Request {
            capabilities: vec![crate::compute_caprichio::CONTRACT.into()],
            compute: Some(compute()),
            ..Request::default()
        };
        let negotiated = negotiate(&compute_request, host()).unwrap();
        assert_eq!(negotiated[0].id, crate::compute_caprichio::CONTRACT);
        assert!(negotiated[0]
            .artifact
            .source()
            .contains("BoundedQueue::<u8>::new(64)"));
        let mut unsupported_compute = host();
        unsupported_compute.compute_target = ComputeTarget::CrossProcess;
        assert_eq!(
            negotiate(&compute_request, unsupported_compute),
            Err(NegotiationError::Compute(
                crate::compute_caprichio::Diagnostic::UnsupportedTarget
            ))
        );

        let data_request = Request {
            capabilities: vec![crate::data_caprichio::CONTRACT.into()],
            data: Some(data()),
            ..Request::default()
        };
        let negotiated = negotiate(&data_request, host()).unwrap();
        assert_eq!(negotiated[0].id, crate::data_caprichio::CONTRACT);
        assert!(negotiated[0]
            .artifact
            .source()
            .contains("MAX_PAYLOAD_BYTES"));
        let mut unsupported_data = host();
        unsupported_data.data_host.byte_slices = false;
        assert_eq!(
            negotiate(&data_request, unsupported_data),
            Err(NegotiationError::Data(crate::data_caprichio::Diagnostic(
                "target runtime requires byte-slice support"
            )))
        );
    }

    #[test]
    fn progress_checksum_and_retry_contracts_fail_closed_on_host_requirements() {
        let progress_request = Request {
            capabilities: vec![crate::progress_caprichio::CONTRACT.into()],
            progress: Some(progress()),
            ..Request::default()
        };
        let progress_artifact = negotiate(&progress_request, host()).unwrap();
        assert_eq!(progress_artifact[0].id, crate::progress_caprichio::CONTRACT);
        assert!(progress_artifact[0]
            .artifact
            .source()
            .contains("Progress::new(100u64)"));
        let mut no_progress_memory = host();
        no_progress_memory.progress_runtime.process_local_memory = false;
        assert_eq!(
            negotiate(&progress_request, no_progress_memory),
            Err(NegotiationError::Progress(
                crate::progress_caprichio::Diagnostic::MissingProcessLocalMemory
            ))
        );

        let checksum_request = Request {
            capabilities: vec![crate::checksum_caprichio::CONTRACT.into()],
            checksum: Some(checksum()),
            ..Request::default()
        };
        let checksum_artifact = negotiate(&checksum_request, host()).unwrap();
        assert_eq!(checksum_artifact[0].id, crate::checksum_caprichio::CONTRACT);
        assert!(checksum_artifact[0]
            .artifact
            .source()
            .contains("data_runtime::checksum"));
        let mut no_checksum_support = host();
        no_checksum_support.checksum_host.u64_arithmetic = false;
        assert_eq!(
            negotiate(&checksum_request, no_checksum_support),
            Err(NegotiationError::Checksum(
                crate::checksum_caprichio::Diagnostic("target requires 64-bit unsigned arithmetic")
            ))
        );

        let retry_request = Request {
            capabilities: vec![crate::retry_caprichio::CONTRACT.into()],
            retry: Some(retry()),
            ..Request::default()
        };
        let retry_artifact = negotiate(&retry_request, host()).unwrap();
        assert_eq!(retry_artifact[0].id, crate::retry_caprichio::CONTRACT);
        assert!(retry_artifact[0]
            .artifact
            .source()
            .contains("RetryPolicy::new(4"));
        let mut no_retry_duration = host();
        no_retry_duration.retry_runtime.standard_duration = false;
        assert_eq!(
            negotiate(&retry_request, no_retry_duration),
            Err(NegotiationError::Retry(
                crate::retry_caprichio::Diagnostic::MissingStandardDuration
            ))
        );
    }

    #[test]
    fn filesystem_negotiation_maps_only_the_narrow_contract_and_checks_host() {
        let request = Request {
            capabilities: vec![crate::filesystem_caprichio::CONTRACT.into()],
            filesystem: Some(filesystem()),
            ..Request::default()
        };
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, crate::filesystem_caprichio::CONTRACT);
        assert_eq!(
            negotiated[0].artifact.contract(),
            crate::filesystem_caprichio::CONTRACT
        );
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("root_isolation=none\n"));
        assert!(negotiated[0]
            .artifact
            .source()
            .contains("read_bounded_file(path, MAX_FILE_BYTES)"));

        let mut unsupported_target = host();
        unsupported_target.filesystem_target = FileTarget::CrossProcess;
        assert_eq!(
            negotiate(&request, unsupported_target),
            Err(NegotiationError::Filesystem(
                crate::filesystem_caprichio::Diagnostic::UnsupportedTarget
            ))
        );
        let mut no_file_read = host();
        no_file_read.filesystem_host.file_read = false;
        assert_eq!(
            negotiate(&request, no_file_read),
            Err(NegotiationError::Filesystem(
                crate::filesystem_caprichio::Diagnostic::MissingFileRead
            ))
        );
    }

    #[test]
    fn memory_is_the_only_supported_memory_contract_and_requires_a_matching_host() {
        let request = Request {
            capabilities: vec![crate::memory_caprichio::CONTRACT.into()],
            memory: Some(memory()),
            ..Request::default()
        };
        let result = negotiate(&request, host()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, crate::memory_caprichio::CONTRACT);
        assert_eq!(
            result[0].artifact.contract(),
            crate::memory_caprichio::CONTRACT
        );
        let mut unsupported = host();
        unsupported.memory_target = MemoryTarget::Native;
        assert_eq!(
            negotiate(&request, unsupported),
            Err(NegotiationError::Memory(
                crate::memory_caprichio::Diagnostic(
                    "only the process-local memory target is supported"
                )
            ))
        );
        let mut missing_runtime = host();
        missing_runtime.memory_runtime.memory_runtime = false;
        assert_eq!(
            negotiate(&request, missing_runtime),
            Err(NegotiationError::Memory(
                crate::memory_caprichio::Diagnostic("target runtime requires memory_runtime")
            ))
        );
    }

    #[test]
    fn missing_host_facilities_are_propagated_as_lowering_errors() {
        let request = Request {
            capabilities: vec![crate::CONTRACT.into()],
            network: Some(network()),
            ..Request::default()
        };
        let mut profile = host();
        profile.network_runtime.fetch = false;
        assert_eq!(
            negotiate(&request, profile),
            Err(NegotiationError::Network(crate::Diagnostic(
                "target runtime requires fetch"
            )))
        );
    }

    /// Explicitly run with `cargo test -p goya-caprichios registry::tests::benchmark_negotiation -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual negotiation microbenchmark"]
    fn benchmark_negotiation() {
        use std::time::Instant;
        let request = Request {
            capabilities: vec![
                crate::CONTRACT.into(),
                crate::storage::CONTRACT.into(),
                crate::memory_caprichio::CONTRACT.into(),
                crate::compute_caprichio::CONTRACT.into(),
                crate::data_caprichio::CONTRACT.into(),
                crate::progress_caprichio::CONTRACT.into(),
                crate::checksum_caprichio::CONTRACT.into(),
                crate::retry_caprichio::CONTRACT.into(),
                crate::filesystem_caprichio::CONTRACT.into(),
                crate::http_caprichio::CONTRACT.into(),
                crate::stream_caprichio::CONTRACT.into(),
                crate::compression_caprichio::CONTRACT.into(),
                crate::deadline_caprichio::CONTRACT.into(),
                crate::priority_caprichio::CONTRACT.into(),
            ],
            network: Some(network()),
            storage: Some(storage()),
            memory: Some(memory()),
            compute: Some(compute()),
            data: Some(data()),
            progress: Some(progress()),
            checksum: Some(checksum()),
            retry: Some(retry()),
            filesystem: Some(filesystem()),
            http: Some(http()),
            stream: Some(stream()),
            compression: Some(compression()),
            batching: None,
            deadline: Some(deadline()),
            csv: None,
            priority: Some(priority()),
            parallelism: None,
            input_validation: None,
            keychain: None,
            metal_compute: None,
            touch_id: None,
            face_id: None,
            apple_pay: None,
            windows_hello: None,
            google_pay: None,
            samsung_pay: None,
            bluetooth_central: None,
            bluetooth_gatt: None,
            bluetooth_stream: None,
            bluetooth_guard: None,
            bluetooth_continuity: None,
            bluetooth_access: None,
            bluetooth_profile: None,
        };
        let start = Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            std::hint::black_box(negotiate(&request, host()).unwrap());
        }
        eprintln!(
            "{iterations} mixed registry negotiations in {:?}",
            start.elapsed()
        );
    }
}
