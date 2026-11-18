use super::pe::Image;
use std::collections::BTreeMap;

pub(crate) struct StringLit {
    pub(crate) rva: u32,
    pub(crate) text: String,
}

pub(crate) struct ImmStore {
    pub(crate) target_va: u32,
    pub(crate) imm: u32,
}

pub(crate) fn scan_strings(bytes: &[u8], base_rva: u32) -> Vec<StringLit> {
    let mut out = Vec::new();
    let mut run_start: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        let printable = (0x20..=0x7e).contains(&b);
        if printable {
            if run_start.is_none() {
                run_start = Some(i);
            }
            continue;
        }
        if let Some(s) = run_start.take() {
            if b == 0 && i.saturating_sub(s) >= 2 {
                if let Some(text) = bytes.get(s..i).and_then(|sl| std::str::from_utf8(sl).ok()) {
                    out.push(StringLit {
                        rva: base_rva.saturating_add(u32::try_from(s).unwrap_or(u32::MAX)),
                        text: text.to_owned(),
                    });
                }
            }
        }
    }
    out
}

pub(crate) fn scan_imm_stores(bytes: &[u8]) -> Vec<ImmStore> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 10 <= bytes.len() {
        let is_store = bytes.get(i) == Some(&0xC7) && bytes.get(i + 1) == Some(&0x05);
        if is_store {
            if let (Some(d), Some(v)) = (bytes.get(i + 2..i + 6), bytes.get(i + 6..i + 10)) {
                if let (Ok(d), Ok(v)) = (<[u8; 4]>::try_from(d), <[u8; 4]>::try_from(v)) {
                    out.push(ImmStore {
                        target_va: u32::from_le_bytes(d),
                        imm: u32::from_le_bytes(v),
                    });
                }
            }
            i += 10;
        } else {
            i += 1;
        }
    }
    out
}

pub(crate) fn scan_data_dwords(bytes: &[u8], base_rva: u32, image_base: u32) -> Vec<ImmStore> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if let Some(v) = bytes
            .get(i..i + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
        {
            let target_rva = base_rva.saturating_add(u32::try_from(i).unwrap_or(u32::MAX));
            out.push(ImmStore {
                target_va: image_base.saturating_add(target_rva),
                imm: u32::from_le_bytes(v),
            });
        }
        i += 4;
    }
    out
}

pub(crate) fn looks_like_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    text.len() >= 2
        && text.len() <= 64
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) struct NameProcPair {
    pub(crate) target_rva: u32,
    pub(crate) name: String,
    pub(crate) proc_rva: u32,
}

pub(crate) fn pair_name_and_proc(
    img: &Image,
    stores: &[ImmStore],
    strings_by_rva: &BTreeMap<u32, String>,
    text_range: (u32, u32),
) -> Vec<NameProcPair> {
    let mut by_target: BTreeMap<u32, &ImmStore> = BTreeMap::new();
    for s in stores {
        by_target.insert(s.target_va, s);
    }

    let mut out = Vec::new();
    for s in stores {
        let Some(name_rva) = img.va_to_rva(s.imm) else {
            continue;
        };
        let Some(name) = strings_by_rva.get(&name_rva) else {
            continue;
        };
        if !looks_like_identifier(name) {
            continue;
        }
        let Some(next_va) = s.target_va.checked_add(4) else {
            continue;
        };
        let Some(next) = by_target.get(&next_va) else {
            continue;
        };
        let Some(proc_rva) = img.va_to_rva(next.imm) else {
            continue;
        };
        if proc_rva < text_range.0 || proc_rva >= text_range.1 {
            continue;
        }
        let Some(target_rva) = img.va_to_rva(s.target_va) else {
            continue;
        };
        out.push(NameProcPair {
            target_rva,
            name: name.clone(),
            proc_rva,
        });
    }
    out
}
