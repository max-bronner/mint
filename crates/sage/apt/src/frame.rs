use crate::movie::{Transform, Vector2};
use crate::reader::{checked_add, checked_mul, cstr_at, f32_at, i32_at, u32_at};
use crate::Result;

fn field(base: usize, delta: usize) -> Result<usize> {
    checked_add(base, delta)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionRef {
    pub block_offset: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameLabel {
    pub label: String,
    pub flags: u32,
    pub frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveObject {
    pub depth: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundColor {
    pub color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitAction {
    pub sprite: u32,
    pub block_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub transform: Transform,
    pub translate: Vector2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceObjectFlags(pub u32);

impl PlaceObjectFlags {
    #[must_use]
    pub fn move_existing(self) -> bool {
        self.0 & 0x01 != 0
    }
    #[must_use]
    pub fn has_character(self) -> bool {
        self.0 & 0x02 != 0
    }
    #[must_use]
    pub fn has_matrix(self) -> bool {
        self.0 & 0x04 != 0
    }
    #[must_use]
    pub fn has_color_transform(self) -> bool {
        self.0 & 0x08 != 0
    }
    #[must_use]
    pub fn has_ratio(self) -> bool {
        self.0 & 0x10 != 0
    }
    #[must_use]
    pub fn has_name(self) -> bool {
        self.0 & 0x20 != 0
    }
    #[must_use]
    pub fn has_clip_depth(self) -> bool {
        self.0 & 0x40 != 0
    }
    #[must_use]
    pub fn has_clip_actions(self) -> bool {
        self.0 & 0x80 != 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceObject {
    pub flags: PlaceObjectFlags,
    pub depth: i32,
    pub character: Option<u32>,
    pub matrix: Option<Placement>,
    pub color_transform: Option<u32>,
    pub color_add: Option<u32>,
    pub ratio: Option<f32>,
    pub name: Option<String>,
    pub clip_depth: Option<i32>,
    pub clip_actions_offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipAction {
    pub events: u32,
    pub block_offset: u32,
}

pub fn parse_clip_actions(apt: &[u8], offset: u32) -> crate::Result<Vec<ClipAction>> {
    let base = offset as usize;
    let count = u32_at(apt, base)? as usize;
    let array = u32_at(apt, field(base, 4)?)? as usize;
    let mut out = Vec::with_capacity(count.min(apt.len().saturating_div(12)));
    for index in 0..count {
        let entry = field(array, checked_mul(index, 12)?)?;
        out.push(ClipAction {
            events: u32_at(apt, entry)?,
            block_offset: u32_at(apt, field(entry, 8)?)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrameItem {
    Action(ActionRef),
    FrameLabel(FrameLabel),
    PlaceObject(PlaceObject),
    RemoveObject(RemoveObject),
    BackgroundColor(BackgroundColor),
    InitAction(InitAction),
    Unknown { item_type: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub items: Vec<FrameItem>,
}

pub fn parse_frames(apt: &[u8], frame_count: u32, frames_offset: u32) -> crate::Result<Vec<Frame>> {
    let count = frame_count as usize;
    let mut frames = Vec::with_capacity(count.min(apt.len().saturating_add(1).saturating_div(8)));
    for index in 0..count {
        let frame_base = field(frames_offset as usize, checked_mul(index, 8)?)?;
        let item_count = u32_at(apt, frame_base)? as usize;
        let items_ptr = u32_at(apt, field(frame_base, 4)?)?;
        let mut items =
            Vec::with_capacity(item_count.min(apt.len().saturating_add(1).saturating_div(4)));
        for item_index in 0..item_count {
            let slot = field(items_ptr as usize, checked_mul(item_index, 4)?)?;
            let item_ptr = u32_at(apt, slot)? as usize;
            items.push(parse_item(apt, item_ptr)?);
        }
        frames.push(Frame { items });
    }
    Ok(frames)
}

fn opt_cstr(apt: &[u8], ptr: u32) -> crate::Result<Option<String>> {
    if ptr == 0 {
        Ok(None)
    } else {
        Ok(Some(cstr_at(apt, ptr as usize)?))
    }
}

fn parse_item(apt: &[u8], item_ptr: usize) -> crate::Result<FrameItem> {
    let ty = u32_at(apt, item_ptr)?;
    Ok(match ty {
        1 => FrameItem::Action(ActionRef {
            block_offset: u32_at(apt, field(item_ptr, 4)?)?,
        }),
        2 => FrameItem::FrameLabel(FrameLabel {
            label: cstr_at(apt, u32_at(apt, field(item_ptr, 4)?)? as usize)?,
            flags: u32_at(apt, field(item_ptr, 8)?)?,
            frame: u32_at(apt, field(item_ptr, 12)?)?,
        }),
        3 => FrameItem::PlaceObject(parse_place_object(apt, item_ptr)?),
        4 => FrameItem::RemoveObject(RemoveObject {
            depth: i32_at(apt, field(item_ptr, 4)?)?,
        }),
        5 => FrameItem::BackgroundColor(BackgroundColor {
            color: u32_at(apt, field(item_ptr, 4)?)?,
        }),
        8 => FrameItem::InitAction(InitAction {
            sprite: u32_at(apt, field(item_ptr, 4)?)?,
            block_offset: u32_at(apt, field(item_ptr, 8)?)?,
        }),
        other => FrameItem::Unknown { item_type: other },
    })
}

fn parse_place_object(apt: &[u8], item_ptr: usize) -> crate::Result<PlaceObject> {
    let flags = PlaceObjectFlags(u32_at(apt, field(item_ptr, 4)?)?);
    let depth = i32_at(apt, field(item_ptr, 8)?)?;
    let character = flags
        .has_character()
        .then(|| u32_at(apt, field(item_ptr, 12)?))
        .transpose()?;
    let matrix = if flags.has_matrix() {
        let transform = Transform {
            m00: f32_at(apt, field(item_ptr, 16)?)?,
            m01: f32_at(apt, field(item_ptr, 20)?)?,
            m10: f32_at(apt, field(item_ptr, 24)?)?,
            m11: f32_at(apt, field(item_ptr, 28)?)?,
        };
        let translate = Vector2 {
            x: f32_at(apt, field(item_ptr, 32)?)?,
            y: f32_at(apt, field(item_ptr, 36)?)?,
        };
        Some(Placement {
            transform,
            translate,
        })
    } else {
        None
    };
    let color_transform = flags
        .has_color_transform()
        .then(|| u32_at(apt, field(item_ptr, 40)?))
        .transpose()?;
    let color_add = flags
        .has_color_transform()
        .then(|| u32_at(apt, field(item_ptr, 44)?))
        .transpose()?;
    let ratio = flags
        .has_ratio()
        .then(|| f32_at(apt, field(item_ptr, 48)?))
        .transpose()?;
    let name = if flags.has_name() {
        opt_cstr(apt, u32_at(apt, field(item_ptr, 52)?)?)?
    } else {
        None
    };
    let clip_depth = flags
        .has_clip_depth()
        .then(|| i32_at(apt, field(item_ptr, 56)?))
        .transpose()?;
    let clip_actions_offset = flags
        .has_clip_actions()
        .then(|| u32_at(apt, field(item_ptr, 60)?))
        .transpose()?;
    Ok(PlaceObject {
        flags,
        depth,
        character,
        matrix,
        color_transform,
        color_add,
        ratio,
        name,
        clip_depth,
        clip_actions_offset,
    })
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;

    fn write_u32(bytes: &mut Vec<u8>, offset: usize, value: u32) {
        if bytes.len() < offset + 4 {
            bytes.resize(offset + 4, 0);
        }
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn write_i32(bytes: &mut Vec<u8>, offset: usize, value: i32) {
        write_u32(bytes, offset, value as u32);
    }
    fn write_f32(bytes: &mut Vec<u8>, offset: usize, value: f32) {
        write_u32(bytes, offset, value.to_bits());
    }

    fn synth_frame_bytes() -> Vec<u8> {
        let mut apt = vec![0u8; 4];
        write_u32(&mut apt, 0, 2);
        write_u32(&mut apt, 4, 16);
        write_u32(&mut apt, 16, 100);
        write_u32(&mut apt, 20, 200);
        write_u32(&mut apt, 100, 4);
        write_i32(&mut apt, 104, -3);
        write_u32(&mut apt, 200, 3);
        write_u32(&mut apt, 204, 0x01);
        write_i32(&mut apt, 208, 7);
        apt.resize(200 + 64, 0);
        apt
    }

    #[test]
    fn parses_a_frame_with_remove_and_place() {
        let apt = synth_frame_bytes();
        let frames = parse_frames(&apt, 1, 0).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].items.len(), 2);
        assert!(matches!(
            frames[0].items[0],
            FrameItem::RemoveObject(RemoveObject { depth: -3 })
        ));
        match &frames[0].items[1] {
            FrameItem::PlaceObject(po) => {
                assert_eq!(po.depth, 7);
                assert!(po.flags.move_existing());
                assert!(!po.flags.has_character());
                assert_eq!(po.character, None);
                assert_eq!(po.matrix, None);
            }
            other => panic!("expected PlaceObject, got {other:?}"),
        }
    }

    #[test]
    fn place_object_reads_gated_fields_only_when_their_flag_is_set() {
        let mut apt = synth_frame_bytes();
        write_u32(&mut apt, 204, 0x02 | 0x04 | 0x20);
        write_u32(&mut apt, 212, 42);
        write_f32(&mut apt, 216, 1.0);
        write_f32(&mut apt, 220, 0.0);
        write_f32(&mut apt, 224, 0.0);
        write_f32(&mut apt, 228, 1.0);
        write_f32(&mut apt, 232, 5.0);
        write_f32(&mut apt, 236, 6.0);
        let name_offset = apt.len();
        apt.extend_from_slice(b"button\0");
        write_u32(&mut apt, 252, name_offset as u32);
        write_i32(&mut apt, 256, 999);

        let frames = parse_frames(&apt, 1, 0).unwrap();
        match &frames[0].items[1] {
            FrameItem::PlaceObject(po) => {
                assert_eq!(po.character, Some(42));
                let placement = po.matrix.unwrap();
                assert_eq!(placement.transform.m00, 1.0);
                assert_eq!(placement.translate.x, 5.0);
                assert_eq!(po.name.as_deref(), Some("button"));
                assert_eq!(po.clip_depth, None, "HasClipDepth was not set");
            }
            other => panic!("expected PlaceObject, got {other:?}"),
        }
    }

    #[test]
    fn place_object_reads_the_color_add_term_gated_on_has_color_transform() {
        let mut apt = synth_frame_bytes();
        write_u32(&mut apt, 204, 0x08);
        write_u32(&mut apt, 240, 0xAABB_CCDD);
        write_u32(&mut apt, 244, 0x0080_40C0);
        let frames = parse_frames(&apt, 1, 0).unwrap();
        match &frames[0].items[1] {
            FrameItem::PlaceObject(po) => {
                assert_eq!(po.color_transform, Some(0xAABB_CCDD));
                assert_eq!(po.color_add, Some(0x0080_40C0));
            }
            other => panic!("expected PlaceObject, got {other:?}"),
        }
    }

    #[test]
    fn color_add_is_none_when_has_color_transform_is_clear_even_if_the_bytes_are_not() {
        let mut apt = synth_frame_bytes();
        write_u32(&mut apt, 204, 0x01);
        write_u32(&mut apt, 244, 0x0012_3456);
        let frames = parse_frames(&apt, 1, 0).unwrap();
        match &frames[0].items[1] {
            FrameItem::PlaceObject(po) => {
                assert_eq!(po.color_transform, None);
                assert_eq!(po.color_add, None);
            }
            other => panic!("expected PlaceObject, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_item_type_is_reported_as_data_not_a_panic() {
        let mut apt = vec![0u8; 4];
        write_u32(&mut apt, 0, 1);
        write_u32(&mut apt, 4, 8);
        write_u32(&mut apt, 8, 200);
        apt.resize(204, 0);
        write_u32(&mut apt, 200, 42);
        let frames = parse_frames(&apt, 1, 0).unwrap();
        assert_eq!(frames[0].items[0], FrameItem::Unknown { item_type: 42 });
    }

    #[test]
    fn clip_actions_read_events_and_block_skipping_the_second_flags_word() {
        let mut apt = vec![0u8; 0];
        write_u32(&mut apt, 0, 2);
        write_u32(&mut apt, 4, 16);
        write_u32(&mut apt, 16, 0x0000_0001);
        write_u32(&mut apt, 20, 0xDEAD_BEEF);
        write_u32(&mut apt, 24, 0x300);
        write_u32(&mut apt, 28, 0x0004_0000);
        write_u32(&mut apt, 36, 0x400);
        let actions = parse_clip_actions(&apt, 0).unwrap();
        assert_eq!(
            actions,
            vec![
                ClipAction {
                    events: 1,
                    block_offset: 0x300
                },
                ClipAction {
                    events: 0x0004_0000,
                    block_offset: 0x400
                },
            ]
        );
        for len in 0..apt.len() {
            assert!(parse_clip_actions(&apt[..len], 0).is_err());
        }
        write_u32(&mut apt, 0, u32::MAX);
        assert!(parse_clip_actions(&apt, 0).is_err());
    }

    #[test]
    fn a_frame_count_past_the_buffer_errors_instead_of_over_allocating() {
        assert!(parse_frames(&[0u8; 4], u32::MAX, 0).is_err());
    }
}
