//! Build and test automation: `cargo xtask <command>`, run from the repository
//! root (alias in .cargo/config.toml). `cargo xtask help` lists the commands.
//!
//! Every command runs its tools from the workspace root and prints each
//! invocation (`$ cargo ...`) before running it, so any step can be reproduced
//! by hand.

mod elf;
mod placement;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// Name of the firmware binary (app/Cargo.toml `[[bin]]`).
const FIRMWARE_BIN: &str = "rust_os";
/// Name of the linker map app/build.rs writes into its OUT_DIR.
const MAP_FILE: &str = "rust_os.map";

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
    let mut cargo_args = Vec::new();
    if opts.flag("--release") {
        cargo_args.push("--release".to_string());
    }
    cargo_args.extend(opts.passthrough);
    build_firmware(&cargo_args).map(drop)
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

/// The ELF and linker map one firmware build produced.
struct Firmware {
    elf: PathBuf,
    map: PathBuf,
}

/// `cargo build <cargo_args>` for the firmware, then the placement check.
/// The ELF and map paths come from cargo's own build messages, so they are
/// right for any profile, feature set, target dir or build dir.
fn build_firmware(cargo_args: &[String]) -> Result<Firmware> {
    let mut cmd = cargo();
    cmd.args(["build", "--message-format=json-render-diagnostics"])
        .args(cargo_args)
        .stdout(Stdio::piped());
    let stdout = run_capture(&mut cmd)?;

    let (mut elf, mut out_dirs) = (None, Vec::new());
    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match msg["reason"].as_str() {
            Some("compiler-artifact") if msg["target"]["name"] == FIRMWARE_BIN => {
                if let Some(exe) = msg["executable"].as_str() {
                    elf = Some((msg["package_id"].clone(), PathBuf::from(exe)));
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
    let (package, elf) = elf.ok_or("cargo did not report the firmware executable")?;
    let out_dir = out_dirs
        .into_iter()
        .find(|(p, _)| *p == package)
        .map(|(_, d)| d)
        .ok_or("cargo did not report the firmware's build script output")?;
    let firmware = Firmware {
        elf,
        map: out_dir.join(MAP_FILE),
    };
    check_placement(&firmware.map, &firmware.elf)?;
    Ok(firmware)
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

/// `cargo`, the same binary that is running this xtask when there is one.
fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Like `run`, but returns the captured stdout (stdout must be piped).
fn run_capture(cmd: &mut Command) -> Result<String> {
    let shown = prepare(cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("cannot run `{shown}`: {e}"))?;
    if !output.status.success() {
        return Err(format!("`{shown}` failed ({})", output.status));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("`{shown}`: {e}"))
}

/// Set the working directory to the workspace root, print the command, and
/// return it as shown.
fn prepare(cmd: &mut Command) -> String {
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
    shown
}
