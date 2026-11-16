use goya_sdk::emit_android_static_text_preview_named;
use std::{env, fs, fs::OpenOptions, io::Write, process};

fn usage() -> ! {
    eprintln!("usage: aragon-androidc <input.gscene> <output.kt> [KotlinClassName]");
    process::exit(2);
}

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("usage: aragon-androidc <input.gscene> <output.kt> [KotlinClassName]");
        println!("Compile the supported single-cell static-text preview to Kotlin.");
        process::exit(0);
    }
    if !(2..=3).contains(&args.len()) {
        usage();
    }

    let input = &args[0];
    let output = &args[1];
    let class_name = match args.get(2) {
        Some(arg) => arg.to_str().unwrap_or_else(|| {
            eprintln!("Kotlin class name must be UTF-8");
            process::exit(2);
        }),
        None => "GoyaStaticTextPreview",
    };
    let source = fs::read_to_string(input).unwrap_or_else(|error| {
        eprintln!("could not read scene {}: {error}", input.to_string_lossy());
        process::exit(1);
    });
    let kotlin =
        emit_android_static_text_preview_named(&source, class_name).unwrap_or_else(|error| {
            eprintln!("scene rejected: {error}");
            process::exit(1);
        });
    let mut generated = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap_or_else(|error| {
            eprintln!(
                "could not create Kotlin output {} (it must not already exist): {error}",
                output.to_string_lossy()
            );
            process::exit(1);
        });
    generated
        .write_all(kotlin.as_bytes())
        .unwrap_or_else(|error| {
            eprintln!(
                "could not write Kotlin {}: {error}",
                output.to_string_lossy()
            );
            process::exit(1);
        });
}
