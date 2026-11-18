pub(crate) struct Section {
    pub(crate) name: String,
    pub(crate) virtual_address: u32,
    pub(crate) virtual_size: u32,
    pub(crate) raw_ptr: u32,
    pub(crate) raw_size: u32,
}

pub(crate) struct Image {
    pub(crate) data: Vec<u8>,
    pub(crate) image_base: u32,
    pub(crate) sections: Vec<Section>,
}

fn u16_at(data: &[u8], off: usize) -> Option<u16> {
    data.get(off..off.checked_add(2)?)
        .and_then(|s| <[u8; 2]>::try_from(s).ok())
        .map(u16::from_le_bytes)
}

fn u32_at(data: &[u8], off: usize) -> Option<u32> {
    data.get(off..off.checked_add(4)?)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map(u32::from_le_bytes)
}

impl Image {
    pub(crate) fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if data.get(0..2) != Some(b"MZ") {
            return Err(format!("{}: not an MZ image", path.display()));
        }
        let e_lfanew = u32_at(&data, 0x3c).ok_or("truncated DOS header")? as usize;
        if data.get(e_lfanew..e_lfanew.checked_add(4).ok_or("overflow")?) != Some(b"PE\0\0") {
            return Err("no PE signature at e_lfanew".to_owned());
        }
        let file_header = e_lfanew + 4;
        let n_sections = u16_at(&data, file_header + 2).ok_or("truncated file header")? as usize;
        let opt_hdr_size = u16_at(&data, file_header + 16).ok_or("truncated file header")?;
        let opt_off = file_header + 20;
        let magic = u16_at(&data, opt_off).ok_or("truncated optional header")?;
        if magic != 0x10b {
            return Err(format!(
                "not PE32 (magic {magic:#x}); PE32+ is not supported"
            ));
        }
        let image_base = u32_at(&data, opt_off + 28).ok_or("truncated optional header")?;

        let section_table = opt_off + opt_hdr_size as usize;
        let mut sections = Vec::with_capacity(n_sections.min(96));
        for i in 0..n_sections {
            let off = section_table + i * 40;
            let raw_name = data.get(off..off + 8).ok_or("truncated section table")?;
            let name = String::from_utf8_lossy(raw_name)
                .trim_end_matches('\0')
                .to_owned();
            let virtual_size = u32_at(&data, off + 8).ok_or("truncated section header")?;
            let virtual_address = u32_at(&data, off + 12).ok_or("truncated section header")?;
            let raw_size = u32_at(&data, off + 16).ok_or("truncated section header")?;
            let raw_ptr = u32_at(&data, off + 20).ok_or("truncated section header")?;
            sections.push(Section {
                name,
                virtual_address,
                virtual_size,
                raw_ptr,
                raw_size,
            });
        }

        Ok(Self {
            data,
            image_base,
            sections,
        })
    }

    pub(crate) fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub(crate) fn raw_bytes(&self, section: &Section) -> &[u8] {
        let start = section.raw_ptr as usize;
        let len = (section.raw_size as usize).min(self.data.len().saturating_sub(start));
        self.data.get(start..start + len).unwrap_or(&[])
    }

    pub(crate) fn rva_to_file_offset(&self, rva: u32) -> Option<usize> {
        for s in &self.sections {
            let end = s.virtual_address.checked_add(s.virtual_size)?;
            if rva >= s.virtual_address && rva < end {
                let delta = rva - s.virtual_address;
                if delta >= s.raw_size {
                    return None;
                }
                return usize::try_from(u64::from(s.raw_ptr) + u64::from(delta)).ok();
            }
        }
        None
    }

    pub(crate) fn va_to_rva(&self, va: u32) -> Option<u32> {
        va.checked_sub(self.image_base)
    }

    pub(crate) fn read_cstr(&self, rva: u32, max_len: usize) -> Option<String> {
        let off = self.rva_to_file_offset(rva)?;
        let bytes = self.data.get(off..)?;
        let mut out = String::new();
        for &b in bytes.iter().take(max_len) {
            if b == 0 {
                return if out.len() >= 2 { Some(out) } else { None };
            }
            if !(0x20..=0x7e).contains(&b) {
                return None;
            }
            out.push(b as char);
        }
        None
    }

    pub(crate) fn read_u32(&self, rva: u32) -> Option<u32> {
        let off = self.rva_to_file_offset(rva)?;
        u32_at(&self.data, off)
    }
}
