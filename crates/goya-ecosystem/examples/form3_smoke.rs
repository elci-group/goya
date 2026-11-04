use goya_ecosystem::{Capability, CapabilityGrant, Form3Agent};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: form3_smoke <3form-agent executable>")?;
    let grant = CapabilityGrant::new([Capability::FormatOutput]);
    let reply = Form3Agent::new(executable).request(
        r#"{"op":"style","text":"Goya","fg":"cyan","attrs":["bold"]}"#,
        &grant,
    )?;
    println!("3form reply: {reply}");
    Ok(())
}
