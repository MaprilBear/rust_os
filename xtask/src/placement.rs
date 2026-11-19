//! Post-link guard for the kernel/user split (formerly tools/check_placement.py).
//! Reads the lld linker map (`-Map=`, written by app/build.rs).
//!
//! Fails if
//!   * anything from the kernel crate landed in a user output section, or
//!   * anything other than the kernel crate / .ksem objects / the vector table
//!     landed in a kernel output section, or
//!   * the kernel crate contributed nothing to KFLASH. That is what happens when
//!     LTO merges every crate into one object: the link "succeeds" but the whole
//!     kernel is in user memory.
//!
//! Why a map file: the kernel crate also has #[no_mangle] symbols (`ResetHandler`,
//! `CURR_TCB`, ...) whose names don't say which crate they came from, but the map
//! records the input file each section came from.

const KERNEL_OUT: &[&str] = &[".vector_table", ".text", ".rodata", ".data", ".bss"];
const USER_OUT: &[&str] = &[".user_text", ".user_rodata", ".user_data", ".user_bss"];

/// One lld map row:  `VMA LMA Size Align <indent>Text`. Indent 0 is an output
/// section, 8 an input section, 16 a symbol.
struct Row<'a> {
    size: u64,
    indent: usize,
    text: &'a str,
}

fn parse_row(line: &str) -> Option<Row<'_>> {
    let mut rest = line.trim_start();
    let mut fields = [""; 4];
    for (i, field) in fields.iter_mut().enumerate() {
        let end = rest.find(' ')?;
        *field = &rest[..end];
        rest = &rest[end..];
        if i < 3 {
            rest = rest.trim_start();
        }
    }
    let text = rest.trim_start_matches(' ');
    let spaces = rest.len() - text.len();
    if spaces == 0 || text.is_empty() || text.starts_with(char::is_whitespace) {
        return None;
    }
    let hex = |s: &str| u64::from_str_radix(s, 16).ok();
    let (_vma, _lma, size) = (hex(fields[0])?, hex(fields[1])?, hex(fields[2])?);
    if !fields[3].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(Row {
        size,
        indent: spaces - 1,
        text,
    })
}

/// Split an input-section description `file:(section)` into its parts.
fn parse_input(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_suffix(')')?;
    let open = inner.rfind('(')?;
    let file = inner[..open].strip_suffix(':')?;
    let sec = &inner[open + 1..];
    if file.is_empty() || sec.is_empty() || sec.contains(')') {
        return None;
    }
    Some((file, sec))
}

/// Same match as the Python check: `libkernel-<lowercase hex>.rlib(`, i.e.
/// link.ld's `*libkernel-*.rlib:*` tightened to a hash.
fn is_kernel_file(file: &str) -> bool {
    file.match_indices("libkernel-").any(|(i, m)| {
        let after = &file[i + m.len()..];
        let hex_len = after
            .bytes()
            .take_while(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            .count();
        hex_len > 0 && after[hex_len..].starts_with(".rlib(")
    })
}

pub struct Report {
    pub errors: Vec<String>,
    pub kernel_text_bytes: u64,
}

pub fn check_map(map: &str) -> Report {
    let mut out: Option<&str> = None;
    let mut errors = Vec::new();
    let mut kernel_text_bytes = 0;

    for line in map.lines() {
        let Some(row) = parse_row(line) else { continue };
        if row.indent == 0 {
            out = Some(row.text);
            continue;
        }
        if row.indent != 8 || row.size == 0 {
            continue;
        }
        let (Some((file, sec)), Some(out)) = (parse_input(row.text), out) else {
            continue;
        };
        if file == "<internal>" {
            // Linker-synthesised section, e.g. a merged string/constant pool
            // (.rodata.str1.1, .rodata.cst8, ...). lld builds those per output
            // section *after* input sections were assigned by pattern, so the
            // pool in .rodata only holds kernel-crate constants. No source file.
            continue;
        }
        let is_kernel = is_kernel_file(file);
        let place = format!("{file}:({sec}) -> {out}");
        if USER_OUT.contains(&out) && is_kernel {
            errors.push(format!("kernel crate in USER memory : {place}"));
        } else if KERNEL_OUT.contains(&out) && !is_kernel {
            if out == ".vector_table" || sec.starts_with(".ksem") {
                continue; // explicit by-section-name opt-ins
            }
            errors.push(format!("non-kernel code/data in KERNEL memory: {place}"));
        }
        if out == ".text" && is_kernel {
            kernel_text_bytes += row.size;
        }
    }

    if kernel_text_bytes == 0 {
        errors.push(
            "kernel crate contributed 0 bytes to .text (KFLASH). Crate boundaries were lost \
             (LTO?) or the rlib name no longer matches *libkernel-*.rlib:* in link.ld"
                .into(),
        );
    }
    Report {
        errors,
        kernel_text_bytes,
    }
}
