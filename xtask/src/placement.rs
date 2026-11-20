//! Post-link guard for the kernel/user split (formerly tools/check_placement.py).
//! Reads the lld linker map (`-Map=`, written by app/build.rs).
//!
//! Fails if
//!   * anything from the kernel crate landed in a user output section, or
//!   * anything other than the kernel crate / .ksem objects / the vector table
//!     landed in a kernel output section, or
//!   * the kernel crate contributed nothing to KFLASH. That is what happens when
//!     LTO merges every crate into one object: the link "succeeds" but the whole
//!     kernel is in user memory, or
//!   * the map does not describe the ELF it is checked with (a stale map from
//!     an older link would otherwise make the whole check meaningless), or
//!   * user code references a generic instantiated in the kernel crate (from
//!     the map's cross-reference table, `--cref`). rustc shares generic
//!     instances with downstream crates when optimising little (share-generics),
//!     so e.g. `core::str::Bytes::into_iter` compiled for the kernel would be
//!     reused by a user crate. That copy sits in KFLASH, and an unprivileged
//!     thread that calls it takes a MemManage fault.
//!
//! Why a map file: the kernel crate also has #[no_mangle] symbols (`ResetHandler`,
//! `CURR_TCB`, ...) whose names don't say which crate they came from, but the map
//! records the input file each section came from.

use std::collections::BTreeMap;

use crate::elf;

const KERNEL_OUT: &[&str] = &[".vector_table", ".text", ".rodata", ".data", ".bss"];
const USER_OUT: &[&str] = &[".user_text", ".user_rodata", ".user_data", ".user_bss"];

/// One lld map row:  `VMA LMA Size Align <indent>Text`. Indent 0 is an output
/// section, 8 an input section, 16 a symbol.
struct Row<'a> {
    vma: u64,
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
    let (vma, _lma, size) = (hex(fields[0])?, hex(fields[1])?, hex(fields[2])?);
    if !fields[3].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(Row {
        vma,
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
    /// Output sections as listed in the map: name -> (address, size).
    pub out_sections: BTreeMap<String, (u64, u64)>,
}

/// Generic instances defined in a kernel-crate object and referenced from
/// user code (see the module docs). Uses the `Cross Reference Table` lld adds
/// to the map with `--cref`; a map without one is an error, since the check
/// would silently pass.
pub fn shared_generics(map: &str) -> Result<Vec<String>, String> {
    let Some(start) = map.find("Cross Reference Table") else {
        return Err(
            "the linker map has no cross-reference table: link with --cref (see app/build.rs)"
                .into(),
        );
    };
    let mut found = Vec::new();
    // Rows: `<symbol> <defining file>`, then `<indent> <referencing file>`.
    // lld demangles the names, which may contain spaces; paths do not.
    let mut current: Option<(&str, bool)> = None; // (symbol, defined in kernel)
    let flag = |sym: &str, file: &str, found: &mut Vec<String>| {
        let name = format!("{:#}", rustc_demangle::demangle(sym));
        // The kernel's own items, and the allocator shims of its
        // #[global_allocator] (the kernel heap is privileged by design).
        let kernel_item = name.starts_with("kernel::")
            || name.starts_with("<kernel::")
            || name.starts_with("__rustc::");
        if !kernel_item && !found.iter().any(|f: &String| f.starts_with(&name)) {
            found.push(format!("{name} (used by {})", short(file)));
        }
    };
    for line in map[start..].lines().skip(3) {
        if line.trim().is_empty() {
            continue;
        }
        let (sym, file) = if line.starts_with(' ') {
            (None, line.trim())
        } else {
            let Some((sym, file)) = line.trim_end().rsplit_once(' ') else {
                continue;
            };
            (Some(sym.trim_end()), file)
        };
        match sym {
            Some(sym) => current = Some((sym, is_kernel_file(file))),
            None => {
                let Some((sym, true)) = current else { continue };
                // Only Rust paths (mangled, or demangled by lld):
                // #[no_mangle] kernel symbols (ResetHandler, CURR_TCB, ...) are
                // the kernel's own.
                let rust = sym.starts_with("_R") || sym.starts_with("_ZN") || sym.contains("::");
                if rust && !is_kernel_file(file) && !file.ends_with("symbols.o") {
                    flag(sym, file, &mut found);
                }
            }
        }
    }
    Ok(found)
}

/// `.../libfoo-<hash>.rlib(...)` -> `foo`; `.../foo.<hash>.rcgu.o` -> `foo`.
fn short(file: &str) -> &str {
    let file = file.rsplit('/').next().unwrap_or(file);
    let file = file.split(['(', '.']).next().unwrap_or(file);
    let file = file.strip_prefix("lib").unwrap_or(file);
    file.rsplit_once('-').map_or(file, |(name, _)| name)
}

pub fn check_map(map: &str) -> Report {
    let mut out: Option<&str> = None;
    let mut errors = Vec::new();
    let mut kernel_text_bytes = 0;
    let mut out_sections = BTreeMap::new();

    for line in map.lines() {
        let Some(row) = parse_row(line) else { continue };
        if row.indent == 0 {
            out = Some(row.text);
            out_sections.insert(row.text.to_string(), (row.vma, row.size));
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

    match shared_generics(map) {
        Ok(found) => errors.extend(found.into_iter().map(|g| {
            format!(
                "user code calls a generic instantiated in the kernel crate (in KFLASH, \
                 faults when a thread calls it): {g}"
            )
        })),
        Err(e) => errors.push(e),
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
        out_sections,
    }
}

/// Errors if the map's output sections do not match the ELF's allocated
/// sections (address and size), i.e. the map is not from the link that
/// produced this ELF.
pub fn check_map_matches_elf(report: &Report, elf_data: &[u8]) -> Result<(), String> {
    let sections = elf::alloc_sections(elf_data)?;
    let mut mismatches = Vec::new();
    for (name, &(addr, size)) in &sections {
        match report.out_sections.get(name) {
            Some(&m) if m == (addr, size) => {}
            Some(&(maddr, msize)) => mismatches.push(format!(
                "{name}: ELF {addr:#x}+{size:#x}, map {maddr:#x}+{msize:#x}"
            )),
            None => mismatches.push(format!("{name}: in the ELF but not in the map")),
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the linker map does not describe this ELF (stale map from an older link?):\n  {}",
            mismatches.join("\n  ")
        ))
    }
}
