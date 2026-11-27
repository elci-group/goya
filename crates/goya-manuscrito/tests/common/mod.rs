//! Shared fixture helpers: deterministic temp project trees for the
//! analysis-only inventory, the compiler-graph frontend, and the CLI.
//!
//! Projects are generated under the system temp directory, keyed by the test
//! tag and the process id, so parallel test binaries never collide and every
//! run starts from a clean tree.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("goya-manuscrito-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture project root");
    dir
}

pub fn write(root: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    fs::write(&path, contents).expect("write fixture file");
    path
}

/// The full inventory fixture: every recognised source kind, two tsconfig
/// files, a root manifest with all four dependency groups, and a lockfile.
pub fn full_project(tag: &str) -> PathBuf {
    let root = temp_project(tag);
    write(
        &root,
        "package.json",
        r#"{
  "name": "fixture-app",
  "version": "1.0.0",
  "dependencies": { "left-pad": "^1.3.0" },
  "devDependencies": { "typescript": "5.4.5" },
  "peerDependencies": { "react": ">=18" },
  "optionalDependencies": { "fsevents": "^2.3.2" }
}
"#,
    );
    write(
        &root,
        "tsconfig.json",
        "{ \"compilerOptions\": { \"strict\": true } }\n",
    );
    write(
        &root,
        "tsconfig.build.json",
        "{ \"extends\": \"./tsconfig.json\" }\n",
    );
    write(
        &root,
        "package-lock.json",
        "{ \"lockfileVersion\": 3, \"packages\": {} }\n",
    );
    write(&root, "src/main.ts", "export const main = 1;\n");
    write(&root, "src/panel.tsx", "export const panel = 2;\n");
    write(&root, "scripts/seed.mjs", "export const seed = 3;\n");
    write(
        &root,
        "types/app.d.ts",
        "export declare const app: number;\n",
    );
    write(&root, "legacy/util.cjs", "module.exports = {};\n");
    root
}

/// A fixture prepared for the compiler-graph frontend: a two-module import
/// cycle, a recognised Node capability call, a non-literal dynamic import,
/// and an unresolved package specifier.
pub fn graph_project(tag: &str) -> PathBuf {
    let root = temp_project(tag);
    write(
        &root,
        "package.json",
        "{ \"name\": \"fixture-graph\", \"version\": \"1.0.0\" }\n",
    );
    write(
        &root,
        "tsconfig.json",
        "{ \"compilerOptions\": { \"strict\": true, \"module\": \"commonjs\", \"target\": \"es2020\" } }\n",
    );
    write(
        &root,
        "src/main.ts",
        r#"import { readFile } from "node:fs/promises";
import { pad, loadByName } from "./util";

export const CONFIG = "cfg";

export async function run(name: string): Promise<string> {
  const padded = pad(CONFIG);
  const raw = await readFile("/etc/hostname", "utf8");
  const mod = await loadByName(name);
  return padded + raw.length + (mod as string);
}
"#,
    );
    write(
        &root,
        "src/util.ts",
        r#"import { CONFIG } from "./main";

export function pad(value: string): string {
  let acc = "";
  try {
    acc = value.trim();
  } catch (error) {
    throw error;
  }
  return acc;
}

export function loadByName(name: string): Promise<unknown> {
  return import(name);
}
"#,
    );
    root
}
