use mint_sage_formation::{build_drag, optimize_drag, rotate_drag, DragUnit};
use mint_sage_modules::{FormationPreviewDecal, ObjectRecord};

use super::drag_formation::drag_footprint;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PreviewDecals {
    #[default]
    Item,
    Slot,
}

impl PreviewDecals {
    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Item => Self::Slot,
            Self::Slot => Self::Item,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Item => "preview decals=item (retail as dumped)",
            Self::Slot => "preview decals=slot (mint's earlier, switch off)",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FormationPreviewItem {
    pub(crate) centre: Vec2,
    pub(crate) facing: f32,
    pub(crate) texture: String,
    pub(crate) half_width: f32,
    pub(crate) half_height: f32,
    pub(crate) slot: usize,
    pub(crate) item_decal: bool,
}

fn has_texture(decal: Option<&FormationPreviewDecal>) -> bool {
    decal.is_some_and(|decal| !decal.texture.is_empty())
}

pub(super) fn decal_of(
    template: &ObjectRecord,
    mode: PreviewDecals,
) -> Option<(&FormationPreviewDecal, bool)> {
    if mode == PreviewDecals::Item && has_texture(template.formation_preview_item_decal.as_ref()) {
        #[allow(clippy::unwrap_used)]
        return Some((
            template.formation_preview_item_decal.as_ref().unwrap(),
            true,
        ));
    }
    if has_texture(template.formation_preview_decal.as_ref()) {
        #[allow(clippy::unwrap_used)]
        return Some((template.formation_preview_decal.as_ref().unwrap(), false));
    }
    None
}

fn horde_member_locals(slot_local: Vec2, offsets: &[Vec2]) -> Vec<Vec2> {
    if offsets.is_empty() {
        return Vec::new();
    }
    #[allow(clippy::cast_precision_loss)]
    let n = offsets.len() as f32;
    let mean_a = offsets.iter().map(|o| o.x).sum::<f32>() / n;
    let mean_b = offsets.iter().map(|o| o.y).sum::<f32>() / n;
    offsets
        .iter()
        .map(|offset| {
            Vec2::new(
                (slot_local.x + mean_b - offset.y).trunc(),
                (slot_local.y + mean_a - offset.x).trunc(),
            )
        })
        .collect()
}

impl RotwkRules {
    pub(crate) fn formation_preview(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        anchor: Vec2,
        drag: Vec2,
        mode: PreviewDecals,
    ) -> Vec<FormationPreviewItem> {
        let movers = self.formation_preview_movers(world, ids, player);
        if movers.is_empty() {
            return Vec::new();
        }
        let Some(template_id) = self
            .drag_formation
            .choose_template(drag.length(), movers.len())
        else {
            return Vec::new();
        };
        let Some(template) = self.drag_formation.templates.get(template_id) else {
            return Vec::new();
        };
        let units: Vec<DragUnit> = movers
            .iter()
            .filter_map(|&entity| self.formation_preview_unit(world, ids, entity, anchor))
            .collect();
        let mut slots = build_drag(
            template,
            &units,
            self.drag_formation.row_padding,
            self.drag_formation.column_padding,
        );
        if slots.is_empty() {
            return Vec::new();
        }
        let facing = Facing::from_vector(drag).unwrap_or(Facing::EAST);
        optimize_drag(&mut slots, &units, facing);
        slots
            .iter()
            .enumerate()
            .flat_map(|(slot_index, slot)| {
                let Some(entity) = movers.get(slot.unit).copied() else {
                    return Vec::new();
                };
                let Some(record) = world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .map(|entry| &entry.record)
                else {
                    return Vec::new();
                };
                let class = self.drag_formation.classify(&record.kind_of, &record.name);
                if mode == PreviewDecals::Item && record.has_kind("HORDE") {
                    if let Some(items) = self.formation_preview_horde(
                        world, ids, entity, record, class, anchor, facing, slot_index, slot.local,
                    ) {
                        return items;
                    }
                }
                let template = self.preview_template(record, class);
                let Some((decal, item_decal)) = decal_of(template, mode) else {
                    return Vec::new();
                };
                vec![self.formation_preview_item(
                    anchor, facing, slot.local, slot_index, decal, item_decal,
                )]
            })
            .collect()
    }

