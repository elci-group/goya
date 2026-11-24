//! Deterministic negotiation for implemented recipe contracts.

use crate::accelerator_caprichio::{
    AcceleratorArtifact, AcceleratorDeclaration,
    AcceleratorRuntimeCapabilities as AcceleratorModelHost,
    AcceleratorTarget as AcceleratorModelTarget,
};
use crate::amd_apu_caprichio::{
    AmdApuArtifact, AmdApuDeclaration, RuntimeCapabilities as AmdApuHost, Target as AmdApuTarget,
};
use crate::amd_desktop_caprichio::{
    AmdDesktopArtifact, AmdDesktopDeclaration, RuntimeCapabilities as AmdDesktopHost,
    Target as AmdDesktopTarget,
};
use crate::amd_epyc_caprichio::{
    AmdEpycArtifact, AmdEpycDeclaration, RuntimeCapabilities as AmdEpycHost,
    Target as AmdEpycTarget,
};
use crate::amd_threadripper_caprichio::{
    AmdThreadripperArtifact, AmdThreadripperDeclaration,
    RuntimeCapabilities as AmdThreadripperHost, Target as AmdThreadripperTarget,
};
use crate::apple_pay_caprichio::{
    ApplePayArtifact, ApplePayDeclaration, RuntimeCapabilities as ApplePayHost,
    Target as ApplePayTarget,
};
use crate::batching_caprichio::{
    BatchArtifact, BatchDeclaration, RuntimeCapabilities as BatchRuntime, Target as BatchTarget,
};
use crate::bluetooth_access_caprichio::{
    BluetoothAccessArtifact, BluetoothAccessDeclaration,
    RuntimeCapabilities as BluetoothAccessHost, Target as BluetoothAccessTarget,
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
    BluetoothProfileArtifact, BluetoothProfileDeclaration,
    RuntimeCapabilities as BluetoothProfileHost, Target as BluetoothProfileTarget,
};
use crate::bluetooth_stream_caprichio::{
    BluetoothStreamArtifact, BluetoothStreamDeclaration,
    RuntimeCapabilities as BluetoothStreamHost, Target as BluetoothStreamTarget,
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
use crate::exynos_accelerator_caprichio::{
    ExynosAcceleratorArtifact, ExynosAcceleratorDeclaration,
    RuntimeCapabilities as ExynosAcceleratorHost, Target as ExynosAcceleratorTarget,
};
use crate::face_id_caprichio::{
    FaceIdArtifact, FaceIdDeclaration, RuntimeCapabilities as FaceIdHost, Target as FaceIdTarget,
};
use crate::filesystem_caprichio::{
    FileReadArtifact, HostProfile as FileHost, ReadDeclaration, Target as FileTarget,
};
use crate::google_pay_caprichio::{
    GooglePayArtifact, GooglePayDeclaration, RuntimeCapabilities as GooglePayHost,
    Target as GooglePayTarget,
};
use crate::http_caprichio::{
    HttpArtifact, HttpDeclaration, RuntimeCapabilities as HttpRuntime, Target as HttpTarget,
};
use crate::input_validation_caprichio::{
    Artifact as InputValidationArtifact, Declaration as InputValidationDeclaration,
    HostProfile as InputValidationHost, Target as InputValidationTarget,
};
use crate::intel_desktop_caprichio::{
    IntelDesktopArtifact, IntelDesktopDeclaration, RuntimeCapabilities as IntelDesktopHost,
    Target as IntelDesktopTarget,
};
use crate::intel_mobile_caprichio::{
    IntelMobileArtifact, IntelMobileDeclaration, RuntimeCapabilities as IntelMobileHost,
    Target as IntelMobileTarget,
};
use crate::intel_xeon_caprichio::{
    IntelXeonArtifact, IntelXeonDeclaration, RuntimeCapabilities as IntelXeonHost,
    Target as IntelXeonTarget,
};
use crate::keychain_caprichio::{
    KeychainArtifact, KeychainDeclaration, RuntimeCapabilities as KeychainHost,
    Target as KeychainTarget,
};
use crate::kirin_accelerator_caprichio::{
    KirinAcceleratorArtifact, KirinAcceleratorDeclaration,
    RuntimeCapabilities as KirinAcceleratorHost, Target as KirinAcceleratorTarget,
};
use crate::mediatek_accelerator_caprichio::{
    MediaTekAcceleratorArtifact, MediaTekAcceleratorDeclaration,
    RuntimeCapabilities as MediaTekAcceleratorHost, Target as MediaTekAcceleratorTarget,
};
use crate::memory_caprichio::{
    MemoryArtifact, MemoryDeclaration, RuntimeCapabilities as MemoryRuntime, Target as MemoryTarget,
};
use crate::metal_caprichio::{
    DispatchArtifact as MetalArtifact, DispatchDeclaration as MetalDeclaration,
    RuntimeCapabilities as MetalRuntime, Target as MetalTarget,
};
use crate::nvidia_accelerator_caprichio::{
    NvidiaCudaArtifact, NvidiaCudaDeclaration, RuntimeCapabilities as NvidiaCudaHost,
    Target as NvidiaCudaTarget,
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
use crate::samsung_pay_caprichio::{
    RuntimeCapabilities as SamsungPayHost, SamsungPayArtifact, SamsungPayDeclaration,
    Target as SamsungPayTarget,
};
use crate::snapdragon_accelerator_caprichio::{
    RuntimeCapabilities as SnapdragonAcceleratorHost, SnapdragonAcceleratorArtifact,
    SnapdragonAcceleratorDeclaration, Target as SnapdragonAcceleratorTarget,
};
use crate::storage::{
    RuntimeCapabilities as StorageRuntime, StorageArtifact, StorageDeclaration,
    Target as StorageTarget,
};
use crate::stream_caprichio::{
    RuntimeCapabilities as StreamRuntime, StreamArtifact, StreamDeclaration, Target as StreamTarget,
};
use crate::tensor_accelerator_caprichio::{
    RuntimeCapabilities as TensorAcceleratorHost, Target as TensorAcceleratorTarget,
    TensorAcceleratorArtifact, TensorAcceleratorDeclaration,
};
use crate::touch_id_caprichio::{
    RuntimeCapabilities as TouchIdHost, Target as TouchIdTarget, TouchIdArtifact,
    TouchIdDeclaration,
};
use crate::windows_hello_caprichio::{
    RuntimeCapabilities as WindowsHelloHost, Target as WindowsHelloTarget, WindowsHelloArtifact,
    WindowsHelloDeclaration,
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
    pub mediatek_neural_accel: Option<MediaTekAcceleratorDeclaration>,
    pub tensor_neural_accel: Option<TensorAcceleratorDeclaration>,
    pub kirin_neural_accel: Option<KirinAcceleratorDeclaration>,
    pub nvidia_cuda: Option<NvidiaCudaDeclaration>,
    pub snapdragon_neural_accel: Option<SnapdragonAcceleratorDeclaration>,
    pub exynos_neural_accel: Option<ExynosAcceleratorDeclaration>,
    pub amd_desktop: Option<AmdDesktopDeclaration>,
    pub amd_apu: Option<AmdApuDeclaration>,
    pub amd_threadripper: Option<AmdThreadripperDeclaration>,
    pub amd_epyc: Option<AmdEpycDeclaration>,
    pub intel_desktop: Option<IntelDesktopDeclaration>,
    pub intel_mobile: Option<IntelMobileDeclaration>,
    pub intel_xeon: Option<IntelXeonDeclaration>,
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
    pub mediatek_neural_accel_target: MediaTekAcceleratorTarget,
    pub mediatek_neural_accel_host: MediaTekAcceleratorHost,
    pub tensor_neural_accel_target: TensorAcceleratorTarget,
    pub tensor_neural_accel_host: TensorAcceleratorHost,
    pub kirin_neural_accel_target: KirinAcceleratorTarget,
    pub kirin_neural_accel_host: KirinAcceleratorHost,
    pub nvidia_cuda_target: NvidiaCudaTarget,
    pub nvidia_cuda_host: NvidiaCudaHost,
    pub snapdragon_accelerator_target: SnapdragonAcceleratorTarget,
    pub snapdragon_accelerator_host: SnapdragonAcceleratorHost,
    pub exynos_accelerator_target: ExynosAcceleratorTarget,
    pub exynos_accelerator_host: ExynosAcceleratorHost,
    pub amd_desktop_target: AmdDesktopTarget,
    pub amd_desktop_host: AmdDesktopHost,
    pub amd_apu_target: AmdApuTarget,
    pub amd_apu_host: AmdApuHost,
    pub amd_threadripper_target: AmdThreadripperTarget,
    pub amd_threadripper_host: AmdThreadripperHost,
    pub amd_epyc_target: AmdEpycTarget,
    pub amd_epyc_host: AmdEpycHost,
    pub intel_desktop_target: IntelDesktopTarget,
    pub intel_desktop_host: IntelDesktopHost,
    pub intel_mobile_target: IntelMobileTarget,
    pub intel_mobile_host: IntelMobileHost,
    pub intel_xeon_target: IntelXeonTarget,
    pub intel_xeon_host: IntelXeonHost,
    pub accelerator_model_target: AcceleratorModelTarget,
    pub accelerator_model_host: AcceleratorModelHost,
}

#[path = "registry_artifact.rs"]
mod artifact;
pub use artifact::Artifact;

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
        crate::amd_apu_caprichio::CONTRACT,
        crate::amd_desktop_caprichio::CONTRACT,
        crate::amd_epyc_caprichio::CONTRACT,
        crate::amd_threadripper_caprichio::CONTRACT,
        crate::intel_desktop_caprichio::CONTRACT,
        crate::intel_mobile_caprichio::CONTRACT,
        crate::intel_xeon_caprichio::CONTRACT,
        crate::metal_caprichio::CONTRACT,
        crate::nvidia_accelerator_caprichio::CONTRACT,
        crate::exynos_accelerator_caprichio::CONTRACT,
        crate::kirin_accelerator_caprichio::CONTRACT,
        crate::mediatek_accelerator_caprichio::CONTRACT,
        crate::snapdragon_accelerator_caprichio::CONTRACT,
        crate::tensor_accelerator_caprichio::CONTRACT,
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
    MediaTekAccelerator(crate::mediatek_accelerator_caprichio::MediaTekAcceleratorDiagnostic),
    TensorAccelerator(crate::tensor_accelerator_caprichio::TensorAcceleratorDiagnostic),
    KirinAccelerator(crate::kirin_accelerator_caprichio::KirinAcceleratorDiagnostic),
    NvidiaCuda(crate::nvidia_accelerator_caprichio::NvidiaCudaDiagnostic),
    SnapdragonAccelerator(crate::snapdragon_accelerator_caprichio::SnapdragonAcceleratorDiagnostic),
    ExynosAccelerator(crate::exynos_accelerator_caprichio::ExynosAcceleratorDiagnostic),
    AmdDesktop(crate::amd_desktop_caprichio::AmdDesktopDiagnostic),
    AmdApu(crate::amd_apu_caprichio::AmdApuDiagnostic),
    AmdThreadripper(crate::amd_threadripper_caprichio::AmdThreadripperDiagnostic),
    AmdEpyc(crate::amd_epyc_caprichio::AmdEpycDiagnostic),
    IntelDesktop(crate::intel_desktop_caprichio::IntelDesktopDiagnostic),
    IntelMobile(crate::intel_mobile_caprichio::IntelMobileDiagnostic),
    IntelXeon(crate::intel_xeon_caprichio::IntelXeonDiagnostic),
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
            Self::MediaTekAccelerator(error) => {
                write!(
                    f,
                    "MediaTek neural-acceleration negotiation failed: {e}",
                    e = error
                )
            }
            Self::TensorAccelerator(error) => {
                write!(
                    f,
                    "Tensor neural-acceleration negotiation failed: {e}",
                    e = error
                )
            }
            Self::KirinAccelerator(error) => {
                write!(
                    f,
                    "Kirin neural-acceleration negotiation failed: {e}",
                    e = error
                )
            }
            Self::NvidiaCuda(error) => {
                write!(f, "NVIDIA CUDA negotiation failed: {e}", e = error)
            }
            Self::SnapdragonAccelerator(error) => {
                write!(
                    f,
                    "Snapdragon neural-acceleration negotiation failed: {err}",
                    err = error
                )
            }
            Self::ExynosAccelerator(error) => {
                write!(
                    f,
                    "Exynos neural-acceleration negotiation failed: {err}",
                    err = error
                )
            }
            Self::AmdDesktop(error) => {
                write!(f, "AMD desktop CPU negotiation failed: {e}", e = error)
            }
            Self::AmdApu(error) => write!(f, "AMD APU negotiation failed: {e}", e = error),
            Self::AmdThreadripper(error) => {
                write!(f, "AMD Threadripper negotiation failed: {e}", e = error)
            }
            Self::AmdEpyc(error) => write!(f, "AMD EPYC negotiation failed: {e}", e = error),
            Self::IntelDesktop(error) => {
                write!(f, "Intel desktop CPU negotiation failed: {e}", e = error)
            }
            Self::IntelMobile(error) => {
                write!(f, "Intel mobile CPU negotiation failed: {e}", e = error)
            }
            Self::IntelXeon(error) => {
                write!(f, "Intel Xeon CPU negotiation failed: {e}", e = error)
            }
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
            Self::BluetoothAccess(error) => {
                write!(f, "Bluetooth access negotiation failed: {error}")
            }
            Self::BluetoothProfile(error) => {
                write!(f, "Bluetooth profile negotiation failed: {error}")
            }
        }
    }
}

impl std::error::Error for NegotiationError {}

#[path = "registry_negotiation.rs"]
mod negotiation;
pub use negotiation::negotiate;

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
