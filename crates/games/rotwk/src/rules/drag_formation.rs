use super::*;

use mint_sage_formation::{build_drag, optimize_drag, rotate_drag, DragUnit};
use mint_sage_modules::ObjectRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InFormationTemplate;

impl Component for InFormationTemplate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl RotwkRules {
    pub(super) fn move_to_drag_formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        anchor: Vec2,
        drag: Vec2,
        attack: bool,
    ) -> Result<()> {
        let anchor = self.clamp_move_goal(tick.state(), ids, player, anchor);
        if self.passability.cell_at(anchor).is_none() {
            return Ok(());
        }
        let (_, movers) = self.move_order_targets_and_movers(tick, ids, player, true);
        if self.group_move_tightens(tick, ids, &movers, anchor) {
            return if attack {
                self.attack_move(tick, ids, player, anchor)
            } else {
                self.move_to(tick, ids, player, anchor)
            };
        }
        let Some(template_id) = self
            .drag_formation
            .choose_template(drag.length(), movers.len())
        else {
            return Ok(());
        };
        let Some(template) = self.drag_formation.templates.get(template_id) else {
            return Ok(());
        };
        let units: Vec<DragUnit> = movers
            .iter()
            .filter_map(|entity| self.drag_unit(tick, ids, *entity, anchor))
            .collect();
        let mut slots = build_drag(
            template,
            &units,
            self.drag_formation.row_padding,
            self.drag_formation.column_padding,
        );
        if slots.is_empty() {
            return Ok(());
        }
        let facing = Facing::from_vector(drag).unwrap_or(Facing::EAST);
        optimize_drag(&mut slots, &units, facing);

        let mut points: BTreeMap<Entity, Vec<Vec2>> = BTreeMap::new();
        for slot in &slots {
            let Some(entity) = movers.get(slot.unit).copied() else {
                continue;
            };
            let destination = anchor.plus(rotate_drag(facing, slot.local));
            points.insert(entity, vec![destination]);
        }
        if points.is_empty() {
            return Ok(());
        }
        let order = super::group_move::FormationOrder { points, facing };
        if attack {
            self.attack_move_with_drag_formation(tick, ids, player, anchor, order)
        } else {
            self.move_to_with_drag_formation(tick, ids, player, anchor, order)
        }
    }

    fn drag_unit(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        anchor: Vec2,
    ) -> Option<DragUnit> {
        let position = tick.state().get(ids.body, entity)?.0.position;
        if tick.state().contains(ids.doomed, entity) {
            return None;
        }
        let record = self.record_of(tick, ids, entity)?;
        let class = self.drag_formation.classify(&record.kind_of, &record.name);
        let (width, depth) = drag_footprint(record);
        let template = tick
            .state()
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
}

pub(super) fn drag_footprint(record: &ObjectRecord) -> (f32, f32) {
    if record.has_kind("LARGE_RECTANGLE_PATHFIND") {
        if let (Some(major), Some(minor)) =
            (record.geometry.major_radius, record.geometry.minor_radius)
        {
            let depth = (2.0 * major + 4.0).trunc();
            let width = (2.0 * minor + 4.0).trunc();
            return (width, depth);
        }
    }
    let radius = record.geometry.radius.unwrap_or(0.0).max(0.0);
    let side = (2.0 * radius + 1.0).trunc();
    (side, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_footprint_is_2r_plus_1_truncated() {
        let record = ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                radius: Some(14.7),
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        assert_eq!(drag_footprint(&record), (30.0, 30.0));
    }

    #[test]
    fn large_rectangle_pathfind_reads_the_box_major_minor() {
        let record = ObjectRecord {
            kind_of: vec!["LARGE_RECTANGLE_PATHFIND".into()],
            geometry: mint_sage_modules::Geometry {
                major_radius: Some(20.0),
                minor_radius: Some(10.0),
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        assert_eq!(drag_footprint(&record), (24.0, 44.0));
    }
}
