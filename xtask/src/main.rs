//! Build and test automation: `cargo xtask <command>`, run from the repository
//! root (alias in .cargo/config.toml). `cargo xtask help` lists the commands.
//!
//! Every command runs its tools from the workspace root and prints each
//! invocation (`$ cargo ...`) before running it, so any step can be reproduced
//! by hand.

mod elf;
mod placement;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// Name of the firmware binary (app/Cargo.toml `[[bin]]`).
const FIRMWARE_BIN: &str = "rust_os";
/// The package with the on-target test images (one binary per suite).
const SUITES_PACKAGE: &str = "tests-target";
/// Crates whose unit tests run on the host.
const HOST_TEST_CRATES: &[&str] = &["kernel", "userlib", "drivers"];

type Result<T = ()> = std::result::Result<T, String>;

/// A command: name, one-line usage, and the function that runs it.
struct Cmd {
    name: &'static str,
    usage: &'static str,
    run: fn(&[String]) -> Result,
}

const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "build",
        usage: "build [--release] [-- <cargo build args>]\n    \
                build the firmware, then check kernel/user placement",
        run: build,
    },
    Cmd {
        name: "check-placement",
        usage: "check-placement --map <path> --elf <path>\n    \
                check the placement of an already-built image",
        run: check_placement_cmd,
    },
    Cmd {
        name: "test-host",
        usage: "test-host [-- <test binary args>]\n    \
                unit/property tests of kernel, userlib and drivers on the host, then\n    \
                the kernel again with its `ktest` feature;\n    \
                args after -- go to the test binaries (a name filter, --nocapture, ...)",
        run: test_host,
    },
    Cmd {
        name: "test-loom",
        usage: "test-loom [-- <test binary args>]\n    \
                loom model checks of the drivers' lock-free code (--cfg loom, release)",
        run: test_loom,
    },
    Cmd {
        name: "miri",
        usage: "miri [-- <test binary args>]\n    \
                the host tests of kernel, userlib and drivers under Miri (nightly)",
        run: miri,
    },
    Cmd {
        name: "fuzz",
        usage: "fuzz [<target>] [--time <seconds>] [-- <libFuzzer args>]\n    \
                coverage-guided fuzzing (cargo-fuzz, nightly); default target\n    \
                `syscalls`, 60 s. Crashes land in fuzz/artifacts/<target>/",
        run: fuzz,
    },
    Cmd {
        name: "test-renode",
        usage: "test-renode [-- <renode-test args> <suite.robot>...]\n    \
                build the app and the tests-target images (debug) and run the Robot\n    \
                suites in Renode: renode/*.robot and renode/tests/**/*.robot, or only\n    \
                the .robot files given after -- (e.g. --variable SEED:7)",
        run: test_renode,
    },
    Cmd {
        name: "test",
        usage: "test\n    test-host, test-loom, then test-renode",
        run: test_all,
    },
    Cmd {
        name: "clippy",
        usage: "clippy [-- <lint args>]\n    \
                clippy for the firmware, the host builds (with tests) and xtask",
        run: clippy,
    },
    Cmd {
        name: "fmt",
        usage: "fmt [--check]\n    rustfmt the workspace, xtask and fuzz",
        run: fmt,
    },
];

