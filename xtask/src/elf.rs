//! Just enough ELF32 (little-endian, ARM) parsing to list a firmware image's
//! allocated sections. Malformed input is an error, never a panic or a
//! silently empty result.

use std::collections::BTreeMap;

const SHF_ALLOC: u32 = 0x2;
const EM_ARM: u16 = 40;
/// Size of an ELF32 section header.
const SHDR_SIZE: usize = 40;
const SHN_UNDEF: usize = 0;
const SHN_LORESERVE: usize = 0xff00;

fn read<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], String> {
    offset
        .checked_add(N)
        .and_then(|end| data.get(offset..end))
        .map(|b| b.try_into().expect("slice has length N"))
        .ok_or_else(|| format!("truncated ELF (reading {N} bytes at {offset:#x})"))
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    read::<2>(data, offset).map(u16::from_le_bytes)
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    read::<4>(data, offset).map(u32::from_le_bytes)
}

/// Allocated, non-empty sections of an ELF32 ARM image: name -> (address, size).
pub fn alloc_sections(data: &[u8]) -> Result<BTreeMap<String, (u64, u64)>, String> {
    if data.get(..4) != Some(b"\x7fELF") {
        return Err("not an ELF file".into());
    }
    let (class, endian) = (data.get(4).copied(), data.get(5).copied());
    if class != Some(1) || endian != Some(1) {
        return Err(format!(
            "expected a 32-bit little-endian ELF, found class {class:?}, data encoding {endian:?}"
        ));
    }
    let machine = u16_at(data, 0x12)?;
    if machine != EM_ARM {
        return Err(format!(
            "expected an ARM ELF (e_machine 40), found {machine}"
        ));
    }

    let shoff = u32_at(data, 0x20)? as usize;
    let shentsize = usize::from(u16_at(data, 0x2E)?);
    let shnum = usize::from(u16_at(data, 0x30)?);
    let shstrndx = usize::from(u16_at(data, 0x32)?);
    if shoff == 0 || shnum == 0 {
        return Err("ELF has no section headers".into());
    }
    if shentsize != SHDR_SIZE {
        return Err(format!("unexpected section header size {shentsize}"));
    }
    if shstrndx == SHN_UNDEF || shstrndx >= SHN_LORESERVE || shstrndx >= shnum {
        return Err(format!("unsupported section name table index {shstrndx}"));
    }
    let header = |i: usize| -> Result<usize, String> {
        i.checked_mul(SHDR_SIZE)
            .and_then(|o| o.checked_add(shoff))
            .ok_or_else(|| "section header offset overflows".to_string())
    };

    let strtab = header(shstrndx)?;
    let (str_off, str_size) = (
        u32_at(data, strtab + 0x10)? as usize,
        u32_at(data, strtab + 0x14)? as usize,
    );
    let strtab_bytes = str_off
        .checked_add(str_size)
        .and_then(|end| data.get(str_off..end))
        .ok_or("section name table lies outside the file")?;
    let name_at = |off: usize| -> Result<String, String> {
        let bytes = strtab_bytes
            .get(off..)
            .ok_or_else(|| format!("section name offset {off:#x} outside the name table"))?;
        let end = bytes
            .iter()
            .position(|&b| b == 0)
            .ok_or("unterminated section name")?;
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    };

    let mut sections = BTreeMap::new();
    for i in 0..shnum {
        let h = header(i)?;
        let flags = u32_at(data, h + 0x08)?;
        let addr = u64::from(u32_at(data, h + 0x0C)?);
        let size = u64::from(u32_at(data, h + 0x14)?);
        if flags & SHF_ALLOC != 0 && size > 0 {
            sections.insert(name_at(u32_at(data, h)? as usize)?, (addr, size));
        }
    }
    if sections.is_empty() {
        return Err("ELF has no allocated sections".into());
    }
    Ok(sections)
}
