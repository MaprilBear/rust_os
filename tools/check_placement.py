#!/usr/bin/env python3
"""Post-link guard for the kernel/user split. Reads the linker map (lld `-Map=`).

    check_placement.py target/thumbv7em-none-eabihf/debug/rust_os.map

Fails (exit 1) if
  * anything from the kernel crate landed in a user output section, or
  * anything other than the kernel crate / .ksem objects / the vector table
    landed in a kernel output section, or
  * the kernel crate contributed nothing to KFLASH. That is what happens when LTO
    merges every crate into one object: the link "succeeds" but the whole kernel
    is in user memory.

Why a map file: the kernel crate also has #[no_mangle] symbols (ResetHandler,
CURR_TCB, ...) whose names don't say which crate they came from, but the map
records the input file each section came from.
"""
import re
import sys

KERNEL_FILE = re.compile(r"libkernel-[0-9a-f]+\.rlib\(")  # same match as link.ld
KERNEL_OUT = {".vector_table", ".text", ".rodata", ".data", ".bss"}
USER_OUT = {".user_text", ".user_rodata", ".user_data", ".user_bss"}

# lld map row:  VMA LMA Size Align <indent>Text   (indent 0 = output section, 8 = input section)
ROW = re.compile(r"^\s*([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+(\d+)( +)(\S.*)$")
INPUT = re.compile(r"^(?P<file>.+?):\((?P<sec>[^()]+)\)$")


def main(path):
    out = None
    errors = []
    kernel_text_bytes = 0
    with open(path) as f:
        for line in f:
            m = ROW.match(line)
            if not m:
                continue
            size, indent, text = int(m.group(3), 16), len(m.group(5)) - 1, m.group(6)
            if indent == 0:
                out = text
                continue
            im = INPUT.match(text) if indent == 8 else None
            if not im or size == 0 or out is None:
                continue
            fname, sec = im.group("file"), im.group("sec")
            if fname == "<internal>":
                # Linker-synthesised section, e.g. a merged string/constant pool
                # (.rodata.str1.1, .rodata.cst8, ...). lld builds those per output
                # section *after* input sections were assigned by pattern, so the
                # pool in .rodata only holds kernel-crate constants. No source file.
                continue
            is_kernel = bool(KERNEL_FILE.search(fname))  # fname looks like dir/libkernel-<hash>.rlib(member.o)
            where = f"{fname}:({sec}) -> {out}"
            if out in USER_OUT and is_kernel:
                errors.append(f"kernel crate in USER memory : {where}")
            elif out in KERNEL_OUT and not is_kernel:
                if out == ".vector_table" or sec.startswith(".ksem"):
                    continue  # explicit by-section-name opt-ins
                errors.append(f"non-kernel code/data in KERNEL memory: {where}")
            if out == ".text" and is_kernel:
                kernel_text_bytes += size

    if kernel_text_bytes == 0:
        errors.append(
            "kernel crate contributed 0 bytes to .text (KFLASH). Crate boundaries were lost "
            "(LTO?) or the rlib name no longer matches *libkernel-*.rlib:* in link.ld"
        )
    for e in errors:
        print("PLACEMENT ERROR:", e)
    if errors:
        return 1
    print(f"placement OK ({kernel_text_bytes} bytes of kernel text in KFLASH)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1]))