fn usage() -> String {
    let mut s = String::from(
        "usage: cargo xtask <command> [options]   (run from the repository root)\n\ncommands:\n",
    );
    for cmd in COMMANDS {
        s.push_str(&format!("  {}\n", cmd.usage));
    }
    s.push_str("  help\n    show this text\n");
    s
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (name, rest) = match args.split_first() {
        Some((n, r)) => (n.as_str(), r),
        None => ("help", &[][..]),
    };
    let result = match (name, COMMANDS.iter().find(|c| c.name == name)) {
        (_, Some(cmd)) => (cmd.run)(rest),
        ("help" | "-h" | "--help", None) if rest.is_empty() => {
            print!("{}", usage());
            Ok(())
        }
        _ => Err(format!(
            "unknown command or arguments: {}\n\n{}",
            args.join(" "),
            usage()
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Commands
///////////////////////////////////////////////////////////////////////////////////////////////////

fn build(args: &[String]) -> Result {
    let opts = Opts::parse(args, &["--release"], &[])?;
    let mut cargo_args = Vec::new();
    if opts.flag("--release") {
        cargo_args.push("--release".to_string());
    }
    cargo_args.extend(opts.passthrough);
    build_firmware(&cargo_args).map(drop)
}

fn test_host(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &[])?;
    let mut cmd = cargo();
    // --no-fail-fast: every crate's tests run even if an earlier crate fails
    // (`-- --ignored` fails in each crate with known issues). --all-targets
    // leaves out doc tests: the docs' examples are `ignore`d sketches, which
    // `--ignored` would otherwise try to compile.
    cmd.args([
        "test",
        "--no-fail-fast",
        "--all-targets",
        "--target",
        "host-tuple",
    ]);
    for krate in HOST_TEST_CRATES {
        cmd.args(["-p", krate]);
    }
    if !opts.passthrough.is_empty() {
        cmd.arg("--").args(&opts.passthrough);
    }
    run(&mut cmd)?;

    // The kernel again as on-target test images build it (feature `ktest`:
    // faulting threads are parked, expected faults).
    let mut ktest = cargo();
    ktest
        .args([
            "test",
            "--no-fail-fast",
            "--all-targets",
            "--target",
            "host-tuple",
        ])
        .args(["-p", "kernel", "--features", "ktest"]);
    if !opts.passthrough.is_empty() {
        ktest.arg("--").args(&opts.passthrough);
    }
    run(&mut ktest)
}

fn test_loom(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &[])?;
    let mut cmd = cargo();
    // Its own target dir: --cfg loom changes every crate's build, and sharing
    // target/ would rebuild the host tests from scratch each time.
    cmd.args(["test", "--release", "--target", "host-tuple"])
        .args(["-p", "drivers", "--lib", "--target-dir"])
        .arg(root().join("target/loom"))
        .env("RUSTFLAGS", "--cfg loom");
    if !opts.passthrough.is_empty() {
        cmd.arg("--").args(&opts.passthrough);
    }
    run(&mut cmd)
}

/// Toolchain for Miri and cargo-fuzz, pinned so results do not change under
/// us. Keep in sync with the Dockerfile.
const NIGHTLY: &str = "+nightly-2026-11-19";

fn miri(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &[])?;
    // `cargo +nightly` needs rustup's proxy, not the stable cargo in $CARGO.
    let mut cmd = Command::new("cargo");
    cmd.args([NIGHTLY, "miri", "test", "--no-fail-fast", "--all-targets"])
        .args(["--target", &host_triple()?]);
    for krate in HOST_TEST_CRATES {
        cmd.args(["-p", krate]);
    }
    if !opts.passthrough.is_empty() {
        cmd.arg("--").args(&opts.passthrough);
    }
    run(&mut cmd)
}

fn fuzz(args: &[String]) -> Result {
    // An optional target name first, then options.
    let (target, rest) = match args.first() {
        Some(t) if !t.starts_with('-') => (t.as_str(), &args[1..]),
        _ => ("syscalls", args),
    };
    let opts = Opts::parse(rest, &[], &["--time"])?;
    let secs = opts.value("--time").unwrap_or("60");
    let mut cmd = Command::new("cargo");
    cmd.args([NIGHTLY, "fuzz", "run", "--fuzz-dir", "fuzz", target, "--"])
        .arg(format!("-max_total_time={secs}"))
        .args(&opts.passthrough);
    run(&mut cmd)
}

/// The host's target triple, from `rustc -vV`.
fn host_triple() -> Result<String> {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| format!("cannot run rustc: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .ok_or_else(|| "rustc -vV printed no host triple".into())
}

fn test_renode(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &[])?;
    // Debug builds with default features: the suites read the debug console.
    let firmware = build_firmware(&[])?;
    let images = build_suite_images()?;
    // All suite images land in one directory (cargo's per-profile dir).
    let image_dir = images
        .first()
        .and_then(|i| i.elf.parent())
        .ok_or("no on-target test images were built")?
        .to_path_buf();

    // .robot files after -- replace the discovered suites; relative paths are
    // relative to where xtask was started.
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    let (given, renode_args): (Vec<&String>, Vec<&String>) = opts
        .passthrough
        .iter()
        .partition(|a| Path::new(a).extension() == Some(OsStr::new("robot")));
    let suites = if given.is_empty() {
        robot_suites()?
    } else {
        given.into_iter().map(|s| cwd.join(s)).collect()
    };
    if suites.is_empty() {
        return Err("no Robot suites (*.robot) found under renode/".into());
    }

    // renode-test writes Robot's reports (log.html, report.html,
    // robot_output.xml) to its working directory, whatever --results-dir says,
    // so run it inside the results directory.
    let results = root().join("target").join("renode-results");
    let _ = fs::remove_dir_all(&results);
    fs::create_dir_all(&results).map_err(|e| format!("{}: {e}", results.display()))?;
    let mut cmd = Command::new("renode-test");
    cmd.current_dir(&results)
        .arg("--results-dir")
        .arg(&results)
        .arg("--variable")
        .arg(format!("ELF:{}", firmware.elf.display()))
        .arg("--variable")
        .arg(format!("KTEST_DIR:{}", image_dir.display()))
        .args(renode_args)
        .args(&suites);
    eprintln!("(Robot reports: {})", results.display());
    run(&mut cmd)
}

fn test_all(args: &[String]) -> Result {
    Opts::parse(args, &[], &[])?;
    test_host(&[])?;
    test_loom(&[])?;
    test_renode(&[])
}

fn clippy(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &[])?;
    let lint_args = |cmd: &mut Command| {
        if !opts.passthrough.is_empty() {
            cmd.arg("--").args(&opts.passthrough);
        }
    };

    // Firmware (default target from .cargo/config.toml).
    let mut firmware = cargo();
    firmware.args(["clippy", "--workspace"]);
    lint_args(&mut firmware);
    run(&mut firmware)?;

    // Host builds, including the tests.
    let mut host = cargo();
    host.args(["clippy", "--all-targets", "--target", "host-tuple"]);
    for krate in HOST_TEST_CRATES {
        host.args(["-p", krate]);
    }
    lint_args(&mut host);
    run(&mut host)?;

    // The kernel as on-target test images build it.
    let mut ktest = cargo();
    ktest
        .args(["clippy", "--all-targets", "--target", "host-tuple"])
        .args(["-p", "kernel", "--features", "ktest"]);
    lint_args(&mut ktest);
    run(&mut ktest)?;

    // The drivers as `test-loom` builds them.
    let mut loom = cargo();
    loom.args([
        "clippy",
        "--tests",
        "--target",
        "host-tuple",
        "-p",
        "drivers",
    ])
    .arg("--target-dir")
    .arg(root().join("target/loom"))
    .env("RUSTFLAGS", "--cfg loom");
    lint_args(&mut loom);
    run(&mut loom)?;

    let mut xtask = cargo();
    xtask.args(["clippy", "--target", "host-tuple", "--manifest-path"]);
    xtask.arg(root().join("xtask/Cargo.toml"));
    lint_args(&mut xtask);
    run(&mut xtask)
}

