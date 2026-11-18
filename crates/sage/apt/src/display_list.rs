use std::collections::BTreeMap;

use crate::frame::{ActionRef, Frame, FrameItem, FrameLabel, InitAction, Placement};
use crate::movie::{Transform, Vector2};

const IDENTITY: Placement = Placement {
    transform: Transform {
        m00: 1.0,
        m01: 0.0,
        m10: 0.0,
        m11: 1.0,
    },
    translate: Vector2 { x: 0.0, y: 0.0 },
};

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayObject {
    pub character: u32,
    pub matrix: Placement,
    pub color_transform: Option<u32>,
    pub color_add: Option<u32>,
    pub ratio: Option<f32>,
    pub name: Option<String>,
    pub clip_depth: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayList {
    pub objects: BTreeMap<i32, DisplayObject>,
    pub background_color: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameEvents {
    pub actions: Vec<ActionRef>,
    pub labels: Vec<FrameLabel>,
    pub init_actions: Vec<InitAction>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimelineFrame {
    pub display_list: DisplayList,
    pub events: FrameEvents,
    pub orphan_moves: u32,
}

#[must_use]
pub fn accumulate_timeline(frames: &[Frame]) -> Vec<TimelineFrame> {
    let mut list = DisplayList::default();
    let mut out = Vec::with_capacity(frames.len());
    for frame in frames {
        let mut events = FrameEvents::default();
        let mut orphan_moves: u32 = 0;
        for item in &frame.items {
            match item {
                FrameItem::PlaceObject(po) => {
                    if let Some(character) = po.character {
                        list.objects.insert(
                            po.depth,
                            DisplayObject {
                                character,
                                matrix: po.matrix.unwrap_or(IDENTITY),
                                color_transform: po.color_transform,
                                color_add: po.color_add,
                                ratio: po.ratio,
                                name: po.name.clone(),
                                clip_depth: po.clip_depth,
                            },
                        );
                    } else if let Some(existing) = list.objects.get_mut(&po.depth) {
                        if let Some(m) = po.matrix {
                            existing.matrix = m;
                        }
                        if po.color_transform.is_some() {
                            existing.color_transform = po.color_transform;
                        }
                        if po.color_add.is_some() {
                            existing.color_add = po.color_add;
                        }
                        if po.ratio.is_some() {
                            existing.ratio = po.ratio;
                        }
                        if po.name.is_some() {
                            existing.name = po.name.clone();
                        }
                        if po.clip_depth.is_some() {
                            existing.clip_depth = po.clip_depth;
                        }
                    } else {
                        orphan_moves = orphan_moves.saturating_add(1);
                    }
                }
                FrameItem::RemoveObject(ro) => {
                    list.objects.remove(&ro.depth);
                }
                FrameItem::BackgroundColor(bg) => {
                    list.background_color = Some(bg.color);
                }
                FrameItem::Action(a) => events.actions.push(*a),
                FrameItem::FrameLabel(l) => events.labels.push(l.clone()),
                FrameItem::InitAction(i) => events.init_actions.push(*i),
                FrameItem::Unknown { .. } => {}
            }
        }
        out.push(TimelineFrame {
            display_list: list.clone(),
            events,
            orphan_moves,
        });
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::frame::{BackgroundColor, PlaceObject, PlaceObjectFlags, RemoveObject};

    fn identity_matrix() -> Placement {
        IDENTITY
    }

    fn place(depth: i32, flags: u32, character: Option<u32>) -> FrameItem {
        FrameItem::PlaceObject(PlaceObject {
            flags: PlaceObjectFlags(flags),
            depth,
            character,
            matrix: None,
            color_transform: None,
            color_add: None,
            ratio: None,
            name: None,
            clip_depth: None,
            clip_actions_offset: None,
        })
    }

    #[test]
    fn a_move_only_record_updates_just_its_own_flagged_fields() {
        let mut new_object = place(5, 0x02 | 0x04, Some(3));
        if let FrameItem::PlaceObject(po) = &mut new_object {
            po.matrix = Some(identity_matrix());
        }
        let mut moved = place(5, 0x01 | 0x04, None);
        if let FrameItem::PlaceObject(po) = &mut moved {
            po.matrix = Some(Placement {
                transform: Transform {
                    m00: 1.0,
                    m01: 0.0,
                    m10: 0.0,
                    m11: 1.0,
                },
                translate: Vector2 { x: 10.0, y: 0.0 },
            });
        }
        let frames = vec![
            Frame {
                items: vec![new_object],
            },
            Frame { items: vec![moved] },
        ];
        let out = accumulate_timeline(&frames);
        assert_eq!(out.len(), 2);
        let after_frame1 = out[1].display_list.objects.get(&5).unwrap();
        assert_eq!(
            after_frame1.character, 3,
            "Move never changes the character"
        );
        assert_eq!(after_frame1.matrix.translate.x, 10.0);
        assert_eq!(
            after_frame1.color_transform, None,
            "a field this Move record did not flag stays as it was (never set)"
        );
    }

    #[test]
    fn a_new_placement_does_not_inherit_fields_from_the_old_occupant() {
        let mut first = place(5, 0x02 | 0x20, Some(3));
        if let FrameItem::PlaceObject(po) = &mut first {
            po.name = Some("original".to_string());
        }
        let second = place(5, 0x02, Some(7));
        let frames = vec![
            Frame { items: vec![first] },
            Frame {
                items: vec![second],
            },
        ];
        let out = accumulate_timeline(&frames);
        let after_frame1 = out[1].display_list.objects.get(&5).unwrap();
        assert_eq!(after_frame1.character, 7);
        assert_eq!(
            after_frame1.name, None,
            "a fresh placement must not carry the old object's name forward"
        );
    }

    #[test]
    fn color_add_is_cumulative_like_color_transform() {
        let mut placed = place(5, 0x02 | 0x08, Some(3));
        if let FrameItem::PlaceObject(po) = &mut placed {
            po.color_transform = Some(0xFFFF_FFFF);
            po.color_add = Some(0x0000_8CE0);
        }
        let unrelated_move = place(5, 0x01 | 0x04, None);
        let frames = vec![
            Frame {
                items: vec![placed],
            },
            Frame {
                items: vec![unrelated_move],
            },
        ];
        let out = accumulate_timeline(&frames);
        let after_frame1 = out[1].display_list.objects.get(&5).unwrap();
        assert_eq!(
            after_frame1.color_add,
            Some(0x0000_8CE0),
            "a Move that does not flag HasColorTransform must not clear the running add term"
        );
    }

    #[test]
    fn remove_object_clears_the_depth_and_a_later_frame_does_not_see_it() {
        let frames = vec![
            Frame {
                items: vec![place(5, 0x02, Some(3))],
            },
            Frame {
                items: vec![FrameItem::RemoveObject(RemoveObject { depth: 5 })],
            },
        ];
        let out = accumulate_timeline(&frames);
        assert!(out[0].display_list.objects.contains_key(&5));
        assert!(!out[1].display_list.objects.contains_key(&5));
    }

    #[test]
    fn a_move_with_nothing_at_that_depth_yet_is_a_counted_no_op() {
        let frames = vec![Frame {
            items: vec![place(9, 0x01, None)],
        }];
        let out = accumulate_timeline(&frames);
        assert!(out[0].display_list.objects.is_empty());
        assert_eq!(out[0].orphan_moves, 1);
    }

    #[test]
    fn background_color_persists_until_changed_again() {
        let frames = vec![
            Frame {
                items: vec![FrameItem::BackgroundColor(BackgroundColor {
                    color: 0xffaa_bbcc,
                })],
            },
            Frame { items: vec![] },
        ];
        let out = accumulate_timeline(&frames);
        assert_eq!(out[0].display_list.background_color, Some(0xffaa_bbcc));
        assert_eq!(
            out[1].display_list.background_color,
            Some(0xffaa_bbcc),
            "a colour set on an earlier frame must still hold on a later one \
             that sets nothing new"
        );
    }

    #[test]
    fn actions_labels_and_init_actions_are_collected_but_not_folded_into_the_list() {
        let frames = vec![Frame {
            items: vec![
                FrameItem::Action(ActionRef { block_offset: 100 }),
                FrameItem::FrameLabel(FrameLabel {
                    label: "idle".to_string(),
                    flags: 0x7_0000,
                    frame: 0,
                }),
                FrameItem::InitAction(InitAction {
                    sprite: 4,
                    block_offset: 200,
                }),
            ],
        }];
        let out = accumulate_timeline(&frames);
        assert_eq!(out[0].events.actions.len(), 1);
        assert_eq!(out[0].events.labels[0].label, "idle");
        assert_eq!(out[0].events.init_actions.len(), 1);
        assert!(out[0].display_list.objects.is_empty());
    }

    #[test]
    fn accumulate_timeline_of_no_frames_returns_no_frames() {
        assert!(accumulate_timeline(&[]).is_empty());
    }
}
