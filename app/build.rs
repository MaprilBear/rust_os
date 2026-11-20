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

    // Linker map in OUT_DIR, which cargo keeps separate per profile and feature
    // set, so the map always belongs to the link of that configuration (a
    // shared path would keep a stale map whenever cargo reuses a cached link).
    // `cargo xtask build` finds it through cargo's build-script messages.
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!(
        "cargo:rustc-link-arg-bins=-Map={}",
        out.join("rust_os.map").display()
    );
    // A cross-reference table in the map (who references each symbol), for
    // the placement check's shared-generics test.
    println!("cargo:rustc-link-arg-bins=--cref");
}