fn fmt(args: &[String]) -> Result {
    let opts = Opts::parse(args, &["--check"], &[])?;
    for manifest in [
        None,
        Some(root().join("xtask/Cargo.toml")),
        Some(root().join("fuzz/Cargo.toml")),
    ] {
        let mut cmd = cargo();
        cmd.arg("fmt");
        match manifest {
            None => cmd.arg("--all"),
            Some(m) => cmd.arg("--manifest-path").arg(m),
        };
        if opts.flag("--check") {
            cmd.args(["--", "--check"]);
        }
        run(&mut cmd)?;
    }
    Ok(())
}

fn check_placement_cmd(args: &[String]) -> Result {
    let opts = Opts::parse(args, &[], &["--map", "--elf"])?;
    let (Some(map), Some(elf)) = (opts.value("--map"), opts.value("--elf")) else {
        return Err(format!(
            "check-placement needs --map and --elf\n\n{}",
            usage()
        ));
    };
    check_placement(Path::new(map), Path::new(elf))
}

/// The ELF and linker map of one firmware image.
struct Firmware {
    name: String,
    elf: PathBuf,
    map: PathBuf,
}

/// `cargo build <cargo_args>` for the firmware (the app), then the placement
/// check.
fn build_firmware(cargo_args: &[String]) -> Result<Firmware> {
    build_images(cargo_args)?
        .into_iter()
        .find(|f| f.name == FIRMWARE_BIN)
        .ok_or_else(|| "cargo did not report the firmware executable".into())
}

/// The on-target test images (tests-target, one per suite), debug, each
/// placement-checked.
fn build_suite_images() -> Result<Vec<Firmware>> {
    build_images(&["-p".into(), SUITES_PACKAGE.into(), "--bins".into()])
}

/// `cargo build <cargo_args>`, then the placement check of every binary it
/// built. Each binary's build script writes `<bin>.map` into its OUT_DIR. The
/// paths come from cargo's own build messages, so they are right for any
/// profile, feature set, target dir or build dir.
fn build_images(cargo_args: &[String]) -> Result<Vec<Firmware>> {
    let mut cmd = cargo();
    cmd.args(["build", "--message-format=json-render-diagnostics"])
        .args(cargo_args)
        .stdout(Stdio::piped());
    let stdout = run_capture(&mut cmd)?;

    let (mut bins, mut out_dirs) = (Vec::new(), Vec::new());
    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match msg["reason"].as_str() {
            Some("compiler-artifact") => {
                let is_bin = msg["target"]["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "bin"));
                if let (true, Some(exe), Some(name)) = (
                    is_bin,
                    msg["executable"].as_str(),
                    msg["target"]["name"].as_str(),
                ) {
                    bins.push((
                        msg["package_id"].clone(),
                        name.to_string(),
                        PathBuf::from(exe),
                    ));
                }
            }
            Some("build-script-executed") => {
                if let Some(dir) = msg["out_dir"].as_str() {
                    out_dirs.push((msg["package_id"].clone(), PathBuf::from(dir)));
                }
            }
            _ => {}
        }
    }
    let mut images = Vec::new();
    for (package, name, elf) in bins {
        let out_dir = out_dirs
            .iter()
            .find(|(p, _)| *p == package)
            .map(|(_, d)| d)
            .ok_or_else(|| format!("cargo did not report {name}'s build script output"))?;
        let image = Firmware {
            map: out_dir.join(format!("{name}.map")),
            name,
            elf,
        };
        check_placement(&image.map, &image.elf)?;
        images.push(image);
    }
    Ok(images)
}

