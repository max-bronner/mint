use crate::container::{NameTable, RawChunk};
use crate::dictionary::{read_dictionary, Dictionary, Value};
use crate::reader::Reader;
use crate::{Error, Result};

pub const SIDES_CHUNK: &str = "SidesList";

const SIDES_LEADING_BOOL_VERSION: u16 = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub properties: Dictionary,
    pub build_list: Vec<BuildListEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildListEntry {
    pub building_name: String,
    pub template_name: String,
    pub position: [f32; 3],
    pub angle: f32,
    pub initially_built: bool,
    pub rebuilds: u32,
    pub script: String,
    pub health: u32,
    pub whiner: bool,
    pub unsellable: bool,
    pub repairable: bool,
}

const SIDES_BUILD_SCRIPT_VERSION: u16 = 3;
const MIN_BUILD_ENTRY_BYTES: usize = 34;

fn read_build_list(r: &mut Reader<'_>, version: u16) -> Result<Vec<BuildListEntry>> {
    let declared = r.u32()?;
    let count = usize::try_from(declared).unwrap_or(usize::MAX);
    if count.saturating_mul(MIN_BUILD_ENTRY_BYTES) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: SIDES_CHUNK,
            offset: r.position(),
            count: declared,
            item_size: MIN_BUILD_ENTRY_BYTES,
            available: r.remaining(),
        });
    }
    if count > 0 && version < SIDES_BUILD_SCRIPT_VERSION {
        return Err(Error::UnknownScriptVersion {
            chunk: SIDES_CHUNK,
            version,
        });
    }
    let mut entries = Vec::new();
    for _ in 0..count {
        entries.push(BuildListEntry {
            building_name: r.ascii_string()?,
            template_name: r.ascii_string()?,
            position: [r.f32()?, r.f32()?, r.f32()?],
            angle: r.f32()?,
            initially_built: r.u8()? != 0,
            rebuilds: r.u32()?,
            script: r.ascii_string()?,
            health: r.u32()?,
            whiner: r.u8()? != 0,
            unsellable: r.u8()? != 0,
            repairable: r.u8()? != 0,
        });
    }
    Ok(entries)
}

impl Side {
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.properties.get(key)? {
            Value::Str(text) | Value::WideStr(text) => Some(text.as_str()),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.text("playerName")
    }

    pub fn names_under<'a>(&'a self, key: &str) -> impl Iterator<Item = &'a str> {
        self.text(key).unwrap_or_default().split_whitespace()
    }
}

pub(crate) fn read_sides(body: &[u8], chunk: &RawChunk, names: &NameTable) -> Result<Vec<Side>> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    if chunk.version >= SIDES_LEADING_BOOL_VERSION {
        let _leading_bool = r.u8()?;
    }
    let count = r.u32()?;
    let mut sides = Vec::new();
    for _ in 0..count {
        let properties = read_dictionary(&mut r, names)?;
        let build_list = read_build_list(&mut r, chunk.version)?;
        sides.push(Side {
            properties,
            build_list,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: SIDES_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(sides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::MapBuilder;

    fn two_sides() -> (Vec<u8>, NameTable) {
        let mut b = MapBuilder::new();
        let neutral = vec![b.dict_str("playerName", "")];
        let other = vec![
            b.dict_str("playerName", "Gaia"),
            b.dict_bool("playerIsHuman", false),
            b.dict_str("playerAllies", "Hermits"),
            b.dict_str("playerEnemies", "Raiders  Bandits"),
        ];
        let mut payload = vec![1u8];
        payload.extend_from_slice(&2u32.to_le_bytes());
        for entries in [neutral, other] {
            payload.extend_from_slice(&b.dict(entries));
            payload.extend_from_slice(&0u32.to_le_bytes());
        }
        (payload, b.name_table())
    }

    fn read(payload: &[u8], names: &NameTable) -> Result<Vec<Side>> {
        read_version(payload, names, 6)
    }

    fn read_version(payload: &[u8], names: &NameTable, version: u16) -> Result<Vec<Side>> {
        let chunk = RawChunk {
            name: SIDES_CHUNK.into(),
            version,
            body: 0,
            len: payload.len(),
        };
        read_sides(payload, &chunk, names)
    }

    #[test]
    fn each_side_reads_its_name_and_its_relation_lists() {
        let (payload, names) = two_sides();
        let sides = read(&payload, &names).unwrap();
        assert_eq!(sides.len(), 2);
        assert_eq!(sides[0].name(), Some(""));
        assert_eq!(sides[1].name(), Some("Gaia"));
        assert_eq!(
            sides[1].names_under("playerEnemies").collect::<Vec<_>>(),
            ["Raiders", "Bandits"]
        );
        assert_eq!(
            sides[1].names_under("playerAllies").collect::<Vec<_>>(),
            ["Hermits"]
        );
    }

    #[test]
    fn the_leading_bool_exists_only_from_version_6() {
        let (payload, names) = two_sides();
        let sides = read_version(&payload[1..], &names, 5).unwrap();
        assert_eq!(sides.len(), 2);
        assert_eq!(sides[1].name(), Some("Gaia"));
    }

    #[test]
    fn an_embedded_build_list_is_read_entry_by_entry() {
        let mut b = MapBuilder::new();
        let entries = vec![b.dict_str("playerName", "Gaia")];
        let dict = b.dict(entries);
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&dict);
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&MapBuilder::ascii("Hut1"));
        payload.extend_from_slice(&MapBuilder::ascii("HutTemplate"));
        for v in [1.0f32, 2.0, 3.0, 0.5] {
            payload.extend_from_slice(&v.to_le_bytes());
        }
        payload.push(1);
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&MapBuilder::ascii("OnBuilt"));
        payload.extend_from_slice(&100u32.to_le_bytes());
        payload.extend_from_slice(&[0, 1, 1]);
        let sides = read_version(&payload, &b.name_table(), 5).unwrap();
        let entry = &sides[0].build_list[0];
        assert_eq!(entry.template_name, "HutTemplate");
        assert_eq!(entry.position, [1.0, 2.0, 3.0]);
        assert_eq!((entry.rebuilds, entry.health), (2, 100));
        assert_eq!(
            (
                entry.initially_built,
                entry.whiner,
                entry.unsellable,
                entry.repairable
            ),
            (true, false, true, true)
        );
    }

    #[test]
    fn truncation_errors_rather_than_panicking() {
        let (payload, names) = two_sides();
        for len in 0..payload.len() {
            assert!(read(&payload[..len], &names).is_err(), "prefix {len}");
        }
    }

    #[test]
    fn corruption_never_panics() {
        let (payload, names) = two_sides();
        for at in 0..payload.len() {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = payload.clone();
                bad[at] ^= flip;
                let _ = read(&bad, &names);
            }
        }
    }
}
