//! Measures compiler-side normalization and source emission, not I/O throughput.
use goya_caprichios::{storage, NetworkDeclaration, RuntimeCapabilities, Target};
use std::{error::Error, hint::black_box, io, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let iterations = match std::env::args().nth(1) {
        Some(value) => value.parse::<u64>()?,
        None => 100_000,
    };
    if iterations == 0 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "iterations must be positive").into(),
        );
    }
    let network = NetworkDeclaration {
        timeout_ms: 5000,
        concurrency: 8,
        retries: 0,
        offline_queue: false,
    };
    let host = RuntimeCapabilities {
        fetch: true,
        abort_controller: true,
        url: true,
        uint8_array: true,
        timers: true,
    };
    let storage = storage::StorageDeclaration {
        namespace: "benchmark".into(),
        max_value_bytes: 4096,
        durability: storage::Durability::BestEffort,
        multi_key_transactions: false,
        encryption_at_rest: false,
    };
    let indexeddb = storage::RuntimeCapabilities { indexed_db: true };
    let build_network = || -> Result<_, Box<dyn Error>> {
        Ok(network.normalize()?.lower_checked(Target::Browser, host)?)
    };
    let build_storage = || -> Result<_, Box<dyn Error>> {
        Ok(storage
            .clone()
            .normalize()?
            .lower(storage::Target::Browser, indexeddb)?)
    };
    let expected_network = build_network()?;
    let expected_storage = build_storage()?;
    if expected_network != build_network()? || expected_storage != build_storage()? {
        return Err(io::Error::other("repeated construction produced different outputs").into());
    }
    for (name, mut build) in [
        (
            "network.browser",
            Box::new(|| Ok(black_box(build_network()?.source.len())))
                as Box<dyn FnMut() -> Result<usize, Box<dyn Error>>>,
        ),
        (
            "storage.browser",
            Box::new(|| Ok(black_box(build_storage()?.source.len())))
                as Box<dyn FnMut() -> Result<usize, Box<dyn Error>>>,
        ),
    ] {
        for _ in 0..1000 {
            black_box(build()?);
        }
        let started = Instant::now();
        let mut bytes = 0usize;
        for _ in 0..iterations {
            bytes = bytes.wrapping_add(black_box(build()?));
        }
        let elapsed = started.elapsed();
        println!("{name}: iterations={iterations} elapsed_ns={} ns_per_op={:.1} emitted_bytes_checksum={bytes}", elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
    }
    Ok(())
}
