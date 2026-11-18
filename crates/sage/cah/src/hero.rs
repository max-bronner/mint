use mint_bytes::{Cursor, Truncated};

use crate::{Crc32, Result};

pub const POWER_SLOTS: usize = 15;

const MAGIC: [u8; 4] = *b"ALAE";
const CONTAINER_V1: [u8; 4] = *b"1STR";
const CONTAINER_V2: [u8; 4] = *b"2STR";
const RECORD_VERSION: u8 = 8;
const LONG_STRING: u8 = 0xFF;
const MIN_BLING_BYTES: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum CahError {
    #[error(transparent)]
    Truncated(#[from] Truncated),
    #[error("not a hero file: bad container magic")]
    BadMagic,
    #[error("unsupported container version {0}")]
    ContainerVersion(u32),
    #[error("tagged container is not supported")]
    Tagged,
    #[error("unsupported record version {0}")]
    RecordVersion(u8),
    #[error("empty bling group name at offset {offset}")]
    EmptyBlingGroup { offset: usize },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PowerEntry {
    pub button: Vec<u8>,
    pub exp_level: u32,
    pub button_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroRecord {
    pub version: u8,
    pub object_id: u32,
    pub name: Vec<u16>,
    pub class_index: u32,
    pub subclass_index: u32,
    pub dummy: [u32; 2],
    pub colors: [u32; 3],
    pub powers: [PowerEntry; POWER_SLOTS],
    pub blings: Vec<(Vec<u8>, u32)>,
    pub id: Option<Vec<u8>>,
    pub is_system: Option<bool>,
    pub stored_crc: Option<u32>,
}

impl HeroRecord {
    #[must_use]
    pub fn name_string(&self) -> String {
        String::from_utf16_lossy(up_to_nul(&self.name))
    }

    #[must_use]
    pub fn id_str(&self) -> Option<String> {
        self.id
            .as_ref()
            .map(|id| String::from_utf8_lossy(id).into_owned())
    }
}

impl Default for HeroRecord {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            object_id: 0,
            name: Vec::new(),
            class_index: 0,
            subclass_index: 0,
            dummy: [0; 2],
            colors: [0xFFFF_FFFF, 0xFF70_7070, 0xFFFF_FFFF],
            powers: Default::default(),
            blings: Vec::new(),
            id: Some(Vec::new()),
            is_system: Some(false),
            stored_crc: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHero {
    pub record: HeroRecord,
    pub crc_ok: bool,
    pub trailing: usize,
}

fn read_len(cur: &mut Cursor<'_>) -> Result<usize> {
    let short = cur.u8()?;
    let n = if short == LONG_STRING {
        cur.u32()?
    } else {
        u32::from(short)
    };
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

fn read_ascii(cur: &mut Cursor<'_>) -> Result<Vec<u8>> {
    let n = read_len(cur)?;
    Ok(cur.take(n)?.to_vec())
}

fn read_wide(cur: &mut Cursor<'_>) -> Result<Vec<u16>> {
    let n = read_len(cur)?;
    let bytes = cur.take(n.saturating_mul(2))?;
    Ok(bytes
        .chunks_exact(2)
        .map(|c| match *c {
            [lo, hi] => u16::from_le_bytes([lo, hi]),
            _ => 0,
        })
        .collect())
}

fn write_len(out: &mut Vec<u8>, n: usize) {
    match u8::try_from(n) {
        Ok(short) if short != LONG_STRING => out.push(short),
        _ => {
            out.push(LONG_STRING);
            out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
        }
    }
}

fn write_ascii(out: &mut Vec<u8>, s: &[u8]) {
    write_len(out, s.len());
    out.extend_from_slice(s);
}

fn up_to_nul<T: Copy + Default + PartialEq>(s: &[T]) -> &[T] {
    s.split(|&u| u == T::default()).next().unwrap_or(&[])
}

fn count_bytes(n: usize) -> [u8; 4] {
    u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes()
}

#[must_use]
pub fn compute_crc(record: &HeroRecord) -> u32 {
    let mut crc = Crc32::new();
    crc.update(&record.object_id.to_le_bytes());
    crc.update(record.name_string().as_bytes());
    crc.update(&record.class_index.to_le_bytes());
    crc.update(&record.subclass_index.to_le_bytes());
    for word in record.dummy.iter().chain(&record.colors) {
        crc.update(&word.to_le_bytes());
    }
    for power in &record.powers {
        crc.update(up_to_nul(&power.button));
        crc.update(&power.exp_level.to_le_bytes());
        crc.update(&power.button_index.to_le_bytes());
    }
    crc.update(&count_bytes(record.blings.len()));
    for (group, index) in &record.blings {
        crc.update(up_to_nul(group));
        crc.update(&index.to_le_bytes());
    }
    crc.update(&[u8::from(record.is_system.unwrap_or(false))]);
    crc.finish()
}

pub fn parse(bytes: &[u8]) -> Result<ParsedHero> {
    let mut cur = Cursor::new(bytes);
    if cur.array::<4>()? != MAGIC {
        return Err(CahError::BadMagic);
    }
    let container_version = match cur.array::<4>()? {
        CONTAINER_V1 => 0,
        CONTAINER_V2 => cur.u32()?,
        _ => return Err(CahError::BadMagic),
    };
    if container_version > 1 {
        return Err(CahError::ContainerVersion(container_version));
    }
    if cur.u32()? != 0 {
        return Err(CahError::Tagged);
    }

    let version = cur.u8()?;
    if !(1..=RECORD_VERSION).contains(&version) {
        return Err(CahError::RecordVersion(version));
    }
    let object_id = cur.u32()?;
    let name = read_wide(&mut cur)?;
    let class_index = cur.u32()?;
    let subclass_index = cur.u32()?;
    let dummy = [cur.u32()?, cur.u32()?];
    let colors = [cur.u32()?, cur.u32()?, cur.u32()?];
    let mut powers: [PowerEntry; POWER_SLOTS] = Default::default();
    for power in &mut powers {
        power.button = read_ascii(&mut cur)?;
        power.exp_level = cur.u32()?;
        power.button_index = cur.u32()?;
    }
    let count = usize::try_from(cur.u32()?).unwrap_or(usize::MAX);
    if count > cur.remaining() / MIN_BLING_BYTES {
        return Err(Truncated {
            offset: cur.position(),
            needed: count.saturating_mul(MIN_BLING_BYTES),
            available: cur.remaining(),
        }
        .into());
    }
    let mut blings = Vec::new();
    for _ in 0..count {
        let offset = cur.position();
        let group = read_ascii(&mut cur)?;
        if group.is_empty() {
            return Err(CahError::EmptyBlingGroup { offset });
        }
        blings.push((group, cur.u32()?));
    }
    let id = if version > 6 {
        Some(read_ascii(&mut cur)?)
    } else {
        None
    };
    let is_system = if version > 7 {
        Some(cur.u8()? != 0)
    } else {
        None
    };
    let stored_crc = if version > 7 { Some(cur.u32()?) } else { None };

    let record = HeroRecord {
        version,
        object_id,
        name,
        class_index,
        subclass_index,
        dummy,
        colors,
        powers,
        blings,
        id,
        is_system,
        stored_crc,
    };
    let crc_ok = stored_crc.is_none_or(|stored| stored == compute_crc(&record));
    Ok(ParsedHero {
        record,
        crc_ok,
        trailing: cur.remaining(),
    })
}

#[must_use]
pub fn write(record: &HeroRecord) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&CONTAINER_V2);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(RECORD_VERSION);
    out.extend_from_slice(&record.object_id.to_le_bytes());
    write_len(&mut out, record.name.len());
    for unit in &record.name {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&record.class_index.to_le_bytes());
    out.extend_from_slice(&record.subclass_index.to_le_bytes());
    for word in record.dummy.iter().chain(&record.colors) {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for power in &record.powers {
        write_ascii(&mut out, &power.button);
        out.extend_from_slice(&power.exp_level.to_le_bytes());
        out.extend_from_slice(&power.button_index.to_le_bytes());
    }
    out.extend_from_slice(&count_bytes(record.blings.len()));
    for (group, index) in &record.blings {
        write_ascii(&mut out, group);
        out.extend_from_slice(&index.to_le_bytes());
    }
    write_ascii(&mut out, record.id.as_deref().unwrap_or(&[]));
    out.push(u8::from(record.is_system.unwrap_or(false)));
    out.extend_from_slice(&compute_crc(record).to_le_bytes());
    out
}

#[must_use]
pub fn file_name(id: &str) -> String {
    format!("MyHero_{id}.cah")
}

#[must_use]
pub fn id_from_guid(guid: [u8; 16]) -> String {
    let [a, b, c, d, e, f, g, h, i, j, k, l, _, _, _, _] = guid;
    let data1 = u32::from_le_bytes([a, b, c, d]);
    let data2 = u16::from_le_bytes([e, f]);
    let data3 = u16::from_le_bytes([g, h]);
    format!("{data1:X}{data2:X}{data3:X}{i:X}{j:X}{k:X}{l:X}")
}
