//! Bakes the shipped content into the binary so a release exe is self-contained
//! (no `assets/` folder needed at runtime). At build time we scan the asset folders
//! and emit two const tables — read back via `include!` in `src/world.rs`:
//!
//! - `EMBEDDED_STORY_MAPS: &[(&str, &str)]` — `(room_name, ron_text)` for `assets/maps/`.
//! - `EMBEDDED_SPRITES: &[(&str, &[u8])]` — `(file_name, png_bytes)` for `assets/sprites/`.
//!
//! Each entry uses `include_str!`/`include_bytes!` on the file's absolute path, so the
//! bytes are embedded and edits re-trigger the build.

use std::fmt::Write as _;
use std::{env, fs, path::Path};

fn main() {
    let mut code = String::new();
    emit(
        &mut code,
        "EMBEDDED_STORY_MAPS",
        "&str",
        "include_str!",
        "assets/maps",
        ".map.ron",
    );
    emit(
        &mut code,
        "EMBEDDED_SPRITES",
        "&[u8]",
        "include_bytes!",
        "assets/sprites",
        ".png",
    );
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("embedded_assets.rs");
    fs::write(out, code).expect("write embedded_assets.rs");
}

/// Append one `const <name>: &[(&str, <ty>)]` table for every file in `dir` whose
/// name ends in `suffix`, embedding it with `macro_name` at its absolute path. For
/// maps the key strips `.map.ron` (the room name); for sprites it's the full file name.
fn emit(code: &mut String, name: &str, ty: &str, macro_name: &str, dir: &str, suffix: &str) {
    println!("cargo:rerun-if-changed={dir}");
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if let Some(stripped) = file.strip_suffix(suffix) {
                let key = if suffix == ".map.ron" {
                    stripped.to_string()
                } else {
                    file.clone()
                };
                let abs = entry
                    .path()
                    .canonicalize()
                    .expect("canonicalize asset path");
                entries.push((key, abs.to_string_lossy().into_owned()));
                println!("cargo:rerun-if-changed={}", entry.path().display());
            }
        }
    }
    entries.sort();
    writeln!(code, "pub(crate) const {name}: &[(&str, {ty})] = &[").unwrap();
    for (key, path) in &entries {
        writeln!(code, "    ({key:?}, {macro_name}({path:?})),").unwrap();
    }
    writeln!(code, "];").unwrap();
}