fn check_placement(map: &Path, elf: &Path) -> Result {
    let map_text = fs::read_to_string(map)
        .map_err(|e| format!("cannot read linker map {}: {e}", map.display()))?;
    let elf_data = fs::read(elf).map_err(|e| format!("cannot read ELF {}: {e}", elf.display()))?;

    let report = placement::check_map(&map_text);
    placement::check_map_matches_elf(&report, &elf_data)
        .map_err(|e| format!("{} vs {}: {e}", map.display(), elf.display()))?;

    for e in &report.errors {
        eprintln!("PLACEMENT ERROR: {e}");
    }
    if !report.errors.is_empty() {
        return Err(format!(
            "{} placement error(s) in {}",
            report.errors.len(),
            map.display()
        ));
    }
    println!(
        "placement OK ({} bytes of kernel text in KFLASH)",
        report.kernel_text_bytes
    );
    Ok(())
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Helpers
///////////////////////////////////////////////////////////////////////////////////////////////////

/// Parsed command options: known boolean flags, known `--key value` options,
/// and everything after `--`.
struct Opts {
    flags: Vec<String>,
    values: Vec<(String, String)>,
    passthrough: Vec<String>,
}

impl Opts {
    fn parse(args: &[String], flags: &[&str], valued: &[&str]) -> Result<Self> {
        let mut opts = Opts {
            flags: Vec::new(),
            values: Vec::new(),
            passthrough: Vec::new(),
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == "--" {
                opts.passthrough.extend(it.cloned());
                break;
            } else if flags.contains(&arg.as_str()) {
                opts.flags.push(arg.clone());
            } else if valued.contains(&arg.as_str()) {
                let v = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                opts.values.push((arg.clone(), v.clone()));
            } else {
                return Err(format!("unexpected argument `{arg}`\n\n{}", usage()));
            }
        }
        Ok(opts)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The workspace root (this crate lives in `<root>/xtask`).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is not inside the workspace")
        .to_path_buf()
}

/// `renode/*.robot` and `renode/tests/**/*.robot`, sorted.
fn robot_suites() -> Result<Vec<PathBuf>> {
    fn collect(dir: &Path, recurse: bool, out: &mut Vec<PathBuf>) -> Result {
        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(());
        };
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() && recurse {
                collect(&path, true, out)?;
            } else if path.extension() == Some(OsStr::new("robot")) {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut suites = Vec::new();
    collect(&root().join("renode"), false, &mut suites)?;
    collect(&root().join("renode/tests"), true, &mut suites)?;
    suites.sort();
    Ok(suites)
}

/// `cargo`, the same binary that is running this xtask when there is one.
fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Print and run `cmd` (in the workspace root unless it already has a working
/// directory). If it fails, exit with its exit code, so callers such as CI can
/// tell "tests failed" from other errors.
fn run(cmd: &mut Command) -> Result {
    let shown = prepare(cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run `{shown}`: {e}"))?;
    exit_on_failure(&shown, status);
    Ok(())
}

/// Like `run`, but returns the captured stdout (stdout must be piped).
fn run_capture(cmd: &mut Command) -> Result<String> {
    let shown = prepare(cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("cannot run `{shown}`: {e}"))?;
    exit_on_failure(&shown, output.status);
    String::from_utf8(output.stdout).map_err(|e| format!("`{shown}`: {e}"))
}

fn exit_on_failure(shown: &str, status: std::process::ExitStatus) {
    if !status.success() {
        eprintln!("xtask: `{shown}` failed ({status})");
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Quote `arg` for a POSIX shell if it needs it, so printed commands can be
/// pasted back.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./=:+,@%".contains(&b));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// Set the working directory to the workspace root (unless already set),
/// print the command, and return it as shown.
fn prepare(cmd: &mut Command) -> String {
    if cmd.get_current_dir().is_none() {
        cmd.current_dir(root());
    }
    // Show `cargo`, not the full toolchain path in $CARGO.
    let program = Path::new(cmd.get_program())
        .file_name()
        .unwrap_or(cmd.get_program());
    let shown = std::iter::once(program)
        .chain(cmd.get_args())
        .map(|a| shell_quote(&a.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("$ {shown}");
    shown
}
