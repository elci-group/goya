//! Negotiation and lowering of supported contracts.
use super::*;

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
            && *id != crate::colour_cue_caprichio::CONTRACT
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
            && *id != crate::mediatek_accelerator_caprichio::CONTRACT
            && *id != crate::tensor_accelerator_caprichio::CONTRACT
            && *id != crate::kirin_accelerator_caprichio::CONTRACT
            && *id != crate::snapdragon_accelerator_caprichio::CONTRACT
            && *id != crate::exynos_accelerator_caprichio::CONTRACT
            && *id != crate::amd_desktop_caprichio::CONTRACT
            && *id != crate::amd_apu_caprichio::CONTRACT
            && *id != crate::amd_threadripper_caprichio::CONTRACT
            && *id != crate::amd_epyc_caprichio::CONTRACT
            && *id != crate::intel_desktop_caprichio::CONTRACT
            && *id != crate::intel_mobile_caprichio::CONTRACT
            && *id != crate::intel_xeon_caprichio::CONTRACT
            && *id != crate::nvidia_accelerator_caprichio::CONTRACT
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
    let needs_colour_cue = ordered.contains(crate::colour_cue_caprichio::CONTRACT);
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
    let needs_mediatek_neural_accel =
        ordered.contains(crate::mediatek_accelerator_caprichio::CONTRACT);
    let needs_tensor_neural_accel = ordered.contains(crate::tensor_accelerator_caprichio::CONTRACT);
    let needs_kirin_neural_accel = ordered.contains(crate::kirin_accelerator_caprichio::CONTRACT);
    let needs_nvidia_cuda = ordered.contains(crate::nvidia_accelerator_caprichio::CONTRACT);
    let needs_snapdragon_accelerator =
        ordered.contains(crate::snapdragon_accelerator_caprichio::CONTRACT);
    let needs_exynos_accelerator = ordered.contains(crate::exynos_accelerator_caprichio::CONTRACT);
    let needs_amd_desktop = ordered.contains(crate::amd_desktop_caprichio::CONTRACT);
    let needs_amd_apu = ordered.contains(crate::amd_apu_caprichio::CONTRACT);
    let needs_amd_threadripper = ordered.contains(crate::amd_threadripper_caprichio::CONTRACT);
    let needs_amd_epyc = ordered.contains(crate::amd_epyc_caprichio::CONTRACT);
    let needs_intel_desktop = ordered.contains(crate::intel_desktop_caprichio::CONTRACT);
    let needs_intel_mobile = ordered.contains(crate::intel_mobile_caprichio::CONTRACT);
    let needs_intel_xeon = ordered.contains(crate::intel_xeon_caprichio::CONTRACT);
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
    if request.colour_cue.is_some() && !needs_colour_cue {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "colour_cue" });
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
    if request.mediatek_neural_accel.is_some() && !needs_mediatek_neural_accel {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "mediatek_neural_accel",
        });
    }
    if request.tensor_neural_accel.is_some() && !needs_tensor_neural_accel {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "tensor_neural_accel",
        });
    }
    if request.kirin_neural_accel.is_some() && !needs_kirin_neural_accel {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "kirin_neural_accel",
        });
    }
    if request.nvidia_cuda.is_some() && !needs_nvidia_cuda {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "nvidia_cuda",
        });
    }
    if request.snapdragon_neural_accel.is_some() && !needs_snapdragon_accelerator {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "snapdragon_neural_accel",
        });
    }
    if request.exynos_neural_accel.is_some() && !needs_exynos_accelerator {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "exynos_neural_accel",
        });
    }
    if request.amd_desktop.is_some() && !needs_amd_desktop {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "amd_desktop",
        });
    }
    if request.amd_apu.is_some() && !needs_amd_apu {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "amd_apu" });
    }
    if request.amd_threadripper.is_some() && !needs_amd_threadripper {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "amd_threadripper",
        });
    }
    if request.amd_epyc.is_some() && !needs_amd_epyc {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "amd_epyc" });
    }
    if request.intel_desktop.is_some() && !needs_intel_desktop {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "intel_desktop",
        });
    }
    if request.intel_mobile.is_some() && !needs_intel_mobile {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "intel_mobile",
        });
    }
    if request.intel_xeon.is_some() && !needs_intel_xeon {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "intel_xeon" });
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
        return Err(NegotiationError::UnexpectedDeclaration { kind: "apple_pay" });
    }
    if request.windows_hello.is_some() && !needs_windows_hello {
        return Err(NegotiationError::UnexpectedDeclaration {
            kind: "windows_hello",
        });
    }
    if request.google_pay.is_some() && !needs_google_pay {
        return Err(NegotiationError::UnexpectedDeclaration { kind: "google_pay" });
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
            crate::colour_cue_caprichio::CONTRACT => {
                let declaration =
                    request
                        .colour_cue
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::colour_cue_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::ColourCue)?;
                Artifact::ColourCue(
                    ir.lower(host.colour_cue_target, host.colour_cue_host)
                        .map_err(NegotiationError::ColourCue)?,
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
                let declaration =
                    request
                        .keychain
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::keychain_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Keychain)?;
                Artifact::Keychain(
                    ir.lower_checked(host.keychain_target, host.keychain_host)
                        .map_err(NegotiationError::Keychain)?,
                )
            }
            crate::mediatek_accelerator_caprichio::CONTRACT => {
                let declaration = request.mediatek_neural_accel.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::mediatek_accelerator_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::MediaTekAccelerator)?;
                Artifact::MediaTekAccelerator(
                    ir.lower_checked(
                        host.mediatek_neural_accel_target,
                        host.mediatek_neural_accel_host,
                    )
                    .map_err(NegotiationError::MediaTekAccelerator)?,
                )
            }
            crate::tensor_accelerator_caprichio::CONTRACT => {
                let declaration = request.tensor_neural_accel.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::tensor_accelerator_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::TensorAccelerator)?;
                Artifact::TensorAccelerator(
                    ir.lower_checked(
                        host.tensor_neural_accel_target,
                        host.tensor_neural_accel_host,
                    )
                    .map_err(NegotiationError::TensorAccelerator)?,
                )
            }
            crate::kirin_accelerator_caprichio::CONTRACT => {
                let declaration = request.kirin_neural_accel.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::kirin_accelerator_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::KirinAccelerator)?;
                Artifact::KirinAccelerator(
                    ir.lower_checked(host.kirin_neural_accel_target, host.kirin_neural_accel_host)
                        .map_err(NegotiationError::KirinAccelerator)?,
                )
            }
            crate::nvidia_accelerator_caprichio::CONTRACT => {
                let declaration =
                    request
                        .nvidia_cuda
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::nvidia_accelerator_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::NvidiaCuda)?;
                Artifact::NvidiaCuda(
                    ir.lower_checked(host.nvidia_cuda_target, host.nvidia_cuda_host)
                        .map_err(NegotiationError::NvidiaCuda)?,
                )
            }
            crate::snapdragon_accelerator_caprichio::CONTRACT => {
                let declaration = request.snapdragon_neural_accel.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::snapdragon_accelerator_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::SnapdragonAccelerator)?;
                Artifact::SnapdragonAccelerator(
                    ir.lower_checked(
                        host.snapdragon_accelerator_target,
                        host.snapdragon_accelerator_host,
                    )
                    .map_err(NegotiationError::SnapdragonAccelerator)?,
                )
            }
            crate::exynos_accelerator_caprichio::CONTRACT => {
                let declaration = request.exynos_neural_accel.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::exynos_accelerator_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::ExynosAccelerator)?;
                Artifact::ExynosAccelerator(
                    ir.lower_checked(host.exynos_accelerator_target, host.exynos_accelerator_host)
                        .map_err(NegotiationError::ExynosAccelerator)?,
                )
            }
            crate::amd_desktop_caprichio::CONTRACT => {
                let declaration =
                    request
                        .amd_desktop
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::amd_desktop_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::AmdDesktop)?;
                Artifact::AmdDesktop(
                    ir.lower_checked(host.amd_desktop_target, host.amd_desktop_host)
                        .map_err(NegotiationError::AmdDesktop)?,
                )
            }
            crate::amd_apu_caprichio::CONTRACT => {
                let declaration =
                    request
                        .amd_apu
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::amd_apu_caprichio::CONTRACT,
                        })?;
                let ir = declaration.normalize().map_err(NegotiationError::AmdApu)?;
                Artifact::AmdApu(
                    ir.lower_checked(host.amd_apu_target, host.amd_apu_host)
                        .map_err(NegotiationError::AmdApu)?,
                )
            }
            crate::amd_threadripper_caprichio::CONTRACT => {
                let declaration = request.amd_threadripper.clone().ok_or(
                    NegotiationError::MissingDeclaration {
                        capability: crate::amd_threadripper_caprichio::CONTRACT,
                    },
                )?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::AmdThreadripper)?;
                Artifact::AmdThreadripper(
                    ir.lower_checked(host.amd_threadripper_target, host.amd_threadripper_host)
                        .map_err(NegotiationError::AmdThreadripper)?,
                )
            }
            crate::amd_epyc_caprichio::CONTRACT => {
                let declaration =
                    request
                        .amd_epyc
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::amd_epyc_caprichio::CONTRACT,
                        })?;
                let ir = declaration.normalize().map_err(NegotiationError::AmdEpyc)?;
                Artifact::AmdEpyc(
                    ir.lower_checked(host.amd_epyc_target, host.amd_epyc_host)
                        .map_err(NegotiationError::AmdEpyc)?,
                )
            }
            crate::intel_desktop_caprichio::CONTRACT => {
                let declaration =
                    request
                        .intel_desktop
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::intel_desktop_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::IntelDesktop)?;
                Artifact::IntelDesktop(
                    ir.lower_checked(host.intel_desktop_target, host.intel_desktop_host)
                        .map_err(NegotiationError::IntelDesktop)?,
                )
            }
            crate::intel_mobile_caprichio::CONTRACT => {
                let declaration =
                    request
                        .intel_mobile
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::intel_mobile_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::IntelMobile)?;
                Artifact::IntelMobile(
                    ir.lower_checked(host.intel_mobile_target, host.intel_mobile_host)
                        .map_err(NegotiationError::IntelMobile)?,
                )
            }
            crate::intel_xeon_caprichio::CONTRACT => {
                let declaration =
                    request
                        .intel_xeon
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::intel_xeon_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::IntelXeon)?;
                Artifact::IntelXeon(
                    ir.lower_checked(host.intel_xeon_target, host.intel_xeon_host)
                        .map_err(NegotiationError::IntelXeon)?,
                )
            }
            crate::metal_caprichio::CONTRACT => {
                let declaration =
                    request
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
                let declaration = request
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
                let declaration =
                    request
                        .apple_pay
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::apple_pay_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::ApplePay)?;
                Artifact::ApplePay(
                    ir.lower_checked(host.apple_pay_target, host.apple_pay_host)
                        .map_err(NegotiationError::ApplePay)?,
                )
            }
            crate::windows_hello_caprichio::CONTRACT => {
                let declaration =
                    request
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
                let declaration =
                    request
                        .google_pay
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::google_pay_caprichio::CONTRACT,
                        })?;
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::GooglePay)?;
                Artifact::GooglePay(
                    ir.lower_checked(host.google_pay_target, host.google_pay_host)
                        .map_err(NegotiationError::GooglePay)?,
                )
            }
            crate::samsung_pay_caprichio::CONTRACT => {
                let declaration =
                    request
                        .samsung_pay
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::samsung_pay_caprichio::CONTRACT,
                        })?;
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
                let ir = declaration
                    .normalize()
                    .map_err(NegotiationError::Bluetooth)?;
                Artifact::Bluetooth(
                    ir.lower_checked(host.bluetooth_central_target, host.bluetooth_central_host)
                        .map_err(NegotiationError::Bluetooth)?,
                )
            }
            crate::bluetooth_control_caprichio::CONTRACT => {
                let declaration =
                    request
                        .bluetooth_gatt
                        .clone()
                        .ok_or(NegotiationError::MissingDeclaration {
                            capability: crate::bluetooth_control_caprichio::CONTRACT,
                        })?;
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
        let id = artifact.contract();
        result.push(NegotiatedCapability { id, artifact });
    }
    Ok(result)
}
