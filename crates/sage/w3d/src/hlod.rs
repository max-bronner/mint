use crate::chunk::Chunk;
use crate::material::need;
use crate::reader::Reader;
use crate::{ChunkType, Result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HLodHeader {
    pub version: u32,
    pub lod_count: u32,
    pub name: String,
    pub hierarchy_name: String,
}

impl HLodHeader {
    pub const SIZE: usize = 40;

    fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "HLOD_HEADER")?;
        let mut r = Reader::new(payload);
        Ok(Self {
            version: r.u32()?,
            lod_count: r.u32()?,
            name: r.fixed_name(16)?,
            hierarchy_name: r.fixed_name(16)?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HLodSubObject {
    pub bone_index: u32,
    pub name: String,
}

impl HLodSubObject {
    pub const SIZE: usize = 36;

    fn parse(payload: &[u8]) -> Result<Self> {
        need(payload, Self::SIZE, "HLOD_SUB_OBJECT")?;
        let mut r = Reader::new(payload);
        Ok(Self {
            bone_index: r.u32()?,
            name: r.fixed_name(32)?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HLodArray {
    pub model_count: u32,
    pub max_screen_size: f32,
    pub sub_objects: Vec<HLodSubObject>,
}

impl HLodArray {
    const HEADER_SIZE: usize = 8;

    fn parse(chunk: &Chunk<'_>) -> Result<Self> {
        let mut out = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER => {
                    need(
                        kid.payload(),
                        Self::HEADER_SIZE,
                        "HLOD_SUB_OBJECT_ARRAY_HEADER",
                    )?;
                    let mut r = Reader::new(kid.payload());
                    out.model_count = r.u32()?;
                    out.max_screen_size = r.f32()?;
                }
                ChunkType::HLOD_SUB_OBJECT => {
                    out.sub_objects.push(HLodSubObject::parse(kid.payload())?);
                }
                _ => {}
            }
        }
        Ok(out)
    }

    #[must_use]
    pub fn counts_agree(&self) -> bool {
        usize::try_from(self.model_count).is_ok_and(|n| n == self.sub_objects.len())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HLod {
    pub header: HLodHeader,
    pub lods: Vec<HLodArray>,
    pub aggregates: Option<HLodArray>,
    pub proxies: Option<HLodArray>,
}

impl HLod {
    pub fn parse(chunk: &Chunk<'_>) -> Result<Self> {
        let mut out = Self::default();
        for kid in chunk.children()? {
            let kid = kid?;
            match kid.kind() {
                ChunkType::HLOD_HEADER => out.header = HLodHeader::parse(kid.payload())?,
                ChunkType::HLOD_LOD_ARRAY => out.lods.push(HLodArray::parse(&kid)?),
                ChunkType::HLOD_AGGREGATE_ARRAY => out.aggregates = Some(HLodArray::parse(&kid)?),
                ChunkType::HLOD_PROXY_ARRAY => out.proxies = Some(HLodArray::parse(&kid)?),
                _ => {}
            }
        }
        if usize::try_from(out.header.lod_count).map_or(true, |n| out.lods.len() < n) {
            return Err(crate::Error::MissingLodArrays {
                stated: out.header.lod_count,
                found: out.lods.len(),
            });
        }
        Ok(out)
    }

    #[must_use]
    pub fn hierarchy_name(&self) -> Option<&str> {
        let n = self.header.hierarchy_name.trim();
        (!n.is_empty()).then_some(n)
    }

    #[must_use]
    pub fn finest_lod(&self) -> Option<&HLodArray> {
        let read = usize::try_from(self.header.lod_count).unwrap_or(usize::MAX);
        self.lods.get(..read.min(self.lods.len()))?.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{chunk, concat};
    use crate::Chunks;

    #[test]
    fn a_sub_object_name_field_is_32_bytes() {
        let mut buf = vec![0u8; HLodSubObject::SIZE];
        buf[0..4].copy_from_slice(&3u32.to_le_bytes());
        buf[4..17].copy_from_slice(b"AICAMP.AICAMP");
        let so = HLodSubObject::parse(&buf).expect("36 bytes");
        assert_eq!(so.bone_index, 3);
        assert_eq!(so.name, "AICAMP.AICAMP");
    }

    #[test]
    fn short_headers_are_rejected() {
        assert!(HLodHeader::parse(&[0u8; 39]).is_err());
        assert!(HLodSubObject::parse(&[0u8; 35]).is_err());
        assert!(HLodHeader::parse(&[]).is_err());
    }

    #[test]
    fn lod_arrays_keep_file_order_and_report_their_counts() {
        let mut array_header = vec![0u8; 8];
        array_header[0..4].copy_from_slice(&2u32.to_le_bytes());
        array_header[4..8].copy_from_slice(&5.0f32.to_le_bytes());

        let mut sub = vec![0u8; 36];
        sub[0..4].copy_from_slice(&7u32.to_le_bytes());
        sub[4..11].copy_from_slice(b"BOX.BOX");

        let mut hlod_header = vec![0u8; 40];
        hlod_header[4..8].copy_from_slice(&1u32.to_le_bytes());
        hlod_header[8..11].copy_from_slice(b"BOX");
        hlod_header[24..32].copy_from_slice(b"BOXSKEL\0");

        let file = chunk(
            ChunkType::HLOD,
            true,
            &concat(&[
                chunk(ChunkType::HLOD_HEADER, false, &hlod_header),
                chunk(
                    ChunkType::HLOD_LOD_ARRAY,
                    true,
                    &concat(&[
                        chunk(
                            ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER,
                            false,
                            &array_header,
                        ),
                        chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub),
                    ]),
                ),
            ]),
        );

        let top = Chunks::new(&file)
            .next()
            .and_then(Result::ok)
            .expect("one chunk");
        let hlod = HLod::parse(&top).expect("valid");
        assert_eq!(hlod.hierarchy_name(), Some("BOXSKEL"));
        let lod = hlod.finest_lod().expect("one LOD array");
        assert_eq!(lod.max_screen_size, 5.0);
        assert_eq!(lod.sub_objects.len(), 1);
        assert_eq!(lod.sub_objects.first().map(|s| s.bone_index), Some(7));
        assert!(
            !lod.counts_agree(),
            "the header claims two sub-objects and one followed"
        );
    }

    fn hlod_with(stated: u32, arrays: &[(&str, u32)]) -> Vec<u8> {
        let mut header = vec![0u8; 40];
        header[4..8].copy_from_slice(&stated.to_le_bytes());
        let mut kids = vec![chunk(ChunkType::HLOD_HEADER, false, &header)];
        for (name, bone) in arrays {
            let mut sub = vec![0u8; 36];
            sub[0..4].copy_from_slice(&bone.to_le_bytes());
            for (slot, byte) in sub[4..].iter_mut().zip(name.bytes()) {
                *slot = byte;
            }
            kids.push(chunk(
                ChunkType::HLOD_LOD_ARRAY,
                true,
                &chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub),
            ));
        }
        chunk(ChunkType::HLOD, true, &concat(&kids))
    }

    fn parse_hlod(file: &[u8]) -> Result<HLod> {
        let top = Chunks::new(file)
            .next()
            .and_then(Result::ok)
            .expect("one chunk");
        HLod::parse(&top)
    }

    #[test]
    fn only_the_stated_number_of_lod_arrays_is_read() {
        let file = hlod_with(1, &[("A.A", 1), ("B.B", 2)]);
        let hlod = parse_hlod(&file).expect("valid");
        let lod = hlod.finest_lod().expect("one array read");
        assert_eq!(lod.sub_objects.first().map(|s| s.bone_index), Some(1));
    }

    #[test]
    fn a_stated_lod_array_that_is_missing_is_an_error() {
        let file = hlod_with(2, &[("A.A", 1)]);
        assert!(matches!(
            parse_hlod(&file),
            Err(crate::Error::MissingLodArrays {
                stated: 2,
                found: 1
            })
        ));
    }

    #[test]
    fn an_empty_hierarchy_name_reads_as_none() {
        let h = HLod {
            header: HLodHeader {
                hierarchy_name: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(h.hierarchy_name(), None);
        assert_eq!(h.finest_lod(), None);
    }
}
