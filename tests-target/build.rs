use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Like app/build.rs, for every suite image in src/bin: the shared linker
/// script, and a linker map per image (`<bin>.map` in OUT_DIR), which
/// `cargo xtask` reads for the placement check.
fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ld = manifest.join("../link.ld").canonicalize().unwrap();
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-link-arg-bins=-T{}", ld.display());
    // Cross-reference table in the map, for the placement check.
    println!("cargo:rustc-link-arg-bins=--cref");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bins = manifest.join("src/bin");
    println!("cargo:rerun-if-changed={}", bins.display());
    for entry in fs::read_dir(&bins).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = Path::new(path.file_stem().unwrap()).display().to_string();
        println!(
            "cargo:rustc-link-arg-bin={name}=-Map={}",
            out.join(format!("{name}.map")).display()
        );
    }
}
