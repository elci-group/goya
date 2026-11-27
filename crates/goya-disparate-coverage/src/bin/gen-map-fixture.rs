//! Regenerates the embedded capability-map fixture from `default_map()`.
//! Run: cargo run -p goya-disparate-coverage --bin gen-map-fixture
fn main() {
    let map = goya_disparate_coverage::default_map();
    let mut out = serde_json::to_string_pretty(&map).expect("map serialises");
    out.push('\n');
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/capability-map-v1.json");
    std::fs::write(&path, out).expect("fixture write");
    println!("written {}", path.display());
}
