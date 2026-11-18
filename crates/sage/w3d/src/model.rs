use std::collections::BTreeMap;

use crate::chunk::{Chunk, Chunks};
use crate::material::need;
use crate::reader::Reader;
use crate::{Channel, ChunkType, HLod, Hierarchy, Mesh, Result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Animation {
    pub name: String,
    pub hierarchy_name: String,
    pub num_frames: u32,
    pub frame_rate: u32,
    pub compressed: bool,
    pub flavor: Option<u16>,
    pub channels: Vec<Channel>,
    pub channel_chunks: u32,
}

impl Animation {
    const HEADER_SIZE: usize = 44;
    const COMPRESSED_HEADER_SIZE: usize = 44;

    fn parse_plain(payload: &[u8]) -> Result<Self> {
        need(payload, Self::HEADER_SIZE, "ANIMATION_HEADER")?;
        let mut r = Reader::new(payload);
        r.skip(4)?;
        Ok(Self {
            name: r.fixed_name(16)?,
            hierarchy_name: r.fixed_name(16)?,
            num_frames: r.u32()?,
            frame_rate: r.u32()?,
            compressed: false,
            flavor: None,
            channels: Vec::new(),
            channel_chunks: 0,
        })
    }

    fn parse_compressed(payload: &[u8]) -> Result<Self> {
        need(
            payload,
            Self::COMPRESSED_HEADER_SIZE,
            "COMPRESSED_ANIMATION_HEADER",
        )?;
        let mut r = Reader::new(payload);
        r.skip(4)?;
        let name = r.fixed_name(16)?;
        let hierarchy_name = r.fixed_name(16)?;
        let num_frames = r.u32()?;
        let frame_rate = u32::from(r.u16()?);
        Ok(Self {
            name,
            hierarchy_name,
            num_frames,
            frame_rate,
            compressed: true,
            flavor: Some(r.u16()?),
            channels: Vec::new(),
            channel_chunks: 0,
        })
    }

    #[must_use]
    pub fn out_of_range_pivots(&self, pivot_count: usize) -> Vec<usize> {
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, c)| usize::try_from(c.pivot).is_ok_and(|p| p >= pivot_count))
            .map(|(i, _)| i)
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Model<'a> {
    pub meshes: Vec<Mesh<'a>>,
    pub hierarchy: Option<Hierarchy>,
    pub hlods: Vec<HLod>,
    pub animations: Vec<Animation>,
    pub chunk_counts: BTreeMap<ChunkType, u32>,
}

