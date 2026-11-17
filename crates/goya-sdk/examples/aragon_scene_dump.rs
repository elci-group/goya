//! Print the Kotlin and source map Aragon generates for a `.gscene` file.
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: aragon_scene_dump <scene.gscene>");
    let source = std::fs::read_to_string(path).expect("readable UTF-8 scene");
    match goya_sdk::compile_android_scene(&source) {
        Ok(scene) => {
            for file in &scene.files {
                println!("===== {}\n{}", file.name, file.text);
            }
            println!("===== source map\n{}", scene.source_map.to_json());
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
