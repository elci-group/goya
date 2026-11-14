use goya_sdk::emit_android_static_text_preview_named;
use std::{env, fs, process};

fn main() {
    let output = env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("usage: emit_android_preview <output.kt>");
        process::exit(2);
    });
    let source = r#"@ontology demo { @cell label { id: "label"; text: "Hello preview" } }"#;
    let kotlin = emit_android_static_text_preview_named(source, "GoyaStaticTextPreview")
        .unwrap_or_else(|error| {
            eprintln!("preview emission failed: {error}");
            process::exit(1);
        });
    fs::write(output, kotlin).unwrap_or_else(|error| {
        eprintln!("could not write generated Kotlin: {error}");
        process::exit(1);
    });
}
