//! Embeds every `.glb` under `assets/models/` in the binary, so artists add or
//! replace models just by changing files (the game needs no code change, and
//! the web build needs no separate downloads).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "glb") {
            let rel = path.strip_prefix(root).unwrap().with_extension("");
            out.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/models");
    println!("cargo:rerun-if-changed=assets/models");
    println!("cargo:rerun-if-changed=build.rs");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    let mut code =
        String::from("/// Every model file: its path under assets/models without the extension, and its bytes.\n");
    code.push_str("pub const MODEL_FILES: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        code.push_str(&format!(
            "    ({name:?}, include_bytes!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    code.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("model_files.rs"), code).unwrap();
}
