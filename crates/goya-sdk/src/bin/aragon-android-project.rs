use goya_sdk::{
    emit_android_d1_runtime, emit_android_root_button_preview,
    emit_android_static_text_preview_named,
};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process,
};

const CLASS_NAME: &str = "GoyaStaticTextPreview";
const BUTTON_CLASS_NAME: &str = "GoyaRootButtonPreview";
const PACKAGE: &str = "dev.goya.preview";
fn usage() -> ! {
    eprintln!("usage: aragon-android-project <input.gscene> <output-directory> [--root-button] [--initial key=int:value|text:value|bool:value ...]");
    process::exit(2);
}

fn validate_layout(files: &[(&str, String)]) -> Result<(), (PathBuf, std::io::Error)> {
    let mut paths = HashSet::new();
    for (relative, _) in files {
        let relative = Path::new(relative);
        let valid = !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)));
        if !valid {
            return Err((
                relative.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "generated paths must be non-empty relative paths without dot segments",
                ),
            ));
        }
        if paths.contains(relative)
            || paths
                .iter()
                .any(|path: &PathBuf| path.starts_with(relative))
        {
            return Err((
                relative.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "generated file paths conflict",
                ),
            ));
        }
        if let Some(parent) = relative.parent() {
            if paths.contains(parent) {
                return Err((
                    relative.to_path_buf(),
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "generated file path conflicts with an earlier file's parent directory",
                    ),
                ));
            }
        }
        paths.insert(relative.to_path_buf());
    }
    Ok(())
}

