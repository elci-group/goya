use goya_ecosystem::{Capability, CapabilityGrant, TheosisClient};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let executable = args.next().ok_or("missing theosis executable")?;
    let remote = args
        .next()
        .ok_or("missing Git remote or local repository path")?;
    let binary = args.next().ok_or("missing installed application binary")?;
    let grant = CapabilityGrant::new([Capability::InspectUpdate]);
    let report = TheosisClient::new(executable).check(&remote, &binary, &grant)?;
    println!("Theosis check: {}", report.trim());
    Ok(())
}
