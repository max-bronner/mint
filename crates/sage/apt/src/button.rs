use crate::movie::{Button, ButtonRecord, Rect, Vector2};
use crate::reader::{checked_add, checked_mul, f32_at, u32_at};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonAction {
    pub condition: u32,
    pub block_offset: u32,
}

pub fn parse_button_actions(apt: &[u8], button: &Button) -> Result<Vec<ButtonAction>> {
    let count = button.action_count as usize;
    let mut out = Vec::with_capacity(count.min(apt.len().saturating_div(8)));
    for index in 0..count {
        let base = checked_add(button.actions_offset as usize, checked_mul(index, 8)?)?;
        out.push(ButtonAction {
            condition: u32_at(apt, base)?,
            block_offset: u32_at(apt, checked_add(base, 4)?)?,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Up,
    Over,
    Down,
}

impl ButtonState {
    fn bit(self) -> u32 {
        match self {
            ButtonState::Up => 0x01,
            ButtonState::Over => 0x02,
            ButtonState::Down => 0x04,
        }
    }
}

pub fn records_for_state(
    button: &Button,
    state: ButtonState,
) -> impl Iterator<Item = &ButtonRecord> {
    let bit = state.bit();
    button.records.iter().filter(move |r| r.flags & bit != 0)
}

pub fn hit_bounds(apt: &[u8], button: &Button) -> Result<Option<Rect>> {
    let count = button.hit_vertex_count as usize;
    if count == 0 {
        return Ok(None);
    }
    let mut min = Vector2 {
        x: f32::MAX,
        y: f32::MAX,
    };
    let mut max = Vector2 {
        x: f32::MIN,
        y: f32::MIN,
    };
    for index in 0..count {
        let base = checked_add(button.hit_vertices_offset as usize, checked_mul(index, 8)?)?;
        let x = f32_at(apt, base)?;
        let y = f32_at(apt, checked_add(base, 4)?)?;
        min.x = min.x.min(x);
        min.y = min.y.min(y);
        max.x = max.x.max(x);
        max.y = max.y.max(y);
    }
    Ok(Some(Rect {
        left: min.x,
        top: min.y,
        right: max.x,
        bottom: max.y,
    }))
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
    use crate::movie::Transform;

    fn record(flags: u32) -> ButtonRecord {
        ButtonRecord {
            flags,
            character: 1,
            depth: 0,
            transform: Transform {
                m00: 1.0,
                m01: 0.0,
                m10: 0.0,
                m11: 1.0,
            },
            translate: Vector2 { x: 0.0, y: 0.0 },
        }
    }

    fn button_with(records: Vec<ButtonRecord>) -> Button {
        Button {
            bounds: Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            },
            hit_triangle_count: 0,
            hit_vertex_count: 0,
            hit_vertices_offset: 0,
            hit_triangles_offset: 0,
            records,
            action_count: 0,
            actions_offset: 0,
        }
    }

    #[test]
    fn a_hit_only_record_is_excluded_from_every_visible_state() {
        let button = button_with(vec![record(0x08)]);
        assert_eq!(records_for_state(&button, ButtonState::Up).count(), 0);
        assert_eq!(records_for_state(&button, ButtonState::Over).count(), 0);
        assert_eq!(records_for_state(&button, ButtonState::Down).count(), 0);
    }

    #[test]
    fn a_record_combining_a_visible_bit_with_hittest_still_draws_for_the_visible_bit() {
        let button = button_with(vec![record(0x02 | 0x04 | 0x08)]);
        assert_eq!(records_for_state(&button, ButtonState::Up).count(), 0);
        assert_eq!(records_for_state(&button, ButtonState::Over).count(), 1);
        assert_eq!(records_for_state(&button, ButtonState::Down).count(), 1);
    }

    #[test]
    fn a_record_with_all_four_bits_draws_for_every_visible_state() {
        let button = button_with(vec![record(0x0f)]);
        assert_eq!(records_for_state(&button, ButtonState::Up).count(), 1);
        assert_eq!(records_for_state(&button, ButtonState::Over).count(), 1);
        assert_eq!(records_for_state(&button, ButtonState::Down).count(), 1);
    }

    fn write_f32(bytes: &mut Vec<u8>, offset: usize, value: f32) {
        if bytes.len() < offset + 4 {
            bytes.resize(offset + 4, 0);
        }
        bytes[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }

    #[test]
    fn hit_bounds_computes_the_vertex_aabb() {
        let mut apt = vec![0u8; 0];
        write_f32(&mut apt, 16, 1.0);
        write_f32(&mut apt, 20, 2.0);
        write_f32(&mut apt, 24, -3.0);
        write_f32(&mut apt, 28, 4.0);
        write_f32(&mut apt, 32, 5.0);
        write_f32(&mut apt, 36, -6.0);
        let mut button = button_with(vec![]);
        button.hit_vertex_count = 3;
        button.hit_vertices_offset = 16;
        let bounds = hit_bounds(&apt, &button).unwrap().unwrap();
        assert_eq!(bounds.left, -3.0);
        assert_eq!(bounds.right, 5.0);
        assert_eq!(bounds.top, -6.0);
        assert_eq!(bounds.bottom, 4.0);
    }

    #[test]
    fn hit_bounds_is_none_when_there_are_no_hit_vertices() {
        let button = button_with(vec![]);
        assert_eq!(hit_bounds(&[], &button).unwrap(), None);
    }

    #[test]
    fn hit_bounds_errors_rather_than_panics_when_the_declared_count_exceeds_the_buffer() {
        let mut button = button_with(vec![]);
        button.hit_vertex_count = 10;
        button.hit_vertices_offset = 0;
        let apt = vec![0u8; 8];
        assert!(hit_bounds(&apt, &button).is_err());
    }

    fn write_u32(bytes: &mut Vec<u8>, offset: usize, value: u32) {
        if bytes.len() < offset + 4 {
            bytes.resize(offset + 4, 0);
        }
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn button_actions_read_condition_then_block_in_file_order() {
        let mut apt = Vec::new();
        write_u32(&mut apt, 8, 0x0001);
        write_u32(&mut apt, 12, 0x100);
        write_u32(&mut apt, 16, 0x0112);
        write_u32(&mut apt, 20, 0x200);
        let mut button = button_with(vec![]);
        button.action_count = 2;
        button.actions_offset = 8;
        let actions = parse_button_actions(&apt, &button).unwrap();
        assert_eq!(
            actions,
            vec![
                ButtonAction {
                    condition: 0x0001,
                    block_offset: 0x100
                },
                ButtonAction {
                    condition: 0x0112,
                    block_offset: 0x200
                },
            ]
        );
        for len in 0..apt.len() {
            assert!(parse_button_actions(&apt[..len], &button).is_err());
        }
        button.action_count = u32::MAX;
        assert!(parse_button_actions(&apt, &button).is_err());
    }

    #[test]
    fn every_prefix_of_a_valid_vertex_buffer_parses_without_panicking() {
        let mut apt = vec![0u8; 16];
        write_f32(&mut apt, 16, 1.0);
        write_f32(&mut apt, 20, 2.0);
        let mut button = button_with(vec![]);
        button.hit_vertex_count = 1;
        button.hit_vertices_offset = 16;
        for len in 0..=apt.len() {
            let _ = hit_bounds(&apt[..len], &button);
        }
    }
}
