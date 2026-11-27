// Unit tests for the parent module.
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
        mediatek_neural_accel_target: MediaTekAcceleratorTarget::MediaTek,
        mediatek_neural_accel_host: MediaTekAcceleratorHost {
            apu_dsp: true,
            neural_delegate: true,
            int8_model: true,
            utf8_strings: true,
        },
        tensor_neural_accel_target: TensorAcceleratorTarget::Tensor,
        tensor_neural_accel_host: TensorAcceleratorHost {
            apu_dsp: true,
            neural_delegate: true,
            int8_model: true,
            utf8_strings: true,
        },
        kirin_neural_accel_target: KirinAcceleratorTarget::Kirin,
        kirin_neural_accel_host: KirinAcceleratorHost {
            apu_dsp: true,
            neural_delegate: true,
            int8_model: true,
            utf8_strings: true,
        },
        nvidia_cuda_target: NvidiaCudaTarget::NvidiaCuda,
        nvidia_cuda_host: NvidiaCudaHost {
            cuda_device: true,
            cuda_runtime_api: true,
            int8_model: true,
            utf8_strings: true,
        },
        snapdragon_accelerator_target: SnapdragonAcceleratorTarget::Snapdragon,
        snapdragon_accelerator_host: SnapdragonAcceleratorHost {
            hexagon_dsp: true,
            neural_delegate: true,
            int8_model: true,
            utf8_strings: true,
        },
        exynos_accelerator_target: ExynosAcceleratorTarget::Exynos,
        exynos_accelerator_host: ExynosAcceleratorHost {
            npu_dsp: true,
            neural_delegate: true,
            int8_model: true,
            utf8_strings: true,
        },
        amd_desktop_target: AmdDesktopTarget::AmdDesktop,
        amd_desktop_host: AmdDesktopHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        amd_apu_target: AmdApuTarget::AmdApu,
        amd_apu_host: AmdApuHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        amd_threadripper_target: AmdThreadripperTarget::AmdThreadripper,
        amd_threadripper_host: AmdThreadripperHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        amd_epyc_target: AmdEpycTarget::AmdEpyc,
        amd_epyc_host: AmdEpycHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        intel_desktop_target: IntelDesktopTarget::IntelDesktop,
        intel_desktop_host: IntelDesktopHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        intel_mobile_target: IntelMobileTarget::IntelMobile,
        intel_mobile_host: IntelMobileHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        intel_xeon_target: IntelXeonTarget::IntelXeon,
        intel_xeon_host: IntelXeonHost {
            x86_64_runtime: true,
            avx: true,
            avx2: true,
            avx512: true,
            utf8_strings: true,
        },
        accelerator_model_target: AcceleratorModelTarget::Device,
        accelerator_model_host: AcceleratorModelHost {
            gpu_driver: true,
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
        bluetooth_profile_host: BluetoothProfileHost { utf8_strings: true },
        colour_cue_target: ColourCueTarget::CpuOnly,
        colour_cue_host: ColourCueHost {
            software_present: true,
        },
        sso_google_target: GoogleSsoTarget::Browser,
        sso_google_host: GoogleSsoHost {
            https_client: true,
            external_user_agent: true,
            secure_storage: true,
            utf8_strings: true,
        },
        sso_microsoft_target: MicrosoftSsoTarget::Browser,
        sso_microsoft_host: MicrosoftSsoHost {
            https_client: true,
            external_user_agent: true,
            secure_storage: true,
            utf8_strings: true,
        },
        sso_github_target: GithubSsoTarget::Browser,
        sso_github_host: GithubSsoHost {
            https_client: true,
            external_user_agent: true,
            secure_storage: true,
            utf8_strings: true,
        },
        sso_x_target: XSsoTarget::Browser,
        sso_x_host: XSsoHost {
            https_client: true,
            external_user_agent: true,
            secure_storage: true,
            utf8_strings: true,
        },
        sso_meta_target: MetaSsoTarget::Browser,
        sso_meta_host: MetaSsoHost {
            https_client: true,
            external_user_agent: true,
            secure_storage: true,
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

fn amd_desktop_declaration() -> AmdDesktopDeclaration {
    AmdDesktopDeclaration {
        cpu: "Ryzen 9000".into(),
        worker_threads: 16,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::amd_desktop_caprichio::Isa::Avx512,
    }
}

fn amd_apu_declaration() -> AmdApuDeclaration {
    AmdApuDeclaration {
        apu: "Strix Halo".into(),
        worker_threads: 16,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::amd_apu_caprichio::Isa::Avx512,
    }
}

fn amd_threadripper_declaration() -> AmdThreadripperDeclaration {
    AmdThreadripperDeclaration {
        cpu: "Threadripper Pro 7000WX".into(),
        worker_threads: 96,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::amd_threadripper_caprichio::Isa::Avx512,
    }
}

fn amd_epyc_declaration() -> AmdEpycDeclaration {
    AmdEpycDeclaration {
        cpu: "EPYC 9005 (Turin)".into(),
        worker_threads: 192,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::amd_epyc_caprichio::Isa::Avx512,
    }
}

fn intel_desktop_declaration() -> IntelDesktopDeclaration {
    IntelDesktopDeclaration {
        cpu: "Core 2 (Penryn)".into(),
        worker_threads: 4,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::intel_desktop_caprichio::Isa::Sse3,
    }
}

fn intel_mobile_declaration() -> IntelMobileDeclaration {
    IntelMobileDeclaration {
        cpu: "Tiger Lake".into(),
        worker_threads: 8,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::intel_mobile_caprichio::Isa::Avx512,
    }
}

fn intel_xeon_declaration() -> IntelXeonDeclaration {
    IntelXeonDeclaration {
        cpu: "Xeon 5500 (Nehalem-EP)".into(),
        worker_threads: 8,
        batch: 8,
        workspace_bytes: 65_536,
        isa: crate::intel_xeon_caprichio::Isa::Sse3,
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
        usage_description: "Connects to your temperature sensor to show readings.".to_owned(),
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
        capabilities: vec![contract.into()],
        touch_id: Some(touch_id()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "apple-touch-id-bounded-attempt-gate"
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("sensor=touch_id\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("fallback=device_passcode\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("data_access=none\n"));
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
        capabilities: vec![contract.into()],
        face_id: Some(face_id()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "apple-face-id-bounded-attempt-gate"
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("sensor=face_id\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("attention=required\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("data_access=none\n"));
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
        capabilities: vec![contract.into()],
        apple_pay: Some(apple_pay()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "apple-pay-bounded-payment-request"
    );
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
    assert_eq!(
        negotiated,
        negotiate(&windows_hello_request, host()).unwrap()
    );

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
        (crate::windows_hello_caprichio::CONTRACT, "windows_hello"),
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
    without_query
        .bluetooth_guard_host
        .bluetooth_permission_query = false;
    assert_eq!(
        negotiate(&request, without_query),
        Err(NegotiationError::BluetoothGuard(
            crate::bluetooth_guard_caprichio::Diagnostic::MissingBluetoothPermissionQuery
        ))
    );
    let mut without_store = host();
    without_store
        .bluetooth_guard_host
        .bluetooth_authorisation_store = false;
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
fn amd_desktop_contract_registers_exactly_and_fails_closed() {
    let contract = crate::amd_desktop_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.amd_desktop.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        amd_desktop: Some(amd_desktop_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "amd-desktop-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::amd_desktop_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=amd_desktop\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        amd_desktop: Some(amd_desktop_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration {
            kind: "amd_desktop"
        })
    );

    let mut unsupported_target = host();
    unsupported_target.amd_desktop_target = AmdDesktopTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::AmdDesktop(
            crate::amd_desktop_caprichio::AmdDesktopDiagnostic::UnsupportedTarget
        ))
    );
    let mut no_avx512 = host();
    no_avx512.amd_desktop_host.avx512 = false;
    assert_eq!(
        negotiate(&request, no_avx512),
        Err(NegotiationError::AmdDesktop(
            crate::amd_desktop_caprichio::AmdDesktopDiagnostic::MissingAvx512
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn amd_apu_contract_registers_exactly_and_fails_closed() {
    let contract = crate::amd_apu_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.amd_apu.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        amd_apu: Some(amd_apu_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "amd-apu-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::amd_apu_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=amd_apu\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        amd_apu: Some(amd_apu_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration { kind: "amd_apu" })
    );

    let mut unsupported_target = host();
    unsupported_target.amd_apu_target = AmdApuTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::AmdApu(
            crate::amd_apu_caprichio::AmdApuDiagnostic::UnsupportedTarget
        ))
    );
    let mut no_runtime = host();
    no_runtime.amd_apu_host.x86_64_runtime = false;
    assert_eq!(
        negotiate(&request, no_runtime),
        Err(NegotiationError::AmdApu(
            crate::amd_apu_caprichio::AmdApuDiagnostic::MissingX86_64Runtime
        ))
    );

    let draft = Request {
        capabilities: vec!["goya.hardware.cpu_compute.v1".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "goya.hardware.cpu_compute.v1".into()
        ))
    );
}

#[test]
fn amd_threadripper_contract_registers_exactly_and_fails_closed() {
    let contract = crate::amd_threadripper_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.amd_threadripper.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        amd_threadripper: Some(amd_threadripper_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "amd-threadripper-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::amd_threadripper_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=amd_threadripper\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        amd_threadripper: Some(amd_threadripper_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration {
            kind: "amd_threadripper"
        })
    );

    let mut unsupported_target = host();
    unsupported_target.amd_threadripper_target = AmdThreadripperTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::AmdThreadripper(
            crate::amd_threadripper_caprichio::AmdThreadripperDiagnostic::UnsupportedTarget
        ))
    );
    let mut no_utf8 = host();
    no_utf8.amd_threadripper_host.utf8_strings = false;
    assert_eq!(
        negotiate(&request, no_utf8),
        Err(NegotiationError::AmdThreadripper(
            crate::amd_threadripper_caprichio::AmdThreadripperDiagnostic::MissingUtf8Strings
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn amd_epyc_contract_registers_exactly_and_fails_closed() {
    let contract = crate::amd_epyc_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.amd_epyc.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        amd_epyc: Some(amd_epyc_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "amd-epyc-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::amd_epyc_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=amd_epyc\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        amd_epyc: Some(amd_epyc_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration { kind: "amd_epyc" })
    );

    let mut unsupported_target = host();
    unsupported_target.amd_epyc_target = AmdEpycTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::AmdEpyc(
            crate::amd_epyc_caprichio::AmdEpycDiagnostic::UnsupportedTarget
        ))
    );
    let mut no_avx512 = host();
    no_avx512.amd_epyc_host.avx512 = false;
    assert_eq!(
        negotiate(&request, no_avx512),
        Err(NegotiationError::AmdEpyc(
            crate::amd_epyc_caprichio::AmdEpycDiagnostic::MissingAvx512
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn intel_desktop_contract_registers_exactly_and_fails_closed() {
    let contract = crate::intel_desktop_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.intel_desktop.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        intel_desktop: Some(intel_desktop_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "intel-desktop-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::intel_desktop_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=intel_desktop\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        intel_desktop: Some(intel_desktop_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration {
            kind: "intel_desktop"
        })
    );

    let mut unsupported_target = host();
    unsupported_target.intel_desktop_target = IntelDesktopTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::IntelDesktop(
            crate::intel_desktop_caprichio::IntelDesktopDiagnostic::UnsupportedTarget
        ))
    );
    // The helper plans on the low-thread Penryn row, so the ISA-ladder
    // negative needs an AVX-512-capable generation to reach the host
    // capability check.
    let mut avx512_declaration = intel_desktop_declaration();
    avx512_declaration.cpu = "Rocket Lake".into();
    avx512_declaration.isa = crate::intel_desktop_caprichio::Isa::Avx512;
    let avx512_request = Request {
        capabilities: vec![contract.into()],
        intel_desktop: Some(avx512_declaration),
        ..Request::default()
    };
    let mut no_avx512 = host();
    no_avx512.intel_desktop_host.avx512 = false;
    assert_eq!(
        negotiate(&avx512_request, no_avx512),
        Err(NegotiationError::IntelDesktop(
            crate::intel_desktop_caprichio::IntelDesktopDiagnostic::MissingAvx512
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn intel_mobile_contract_registers_exactly_and_fails_closed() {
    let contract = crate::intel_mobile_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.intel_mobile.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        intel_mobile: Some(intel_mobile_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "intel-mobile-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::intel_mobile_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=intel_mobile\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        intel_mobile: Some(intel_mobile_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration {
            kind: "intel_mobile"
        })
    );

    let mut unsupported_target = host();
    unsupported_target.intel_mobile_target = IntelMobileTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::IntelMobile(
            crate::intel_mobile_caprichio::IntelMobileDiagnostic::UnsupportedTarget
        ))
    );
    let mut no_avx512 = host();
    no_avx512.intel_mobile_host.avx512 = false;
    assert_eq!(
        negotiate(&request, no_avx512),
        Err(NegotiationError::IntelMobile(
            crate::intel_mobile_caprichio::IntelMobileDiagnostic::MissingAvx512
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn intel_xeon_contract_registers_exactly_and_fails_closed() {
    let contract = crate::intel_xeon_caprichio::CONTRACT;
    assert_eq!(contract, "goya.hardware.cpu_compute.intel_xeon.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"hardware.cpu_compute"));
    assert!(!supported_contracts().contains(&"goya.hardware.cpu_compute.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        intel_xeon: Some(intel_xeon_declaration()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "intel-xeon-bounded-worker-plan"
    );
    assert_eq!(
        negotiated[0].artifact.recipe_version(),
        crate::intel_xeon_caprichio::RECIPE_VERSION
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("target=intel_xeon\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("execution=none_attempted\n"));
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
        intel_xeon: Some(intel_xeon_declaration()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration { kind: "intel_xeon" })
    );

    let mut unsupported_target = host();
    unsupported_target.intel_xeon_target = IntelXeonTarget::CrossPlatform;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::IntelXeon(
            crate::intel_xeon_caprichio::IntelXeonDiagnostic::UnsupportedTarget
        ))
    );
    // The helper plans on Nehalem-EP, so the ISA-ladder negative needs
    // an AVX-512-capable generation to reach the host capability check.
    let mut avx512_declaration = intel_xeon_declaration();
    avx512_declaration.cpu = "Xeon 6 6900P (Granite Rapids-AP)".into();
    avx512_declaration.isa = crate::intel_xeon_caprichio::Isa::Avx512;
    let avx512_request = Request {
        capabilities: vec![contract.into()],
        intel_xeon: Some(avx512_declaration),
        ..Request::default()
    };
    let mut no_avx512 = host();
    no_avx512.intel_xeon_host.avx512 = false;
    assert_eq!(
        negotiate(&avx512_request, no_avx512),
        Err(NegotiationError::IntelXeon(
            crate::intel_xeon_caprichio::IntelXeonDiagnostic::MissingAvx512
        ))
    );

    let draft = Request {
        capabilities: vec!["hardware.cpu_compute".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability(
            "hardware.cpu_compute".into()
        ))
    );
}

#[test]
fn mixed_request_is_sorted_and_artifacts_keep_contract_identity() {
    let request = Request {
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
        amd_desktop: None,
        amd_apu: None,
        amd_threadripper: None,
        amd_epyc: None,
        intel_desktop: None,
        intel_mobile: None,
        intel_xeon: None,
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
        accelerator_model: None,
        bluetooth_central: None,
        bluetooth_gatt: None,
        bluetooth_stream: None,
        bluetooth_guard: None,
        bluetooth_continuity: None,
        bluetooth_access: None,
        bluetooth_profile: None,
        colour_cue: None,
        sso_google: None,
        sso_microsoft: None,
        sso_github: None,
        sso_x: None,
        sso_meta: None,
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
            "goya.graphics.colour_cue.placement.v1",
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
            "goya.hardware.cpu_compute.amd_apu.v1",
            "goya.hardware.cpu_compute.amd_desktop.v1",
            "goya.hardware.cpu_compute.amd_epyc.v1",
            "goya.hardware.cpu_compute.amd_threadripper.v1",
            "goya.hardware.cpu_compute.intel_desktop.v1",
            "goya.hardware.cpu_compute.intel_mobile.v1",
            "goya.hardware.cpu_compute.intel_xeon.v1",
            "goya.hardware.gpu_compute.apple_silicon_metal.v1",
            "goya.hardware.gpu_compute.nvidia_cuda.v1",
            "goya.hardware.neural_accel.exynos.v1",
            "goya.hardware.neural_accel.kirin.v1",
            "goya.hardware.neural_accel.mediatek.v1",
            "goya.hardware.neural_accel.snapdragon.v1",
            "goya.hardware.neural_accel.tensor.v1",
            "goya.hardware.payments.apple_pay.v1",
            "goya.hardware.payments.google_pay.v1",
            "goya.hardware.payments.samsung_pay.v1",
            "goya.hardware.secure_storage.keychain.v1",
            "goya.identity.sso.github.v1",
            "goya.identity.sso.google.v1",
            "goya.identity.sso.meta.v1",
            "goya.identity.sso.microsoft.v1",
            "goya.identity.sso.x.v1",
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
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
        nvidia_cuda: None,
        kirin_neural_accel: None,
        mediatek_neural_accel: None,
        tensor_neural_accel: None,
        snapdragon_neural_accel: None,
        exynos_neural_accel: None,
        amd_desktop: None,
        amd_apu: None,
        amd_threadripper: None,
        amd_epyc: None,
        intel_desktop: None,
        intel_mobile: None,
        intel_xeon: None,
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
        accelerator_model: None,
        bluetooth_central: None,
        bluetooth_gatt: None,
        bluetooth_stream: None,
        bluetooth_guard: None,
        bluetooth_continuity: None,
        bluetooth_access: None,
        bluetooth_profile: None,
        colour_cue: None,
        sso_google: None,
        sso_microsoft: None,
        sso_github: None,
        sso_x: None,
        sso_meta: None,
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

#[test]
fn colour_cue_placement_integrated_cheaper_uses_shared_memory_inventory() {
    let contract = crate::colour_cue_caprichio::CONTRACT;
    let mut profile = host();
    profile.colour_cue_target = ColourCueTarget::IntegratedAmdIntel;
    profile.colour_cue_host.software_present = true;
    let request = Request {
        capabilities: vec![contract.into()],
        colour_cue: Some(ColourCueDeclaration {
            target: ColourCueTarget::IntegratedAmdIntel,
            indexed_cheaper: true,
            request_device_local: false,
        }),
        ..Request::default()
    };
    let negotiated = negotiate(&request, profile).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(negotiated[0].artifact.recipe(), "colour-cue-placement");
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("strategy=SharedMemoryInventory\n"));
    assert_eq!(negotiated, negotiate(&request, profile).unwrap());
}

// ---- SSO provider contracts (ported from sash/phase0-smoke) ----

fn sso_google() -> GoogleSsoDeclaration {
    GoogleSsoDeclaration {
        client_id: "goya-client.apps.googleusercontent.com".into(),
        redirect_uri: "https://app.example/oauth/callback".into(),
        state: "state-1234567890ab".into(),
        scopes: vec!["openid".into(), "profile".into()],
        pkce_verifier: Some("v".repeat(64)),
    }
}

fn sso_microsoft() -> MicrosoftSsoDeclaration {
    MicrosoftSsoDeclaration {
        client_id: "goya-client.example".into(),
        redirect_uri: "https://app.example/oauth/callback".into(),
        state: "state-1234567890ab".into(),
        scopes: vec!["openid".into(), "User.Read".into()],
        pkce_verifier: Some("m".repeat(48)),
    }
}

fn sso_github() -> GithubSsoDeclaration {
    GithubSsoDeclaration {
        client_id: "goya-client.example".into(),
        redirect_uri: "https://app.example/oauth/callback".into(),
        state: "state-1234567890ab".into(),
        scopes: vec!["read:user".into(), "user:email".into()],
        pkce_verifier: Some("g".repeat(56)),
    }
}

fn sso_x() -> XSsoDeclaration {
    XSsoDeclaration {
        client_id: "goya-client.example".into(),
        redirect_uri: "https://app.example/oauth/callback".into(),
        state: "state-1234567890ab".into(),
        scopes: vec!["tweet.read".into(), "users.read".into()],
        pkce_verifier: Some("x".repeat(64)),
    }
}

fn sso_meta() -> MetaSsoDeclaration {
    MetaSsoDeclaration {
        client_id: "goya-client.example".into(),
        redirect_uri: "https://app.example/oauth/callback".into(),
        state: "state-1234567890ab".into(),
        scopes: vec!["public_profile".into(), "email".into()],
        pkce_verifier: None,
    }
}

#[test]
fn sso_google_contract_registers_exactly_and_fails_closed() {
    let contract = crate::google_sso_caprichio::CONTRACT;
    assert_eq!(contract, "goya.identity.sso.google.v1");
    assert!(supported_contracts().contains(&contract));
    assert!(!supported_contracts().contains(&"identity.sso"));
    assert!(!supported_contracts().contains(&"goya.identity.sso.v1"));

    let request = Request {
        capabilities: vec![contract.into()],
        sso_google: Some(sso_google()),
        ..Request::default()
    };
    let negotiated = negotiate(&request, host()).unwrap();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].id, contract);
    assert_eq!(negotiated[0].artifact.contract(), contract);
    assert_eq!(
        negotiated[0].artifact.recipe(),
        "google-sso-authorization-code-pkce"
    );
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("provider=google\n"));
    assert!(negotiated[0]
        .artifact
        .canonical_ir()
        .contains("token_exchange=none\n"));
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
        sso_google: Some(sso_google()),
        ..Request::default()
    };
    assert_eq!(
        negotiate(&unexpected, host()),
        Err(NegotiationError::UnexpectedDeclaration {
            kind: "sso_google"
        })
    );

    let mut unsupported_target = host();
    unsupported_target.sso_google_target = GoogleSsoTarget::Node;
    assert_eq!(
        negotiate(&request, unsupported_target),
        Err(NegotiationError::GoogleSso(
            crate::google_sso_caprichio::Diagnostic::UnsupportedTarget
        ))
    );
    let mut no_agent = host();
    no_agent.sso_google_host.external_user_agent = false;
    assert_eq!(
        negotiate(&request, no_agent),
        Err(NegotiationError::GoogleSso(
            crate::google_sso_caprichio::Diagnostic::MissingExternalUserAgent
        ))
    );

    let draft = Request {
        capabilities: vec!["identity.sso".into()],
        ..Request::default()
    };
    assert_eq!(
        negotiate(&draft, host()),
        Err(NegotiationError::UnknownCapability("identity.sso".into()))
    );
}

#[test]
fn sso_provider_contracts_negotiate_deterministically_and_fail_closed() {
    let providers = [
        (
            crate::microsoft_sso_caprichio::CONTRACT,
            "goya.identity.sso.microsoft.v1",
            "microsoft-sso-authorization-code-pkce",
            "provider=microsoft\n",
            Request {
                capabilities: vec![crate::microsoft_sso_caprichio::CONTRACT.into()],
                sso_microsoft: Some(sso_microsoft()),
                ..Request::default()
            },
            NegotiationError::MicrosoftSso(
                crate::microsoft_sso_caprichio::Diagnostic::UnsupportedTarget
            ),
        ),
        (
            crate::github_sso_caprichio::CONTRACT,
            "goya.identity.sso.github.v1",
            "github-sso-authorization-code-pkce",
            "provider=github\n",
            Request {
                capabilities: vec![crate::github_sso_caprichio::CONTRACT.into()],
                sso_github: Some(sso_github()),
                ..Request::default()
            },
            NegotiationError::GithubSso(
                crate::github_sso_caprichio::Diagnostic::UnsupportedTarget
            ),
        ),
        (
            crate::x_sso_caprichio::CONTRACT,
            "goya.identity.sso.x.v1",
            "x-sso-authorization-code-pkce",
            "provider=x\n",
            Request {
                capabilities: vec![crate::x_sso_caprichio::CONTRACT.into()],
                sso_x: Some(sso_x()),
                ..Request::default()
            },
            NegotiationError::XSso(crate::x_sso_caprichio::Diagnostic::UnsupportedTarget),
        ),
        (
            crate::meta_sso_caprichio::CONTRACT,
            "goya.identity.sso.meta.v1",
            "meta-sso-authorization-code-pkce",
            "provider=meta\n",
            Request {
                capabilities: vec![crate::meta_sso_caprichio::CONTRACT.into()],
                sso_meta: Some(sso_meta()),
                ..Request::default()
            },
            NegotiationError::MetaSso(crate::meta_sso_caprichio::Diagnostic::UnsupportedTarget),
        ),
    ];
    for (contract, id, recipe, provider_line, request, unsupported) in providers {
        assert_eq!(contract, id);
        assert!(supported_contracts().contains(&contract));
        let negotiated = negotiate(&request, host()).unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].id, contract);
        assert_eq!(negotiated[0].artifact.contract(), contract);
        assert_eq!(negotiated[0].artifact.recipe(), recipe);
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains(provider_line));
        assert!(negotiated[0]
            .artifact
            .canonical_ir()
            .contains("token_exchange=none\n"));
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

        let mut cross_target = host();
        cross_target.sso_microsoft_target = MicrosoftSsoTarget::CrossPlatform;
        cross_target.sso_github_target = GithubSsoTarget::CrossPlatform;
        cross_target.sso_x_target = XSsoTarget::CrossPlatform;
        cross_target.sso_meta_target = MetaSsoTarget::CrossPlatform;
        assert_eq!(negotiate(&request, cross_target), Err(unsupported));
    }
}