fn create_project(root: &Path, files: &[(&str, String)]) -> Result<(), (PathBuf, std::io::Error)> {
    validate_layout(files)?;
    fs::create_dir(root).map_err(|error| (root.to_path_buf(), error))?;

    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| (parent.to_path_buf(), error))?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| (path.clone(), error))?;
        file.write_all(contents.as_bytes())
            .map_err(|error| (path.clone(), error))?;
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("usage: aragon-android-project <input.gscene> <output-directory> [--root-button] [--initial key=int:value|text:value|bool:value ...]");
        println!("Generate a deterministic Android starter app for a static-text or root-button D1 preview.");
        process::exit(0);
    }
    if args.len() < 2 {
        usage();
    }

    let mut root_button = false;
    let mut initial = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--root-button" && !root_button {
            root_button = true;
            i += 1;
        } else if args[i] == "--initial" && root_button && i + 1 < args.len() {
            let specification = args[i + 1].to_str().unwrap_or_else(|| {
                eprintln!("initial state must be UTF-8");
                process::exit(2);
            });
            let (key, value) = specification.split_once('=').unwrap_or_else(|| {
                eprintln!("invalid initial state {specification:?}; expected key=type:value");
                process::exit(2);
            });
            let (kind, value) = value.split_once(':').unwrap_or_else(|| {
                eprintln!("invalid initial state {specification:?}; expected key=type:value");
                process::exit(2);
            });
            if key.is_empty() {
                eprintln!("initial state key must not be empty");
                process::exit(2);
            }
            let parsed = match kind {
                "int" => value
                    .parse::<i64>()
                    .map(|value| {
                        if value == i64::MIN {
                            "IntValue(Long.MIN_VALUE)".to_owned()
                        } else {
                            format!("IntValue({value}L)")
                        }
                    })
                    .map_err(|_| "integer initial state must fit signed 64-bit".to_owned()),
                "text" => Ok(format!("TextValue({})", kotlin_string_literal(value))),
                "bool" if value == "true" || value == "false" => Ok(format!("BoolValue({value})")),
                "bool" => Err("boolean initial state must be true or false".to_owned()),
                _ => Err("initial state type must be int, text, or bool".to_owned()),
            }
            .unwrap_or_else(|message| {
                eprintln!("invalid initial state {specification:?}: {message}");
                process::exit(2);
            });
            if initial.insert(key.to_owned(), parsed).is_some() {
                eprintln!("duplicate initial state key {key:?}");
                process::exit(2);
            }
            i += 2;
        } else {
            usage();
        }
    }
    if !root_button && !initial.is_empty() {
        usage();
    }

    let source = fs::read_to_string(&args[0]).unwrap_or_else(|error| {
        eprintln!(
            "could not read scene {}: {error}",
            args[0].to_string_lossy()
        );
        process::exit(1);
    });
    let (preview, runtime, main_activity) = if root_button {
        let preview = emit_android_root_button_preview(&source).unwrap_or_else(|error| {
            eprintln!("scene rejected: {error}");
            process::exit(1);
        });
        let runtime = emit_android_d1_runtime(&source).unwrap_or_else(|error| {
            eprintln!("D1 runtime rejected: {error}");
            process::exit(1);
        });
        validate_initial_state(&source, &initial).unwrap_or_else(|error| {
            eprintln!("initial state rejected: {error}");
            process::exit(1);
        });
        let initial_entries = initial
            .iter()
            .map(|(key, value)| {
                format!(
                    "{} to GoyaD1Runtime.Value.{value}",
                    kotlin_string_literal(key)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let main = format!(
            "package {PACKAGE}\n\nimport android.app.Activity\nimport android.os.Bundle\n\nclass MainActivity : Activity() {{\n    private lateinit var d1: GoyaD1Runtime\n\n    override fun onCreate(savedInstanceState: Bundle?) {{\n        super.onCreate(savedInstanceState)\n        d1 = GoyaD1Runtime(mapOf({initial_entries}))\n        setContentView({BUTTON_CLASS_NAME}(this) {{ actionId ->\n            d1.dispatch(actionId, emptyMap())\n        }})\n    }}\n}}\n"
        );
        (preview, runtime, main)
    } else {
        let preview =
            emit_android_static_text_preview_named(&source, CLASS_NAME).unwrap_or_else(|error| {
                eprintln!("scene rejected: {error}");
                process::exit(1);
            });
        let main = format!("package {PACKAGE}\n\nimport android.app.Activity\nimport android.os.Bundle\n\nclass MainActivity : Activity() {{\n    override fun onCreate(savedInstanceState: Bundle?) {{\n        super.onCreate(savedInstanceState)\n        setContentView({CLASS_NAME}(this))\n    }}\n}}\n");
        (preview, String::new(), main)
    };
    let root = Path::new(&args[1]);
    let preview_name = if root_button {
        BUTTON_CLASS_NAME
    } else {
        CLASS_NAME
    };
    let project_name = if root_button {
        "GoyaRootButtonD1"
    } else {
        "GoyaStaticTextPreview"
    };
    let preview_file = format!("app/src/main/java/dev/goya/preview/{preview_name}.kt");
    let mut files = vec![
        ("settings.gradle.kts", format!("pluginManagement {{ repositories {{ google(); mavenCentral(); gradlePluginPortal() }} }}\ndependencyResolutionManagement {{ repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories {{ google(); mavenCentral() }} }}\nrootProject.name = \"{project_name}\"\ninclude(\":app\")\n")),
        ("build.gradle.kts", "plugins { id(\"com.android.application\") version \"8.7.3\" apply false; id(\"org.jetbrains.kotlin.android\") version \"2.0.21\" apply false }\n".to_owned()),
        ("gradle.properties", "org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8\nandroid.useAndroidX=true\nkotlin.code.style=official\n".to_owned()),
        ("app/build.gradle.kts", "plugins { id(\"com.android.application\"); id(\"org.jetbrains.kotlin.android\") }\n\nandroid { namespace = \"dev.goya.preview\"; compileSdk = 35; defaultConfig { applicationId = \"dev.goya.preview\"; minSdk = 23; targetSdk = 35; versionCode = 1; versionName = \"1.0\" } }\n".to_owned()),
        ("app/src/main/AndroidManifest.xml", "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\"><application android:theme=\"@android:style/Theme.Material.Light\" android:label=\"Goya preview\"><activity android:name=\"dev.goya.preview.MainActivity\" android:exported=\"true\"><intent-filter><action android:name=\"android.intent.action.MAIN\"/><category android:name=\"android.intent.category.LAUNCHER\"/></intent-filter></activity></application></manifest>\n".to_owned()),
        (preview_file.as_str(), format!("package {PACKAGE}\n\n{preview}")),
        ("app/src/main/java/dev/goya/preview/MainActivity.kt", main_activity),
    ];
    if root_button {
        files.push((
            "app/src/main/java/dev/goya/preview/GoyaD1Runtime.kt",
            format!("package {PACKAGE}\n\n{runtime}"),
        ));
    }
    create_project(root, &files).unwrap_or_else(|(path, error)| {
        eprintln!(
            "could not create project output at {} (destination must not already exist): {error}",
            path.display()
        );
        process::exit(1);
    });
}

fn kotlin_string_literal(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '$' => output.push_str("\\$"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() || !c.is_ascii() => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units).iter() {
                    output.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn validate_initial_state(source: &str, initial: &BTreeMap<String, String>) -> Result<(), String> {
    let scene = goya_lang::parse_strict(source)
        .map_err(|error| error.to_string())?
        .scene;
    let mut schema = BTreeMap::new();
    for declaration in &scene.interactions {
        for key in declaration
            .state_reads
            .iter()
            .chain(&declaration.state_writes)
        {
            if let Some(existing) = schema.insert(key.name.clone(), key.value_type) {
                if existing != key.value_type {
                    return Err(format!("conflicting state types for {:?}", key.name));
                }
            }
        }
    }
    if initial.len() != schema.len() {
        return Err(format!(
            "expected explicit values for exactly these state keys: {}",
            schema.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    for (key, kind) in schema {
        let value = initial
            .get(&key)
            .ok_or_else(|| format!("missing explicit value for state key {key:?}"))?;
        let expected = match kind {
            goya_lang::InteractionType::Int => "IntValue(",
            goya_lang::InteractionType::Text => "TextValue(",
            goya_lang::InteractionType::Bool => "BoolValue(",
        };
        if !value.starts_with(expected) {
            return Err(format!("state key {key:?} requires type {kind:?}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::create_project;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn rejects_conflicting_generated_paths_before_creating_output() {
        let root = std::env::temp_dir().join(format!(
            "goya-android-project-writer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let files = [
            ("first.txt", "would be written".to_owned()),
            ("block", "file".to_owned()),
            ("block/child.txt", "conflicts with file".to_owned()),
        ];

        let result = create_project(&root, &files);
        assert!(result.is_err());
        assert!(!root.exists());
    }
}