    fn formation_preview_item(
        &self,
        anchor: Vec2,
        facing: Facing,
        local: Vec2,
        slot_index: usize,
        decal: &FormationPreviewDecal,
        item_decal: bool,
    ) -> FormationPreviewItem {
        FormationPreviewItem {
            centre: anchor.plus(rotate_drag(facing, local)),
            facing: facing.radians(),
            texture: decal.texture.clone(),
            half_width: decal.width / 2.0,
            half_height: decal.height / 2.0,
            slot: slot_index,
            item_decal,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn formation_preview_horde(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        record: &ObjectRecord,
        class: Option<usize>,
        anchor: Vec2,
        facing: Facing,
        slot_index: usize,
        local: Vec2,
    ) -> Option<Vec<FormationPreviewItem>> {
        let own_item = has_texture(record.formation_preview_item_decal.as_ref());
        let fallback = self.preview_object_record(class);
        let fallback_item =
            fallback.is_some_and(|r| has_texture(r.formation_preview_item_decal.as_ref()));
        if !own_item && !fallback_item {
            return None;
        }
        #[allow(clippy::unwrap_used)]
        let template = if own_item { record } else { fallback.unwrap() };
        let Some((decal, item_decal)) = decal_of(template, PreviewDecals::Item) else {
            return Some(Vec::new());
        };
        let Some(horde) = world.get(ids.horde, entity) else {
            return Some(Vec::new());
        };
        let count = horde.members.len();
        if count == 0 {
            return Some(Vec::new());
        }
        let offsets: Vec<Vec2> = horde.offsets().take(count).collect();
        if offsets.is_empty() {
            return Some(Vec::new());
        }
        Some(
            horde_member_locals(local, &offsets)
                .into_iter()
                .map(|member_local| {
                    self.formation_preview_item(
                        anchor,
                        facing,
                        member_local,
                        slot_index,
                        decal,
                        item_decal,
                    )
                })
                .collect(),
        )
    }

    fn preview_template<'a>(
        &'a self,
        record: &'a ObjectRecord,
        class: Option<usize>,
    ) -> &'a ObjectRecord {
        if has_texture(record.formation_preview_decal.as_ref()) {
            return record;
        }
        self.preview_object_record(class).unwrap_or(record)
    }

    fn preview_object_record(&self, class: Option<usize>) -> Option<&ObjectRecord> {
        let name = self
            .drag_formation
            .definitions
            .get(class?)?
            .preview_object
            .as_deref()?;
        let kind = self.kinds.kind(name)?;
        self.kinds.get(kind).map(|entry| &entry.record)
    }

    fn formation_preview_unit(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        anchor: Vec2,
    ) -> Option<DragUnit> {
        let position = world.get(ids.body, entity)?.0.position;
        if world.contains(ids.doomed, entity) {
            return None;
        }
        let record = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)?;
        let class = self.drag_formation.classify(&record.kind_of, &record.name);
        let (width, depth) = drag_footprint(record);
        let template = world
            .get(ids.kind, entity)
            .map_or(u64::MAX, |kind| u64::from(kind.0));
        Some(DragUnit {
            offset: position.minus(anchor),
            class,
            width,
            depth,
            template,
        })
    }

    fn formation_preview_movers(&self, world: &World, ids: Ids, player: PlayerId) -> Vec<Entity> {
        let commandable = |entity: Entity| {
            world
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player)
                || world.contains(ids.dummy, entity)
        };
        let mut targets: Vec<Entity> = world
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let owned: Vec<Entity> = targets
            .iter()
            .copied()
            .filter(|&e| commandable(e))
            .collect();
        let hordes = crate::horde::hordes_of(world, Self::horde_ids(ids), &owned);
        targets.retain(|entity| !world.contains(ids.member, *entity));
        let mut ordered = hordes;
        ordered.append(&mut targets);
        ordered.retain(|&entity| {
            (world.contains(ids.horde, entity) || commandable(entity))
                && world.contains(ids.body, entity)
                && !world.contains(ids.doomed, entity)
                && !Self::emotion_refuses_order(world, ids, entity)
                && !self.siege_refuses_order(world, ids, entity)
        });
        ordered
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod unit_tests {
    use super::*;

    fn decal(texture: &str) -> FormationPreviewDecal {
        FormationPreviewDecal {
            texture: texture.to_owned(),
            width: 1.0,
            height: 1.0,
        }
    }

    fn record_with(item: Option<&str>, slot: Option<&str>) -> ObjectRecord {
        ObjectRecord {
            formation_preview_item_decal: item.map(decal),
            formation_preview_decal: slot.map(decal),
            ..ObjectRecord::default()
        }
    }

    #[test]
    fn decal_of_follows_the_mode_when_a_template_has_both() {
        let record = record_with(Some("Item"), Some("Slot"));
        assert_eq!(
            decal_of(&record, PreviewDecals::Item).unwrap().0.texture,
            "Item"
        );
        assert_eq!(
            decal_of(&record, PreviewDecals::Slot).unwrap().0.texture,
            "Slot"
        );
    }

    #[test]
    fn decal_of_falls_back_to_the_slot_decal_with_no_item_texture() {
        let record = record_with(None, Some("Slot"));
        assert_eq!(
            decal_of(&record, PreviewDecals::Item).unwrap().0.texture,
            "Slot"
        );
    }

    #[test]
    fn decal_of_is_none_with_neither_decal() {
        let record = record_with(None, None);
        assert!(decal_of(&record, PreviewDecals::Item).is_none());
    }

    #[test]
    fn horde_member_locals_matches_the_worked_example() {
        let offsets = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(0.0, 20.0),
        ];
        let locals = horde_member_locals(Vec2::new(5.0, 7.0), &offsets);
        assert_eq!(
            locals,
            vec![
                Vec2::new(15.0, 7.0),
                Vec2::new(5.0, 7.0),
                Vec2::new(-5.0, 7.0),
            ]
        );
    }

    #[test]
    fn horde_member_locals_is_empty_with_no_offsets() {
        assert!(horde_member_locals(Vec2::new(5.0, 7.0), &[]).is_empty());
    }
}
