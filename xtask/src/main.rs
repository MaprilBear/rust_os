//! Build and test automation: `cargo xtask <command>`, run from the repository
//! root (alias in .cargo/config.toml). `cargo xtask help` lists the commands.
//!
//! Every command runs its tools from the workspace root and prints each
//! invocation (`$ cargo ...`) before running it, so any step can be reproduced
//! by hand.

mod placement;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The firmware's target triple (also the default in .cargo/config.toml).
const FIRMWARE_TARGET: &str = "thumbv7em-none-eabihf";
/// Name of the firmware binary (app/Cargo.toml `[[bin]]`).
const FIRMWARE_BIN: &str = "rust_os";

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
        usage: "check-placement [--release] [--map <path>]\n    \
                check the placement of an already-built image",
        run: check_placement_cmd,
    },
];

fn usage() -> String {
    let mut s = String::from("usage: cargo xtask <command> [options]\n\ncommands:\n");
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
    let release = opts.flag("--release");
    let mut cmd = cargo();
    cmd.arg("build");
    if release {
        cmd.arg("--release");
    }
    cmd.args(&opts.passthrough);
    run(&mut cmd)?;
    check_placement(&map_path(release))
}

fn check_placement_cmd(args: &[String]) -> Result {
    let opts = Opts::parse(args, &["--release"], &["--map"])?;
    let map = opts
        .value("--map")
        .map_or_else(|| map_path(opts.flag("--release")), PathBuf::from);
    check_placement(&map)
}

fn check_placement(map: &Path) -> Result {
    let map_text = fs::read_to_string(map)
        .map_err(|e| format!("cannot read linker map {}: {e}", map.display()))?;
    let report = placement::check_map(&map_text);
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

/// Where app/build.rs tells the linker to write the map.
fn map_path(release: bool) -> PathBuf {
    let target =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), |d| root().join(d));
    target
        .join(FIRMWARE_TARGET)
        .join(if release { "release" } else { "debug" })
        .join(format!("{FIRMWARE_BIN}.map"))
}

/// `cargo`, the same binary that is running this xtask when there is one.
fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Print and run `cmd` in the workspace root; error if it fails.
fn run(cmd: &mut Command) -> Result {
    cmd.current_dir(root());
    // Show `cargo`, not the full toolchain path in $CARGO.
    let program = Path::new(cmd.get_program())
        .file_name()
        .unwrap_or(cmd.get_program());
    let shown = std::iter::once(program)
        .chain(cmd.get_args())
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("$ {shown}");
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run `{shown}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{shown}` failed ({status})"))
    }
}
