use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    // link.ld lives at the workspace root, shared by the whole image.
    let root: PathBuf = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .canonicalize()
        .unwrap();
    let ld = root.join("link.ld");
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-link-arg-bins=-T{}", ld.display());

    // Linker map at <target>/<triple>/<profile>/rust_os.map, read by
    // tools/check_placement.py. OUT_DIR is <profile>/build/app-<hash>/out, so this
    // follows the profile and honours CARGO_TARGET_DIR.
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out.ancestors().nth(3).unwrap();
    println!(
        "cargo:rustc-link-arg-bins=-Map={}",
        profile_dir.join("rust_os.map").display()
    );
}
