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
    // tools/check_placement.py. OUT_DIR is <profile>/build/<this package's dir>/...:
    //   classic layout:                  <profile>/build/app-<hash>/out
    //   cargo's newer build-dir layout:  <profile>/build/app/<hash>/out
    //     (seen with cargo 1.100)
    // so find the `build` directory whose child is this package's directory and
    // take its parent. This follows the profile (including custom profiles) and
    // CARGO_TARGET_DIR. If cargo's separate build directory is configured
    // (`build.build-dir`), OUT_DIR lives there, and so will the map.
    let pkg = env::var("CARGO_PKG_NAME").unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out
        .ancestors()
        .find(|dir| {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let in_build = dir.parent().and_then(Path::file_name) == Some("build".as_ref());
            in_build && (name == pkg || name.starts_with(&format!("{pkg}-")))
        })
        .and_then(|pkg_dir| pkg_dir.parent()?.parent())
        .expect("OUT_DIR is not under <profile>/build/<package>");
    println!(
        "cargo:rustc-link-arg-bins=-Map={}",
        profile_dir.join("rust_os.map").display()
    );
}