impl<'a> Model<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let mut model = Self::default();
        for chunk in Chunks::new(data) {
            let chunk = chunk?;
            model.count(&chunk)?;
            match chunk.kind() {
                ChunkType::MESH => model.meshes.push(Mesh::parse(&chunk)?),
                ChunkType::HIERARCHY => model.hierarchy = Some(Hierarchy::parse(&chunk)?),
                ChunkType::HLOD => model.hlods.push(HLod::parse(&chunk)?),
                ChunkType::ANIMATION => model.parse_animation(&chunk, false)?,
                ChunkType::COMPRESSED_ANIMATION => model.parse_animation(&chunk, true)?,
                _ => {}
            }
        }
        Ok(model)
    }

    fn parse_animation(&mut self, chunk: &Chunk<'a>, compressed: bool) -> Result<()> {
        let mut found: Option<Animation> = None;
        let mut channel_chunks: u32 = 0;
        let mut channels: Vec<Channel> = Vec::new();
        for kid in chunk.children()? {
            let kid = kid?;
            match (kid.kind(), compressed) {
                (ChunkType::ANIMATION_HEADER, false) => {
                    found = Some(Animation::parse_plain(kid.payload())?);
                }
                (ChunkType::COMPRESSED_ANIMATION_HEADER, true) => {
                    found = Some(Animation::parse_compressed(kid.payload())?);
                }
                (
                    ChunkType::ANIMATION_CHANNEL
                    | ChunkType::BIT_CHANNEL
                    | ChunkType::COMPRESSED_ANIMATION_CHANNEL
                    | ChunkType::COMPRESSED_BIT_CHANNEL
                    | ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
                    _,
                ) => {
                    channel_chunks = channel_chunks.saturating_add(1);
                    let flavor = found.as_ref().and_then(|a| a.flavor);
                    if let Some(decoded) = Channel::parse(&kid, flavor)? {
                        channels.push(decoded);
                    }
                }
                _ => {}
            }
        }
        if let Some(mut anim) = found {
            anim.channel_chunks = channel_chunks;
            anim.channels = channels;
            self.animations.push(anim);
        }
        Ok(())
    }

    fn count(&mut self, chunk: &Chunk<'a>) -> Result<()> {
        let slot = self.chunk_counts.entry(chunk.kind()).or_insert(0);
        *slot = slot.saturating_add(1);
        for kid in chunk.children()? {
            self.count(&kid?)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(Mesh::vertex_count).sum()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(Mesh::triangle_count).sum()
    }

    #[must_use]
    pub fn mesh(&self, name: &str) -> Option<&Mesh<'a>> {
        self.meshes
            .iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn external_hierarchy(&self) -> Option<&str> {
        let wanted = self.hlods.first()?.hierarchy_name()?;
        match &self.hierarchy {
            Some(h) if h.header.name.eq_ignore_ascii_case(wanted) => None,
            _ => Some(wanted),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{animated_model, chunk, concat, tiny_model};

    #[test]
    fn parses_a_whole_file() {
        let bytes = tiny_model();
        let m = Model::parse(&bytes).expect("valid model");
        assert_eq!(m.meshes.len(), 1);
        assert_eq!(m.vertex_count(), 2);
        assert_eq!(m.triangle_count(), 1);

        let mesh = m.mesh("box").expect("found by case-insensitive name");
        assert!(mesh.counts_agree());
        assert_eq!(mesh.texture_name(), Some("box.tga"));
        assert_eq!(mesh.texcoords().len(), 2);

        assert_eq!(m.external_hierarchy(), Some("BOXSKEL"));
        assert_eq!(m.chunk_counts.get(&ChunkType::TEXTURE_NAME), Some(&1));
        assert_eq!(m.chunk_counts.get(&ChunkType::MESH), Some(&1));
        assert_eq!(m.chunk_counts.get(&ChunkType::HLOD_SUB_OBJECT), Some(&1));
    }

    #[test]
    fn animation_headers_and_channels_are_decoded() {
        let mut header = vec![0u8; 44];
        header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
        header[4..12].copy_from_slice(b"RUNCYCLE");
        header[20..27].copy_from_slice(b"BOXSKEL");
        header[36..40].copy_from_slice(&30u32.to_le_bytes());
        header[40..44].copy_from_slice(&30u32.to_le_bytes());

        let mut anim_channel = vec![0u8; 12];
        anim_channel.extend_from_slice(&0.0f32.to_le_bytes());
        anim_channel[8..10].copy_from_slice(&3u16.to_le_bytes());

        let mut bit_channel = vec![0u8; 10];
        bit_channel[6..8].copy_from_slice(&4u16.to_le_bytes());
        bit_channel[8] = 1;

        let file = chunk(
            ChunkType::ANIMATION,
            true,
            &concat(&[
                chunk(ChunkType::ANIMATION_HEADER, false, &header),
                chunk(ChunkType::ANIMATION_CHANNEL, false, &anim_channel),
                chunk(ChunkType::BIT_CHANNEL, false, &bit_channel),
            ]),
        );

        let m = Model::parse(&file).expect("valid");
        let anim = m.animations.first().expect("one animation");
        assert_eq!(anim.name, "RUNCYCLE");
        assert_eq!(anim.hierarchy_name, "BOXSKEL");
        assert_eq!(anim.num_frames, 30);
        assert_eq!(anim.frame_rate, 30);
        assert!(!anim.compressed);
        assert_eq!(anim.flavor, None);
        assert_eq!(anim.channel_chunks, 2);
        assert_eq!(
            anim.channels.len(),
            2,
            "both channels decoded, not just counted"
        );
        assert_eq!(anim.channels[0].pivot, 3);
        assert_eq!(anim.channels[0].kind, crate::ChannelKind::TranslationX);
        assert_eq!(anim.channels[1].pivot, 4);
        assert_eq!(anim.channels[1].kind, crate::ChannelKind::Visibility);
        assert!(anim.out_of_range_pivots(10).is_empty());
        assert_eq!(
            anim.out_of_range_pivots(4),
            vec![1],
            "pivot 4 needs 5 pivots"
        );
    }

    #[test]
    fn a_compressed_animation_decodes_its_motion_channel() {
        let mut header = vec![0u8; 44];
        header[4..8].copy_from_slice(b"WALK");
        header[36..40].copy_from_slice(&60u32.to_le_bytes());
        header[40..42].copy_from_slice(&30u16.to_le_bytes());
        header[42..44].copy_from_slice(&1u16.to_le_bytes());

        let mut motion = vec![0u8; 8];
        motion[6..8].copy_from_slice(&9u16.to_le_bytes());

        let file = chunk(
            ChunkType::COMPRESSED_ANIMATION,
            true,
            &concat(&[
                chunk(ChunkType::COMPRESSED_ANIMATION_HEADER, false, &header),
                chunk(
                    ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
                    false,
                    &motion,
                ),
            ]),
        );

        let m = Model::parse(&file).expect("valid");
        let anim = m.animations.first().expect("one animation");
        assert_eq!(anim.name, "WALK");
        assert_eq!(anim.num_frames, 60);
        assert_eq!(anim.frame_rate, 30);
        assert!(anim.compressed);
        assert_eq!(anim.flavor, Some(1));
        assert_eq!(anim.channel_chunks, 1);
        let decoded = anim.channels.first().expect("one channel");
        assert_eq!(decoded.pivot, 9);
        assert_eq!(
            decoded.data,
            crate::ChannelData::TimeCoded(Vec::new()),
            "delta_type routes this channel, not the header's dead flavour field"
        );
    }

    #[test]
    fn truncation_at_every_length_errors_but_never_panics() {
        let bytes = tiny_model();
        for cut in 0..bytes.len() {
            let Some(prefix) = bytes.get(..cut) else {
                continue;
            };
            let _ = Model::parse(prefix);
        }
    }

    #[test]
    fn every_single_byte_corruption_is_handled() {
        let bytes = tiny_model();
        for i in 0..bytes.len() {
            for mask in [0x01u8, 0x80, 0xFF] {
                let mut corrupt = bytes.clone();
                if let Some(b) = corrupt.get_mut(i) {
                    *b ^= mask;
                }
                let _ = Model::parse(&corrupt);
            }
        }
    }

    #[test]
    fn an_animated_model_decodes_every_channel_shape() {
        let bytes = animated_model();
        let m = Model::parse(&bytes).expect("valid");
        assert_eq!(m.animations.len(), 2);
        assert_eq!(m.animations[0].channels.len(), 2);
        assert_eq!(m.animations[1].channels.len(), 2);
    }

    #[test]
    fn truncation_of_an_animated_model_errors_but_never_panics() {
        let bytes = animated_model();
        for cut in 0..bytes.len() {
            let Some(prefix) = bytes.get(..cut) else {
                continue;
            };
            let _ = Model::parse(prefix);
        }
    }

    #[test]
    fn every_single_byte_corruption_of_an_animated_model_is_handled() {
        let bytes = animated_model();
        for i in 0..bytes.len() {
            for mask in [0x01u8, 0x80, 0xFF] {
                let mut corrupt = bytes.clone();
                if let Some(b) = corrupt.get_mut(i) {
                    *b ^= mask;
                }
                let _ = Model::parse(&corrupt);
            }
        }
    }

    #[test]
    fn empty_input_is_an_empty_model() {
        let m = Model::parse(&[]).expect("no chunks is not an error");
        assert!(m.meshes.is_empty());
        assert!(m.chunk_counts.is_empty());
    }

    #[test]
    fn a_depth_bomb_is_refused_by_the_histogram_walk_too() {
        let mut payload = Vec::new();
        for _ in 0..crate::MAX_CHUNK_DEPTH.saturating_add(4) {
            payload = chunk(ChunkType::MESH, true, &payload);
        }
        assert!(matches!(
            Model::parse(&payload),
            Err(crate::Error::TooDeep { .. })
        ));
    }
}
