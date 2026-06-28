//! Bakes the shipped content into the binary so a release exe is self-contained
//! (no `assets/` folder needed at runtime). At build time we scan the asset folders
//! and emit two const tables — read back via `include!` in `src/world.rs`:
//!
//! - `EMBEDDED_STORY_MAPS: &[(&str, &str)]` — `(room_name, ron_text)` for `assets/maps/`.
//! - `EMBEDDED_SPRITES: &[(&str, &[u8])]` — `(file_name, png_bytes)` for `assets/sprites/`.
//! - `EMBEDDED_SOUNDS: &[(&str, &[u8])]` — `(file_name, ogg_bytes)` for `assets/sounds/`.
//! - `EMBEDDED_MUSIC: &[(&str, &[u8])]` — `(file_name, ogg_bytes)` for `assets/music/`.
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
    emit_nested(&mut code, "EMBEDDED_SCENERY", "assets/scenery", ".png");
    emit(
        &mut code,
        "EMBEDDED_SOUNDS",
        "&[u8]",
        "include_bytes!",
        "assets/sounds",
        ".ogg",
    );
    emit(
        &mut code,
        "EMBEDDED_MUSIC",
        "&[u8]",
        "include_bytes!",
        "assets/music",
        ".ogg",
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

/// Like [`emit`] but one level deep: every `dir/<sub>/<file><suffix>` becomes a
/// `("<sub>/<file>", include_bytes!(..))` entry — so scenery sets live in their own
/// folders and are keyed `"forest_meadow/far.png"`.
fn emit_nested(code: &mut String, name: &str, dir: &str, suffix: &str) {
    println!("cargo:rerun-if-changed={dir}");
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(subs) = fs::read_dir(dir) {
        for sub in subs.flatten() {
            if !sub.path().is_dir() {
                continue;
            }
            let folder = sub.file_name().to_string_lossy().into_owned();
            println!("cargo:rerun-if-changed={}", sub.path().display());
            if let Ok(files) = fs::read_dir(sub.path()) {
                for file in files.flatten() {
                    let fname = file.file_name().to_string_lossy().into_owned();
                    if fname.ends_with(suffix) {
                        let abs = file.path().canonicalize().expect("canonicalize asset path");
                        entries.push((
                            format!("{folder}/{fname}"),
                            abs.to_string_lossy().into_owned(),
                        ));
                        println!("cargo:rerun-if-changed={}", file.path().display());
                    }
                }
            }
        }
    }
    entries.sort();
    writeln!(code, "pub(crate) const {name}: &[(&str, &[u8])] = &[").unwrap();
    for (key, path) in &entries {
        writeln!(code, "    ({key:?}, include_bytes!({path:?})),").unwrap();
    }
    writeln!(code, "];").unwrap();
}
