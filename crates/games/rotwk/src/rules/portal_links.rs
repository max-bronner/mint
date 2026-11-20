use mint_movement::{
    Cell, CellGeometry, GridExtent, Leg, Portal, PortalLink, PortalNode, PortalPass, Vec2,
};
use mint_sage_modules::{Link, WayPoint, WayPointType};
use mint_sim::{Entity, World};

use super::{Ids, RotwkRules};

pub(super) const KIND_WALK: u8 = 2;
pub(super) const KIND_CLIMB: u8 = 3;
const KIND_PRE_CLIMB: u8 = 4;
pub(super) const KIND_FAKE: u8 = 7;

pub(crate) fn leg_of_kind(kind: u8) -> Leg {
    match kind {
        KIND_WALK | KIND_PRE_CLIMB => Leg::Walk,
        KIND_CLIMB => Leg::Climb,
        1 => Leg::Bare,
        KIND_FAKE => Leg::Halt,
        _ => Leg::Through,
    }
}

fn kind_of(point: WayPointType) -> u8 {
    match point {
        WayPointType::Walk => KIND_WALK,
        WayPointType::Climb => KIND_CLIMB,
        WayPointType::PreClimb => KIND_PRE_CLIMB,
    }
}

pub(crate) fn tag_id(entity: Entity, line: u32) -> u32 {
    ((entity.index() & 0x00FF_FFFF) << 8) | (line & 0xFF)
}

#[cfg(test)]
pub(crate) fn tag_owner(id: u32) -> (u32, u32) {
    (id >> 8, id & 0xFF)
}

pub(crate) struct PortalSite<'a> {
    pub(crate) entity: Entity,
    pub(crate) waypoints: &'a [WayPoint],
    pub(crate) links: &'a [Link],
    pub(crate) bones: &'a [crate::perches::Perch],
    pub(crate) centre: Vec2,
    pub(crate) angle: f32,
    pub(crate) geometry: CellGeometry,
    pub(crate) extent: GridExtent,
}

struct Placed {
    position: Vec2,
    z: f32,
    kind: u8,
    cell: Cell,
}

impl PortalSite<'_> {
    fn waypoint(&self, line: u32) -> Option<Placed> {
        let point = self.waypoints.get(usize::try_from(line).ok()?)?;
        let bone = self.bones.get(usize::try_from(point.index).ok()?)?;
        let (sin, cos) = mint_math::f32::sin_cos(self.angle);
        let [x, y, z] = bone.position;
        let position = Vec2::new(
            self.centre.x + x * cos - y * sin,
            self.centre.y + x * sin + y * cos,
        );
        Some(Placed {
            position,
            z,
            kind: kind_of(point.kind),
            cell: self.geometry.cell_at(position, 0, self.extent)?,
        })
    }

    fn tag(&self, from: &Placed, line: u32, rise: f32) -> Portal {
        Portal {
            id: tag_id(self.entity, line),
            kind: from.kind,
            leg: leg_of_kind(from.kind),
            rise,
        }
    }

    pub(crate) fn links(&self) -> Vec<PortalLink> {
        let mut out = Vec::new();
        for (at, link) in self.links.iter().enumerate() {
            let (Some(entry), Some(exit)) = (self.waypoint(link.from), self.waypoint(link.to))
            else {
                continue;
            };
            let mut lines = vec![link.from];
            lines.extend(link.via.iter().rev().copied());
            let head = self.links.iter().enumerate().find_map(|(other, held)| {
                let at_exit = self
                    .waypoint(held.from)
                    .is_some_and(|p| p.cell == exit.cell);
                (other != at && at_exit).then_some(held.from)
            });
            lines.extend(head);
            let Some(placed) = lines
                .iter()
                .map(|line| self.waypoint(*line))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut nodes = Vec::with_capacity(placed.len());
            for (k, point) in placed.iter().enumerate() {
                let tag = k.checked_sub(1).and_then(|before| {
                    let from = placed.get(before)?;
                    Some(self.tag(from, *lines.get(before)?, point.z - from.z))
                });
                nodes.push(PortalNode {
                    position: point.position,
                    tag,
                });
            }
            let after = placed
                .last()
                .zip(lines.last())
                .map(|(last, line)| self.tag(last, *line, 0.0));
            out.push(PortalLink {
                entry: entry.cell,
                exit: exit.cell,
                slot: 0,
                nodes,
                after,
            });
        }
        out
    }
}

pub(crate) fn fake_gate_links(
    entity: Entity,
    centre: Vec2,
    angle: f32,
    reach: f32,
    geometry: CellGeometry,
    extent: GridExtent,
) -> Vec<PortalLink> {
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    let side = |sign: f32| {
        let along = sign * (reach + 10.0);
        Vec2::new(centre.x + cos * along, centre.y + sin * along)
    };
    let (near, far) = (side(-1.0), side(1.0));
    let (Some(near_cell), Some(far_cell)) = (
        geometry.cell_at(near, 0, extent),
        geometry.cell_at(far, 0, extent),
    ) else {
        return Vec::new();
    };
    if near_cell == far_cell {
        return Vec::new();
    }
    let tag = |line: u32| Portal {
        id: tag_id(entity, line),
        kind: KIND_FAKE,
        leg: leg_of_kind(KIND_FAKE),
        rise: 0.0,
    };
    let link = |entry: Cell, exit: Cell, from: Vec2, to: Vec2, line: u32| PortalLink {
        entry,
        exit,
        slot: 0,
        nodes: vec![
            PortalNode {
                position: from,
                tag: None,
            },
            PortalNode {
                position: to,
                tag: Some(tag(line)),
            },
        ],
        after: Some(tag(line ^ 1)),
    };
    vec![
        link(near_cell, far_cell, near, far, 0),
        link(far_cell, near_cell, far, near, 1),
    ]
}

impl RotwkRules {
    pub(super) fn portal_pass_in(&self, world: &World, ids: Ids, mover: Entity) -> PortalPass {
        self.portal_pass_for(world, ids, mover, true)
    }

    pub(super) fn portal_pass_aside(&self, world: &World, ids: Ids, mover: Entity) -> PortalPass {
        self.portal_pass_for(world, ids, mover, false)
    }

    fn portal_pass_for(
        &self,
        world: &World,
        ids: Ids,
        mover: Entity,
        fake_links: bool,
    ) -> PortalPass {
        let Some(layer) = crate::portals::layer_in(world, ids.portals) else {
            return PortalPass::NONE;
        };
        let pass = layer
            .slots()
            .into_iter()
            .filter(|(_, portal)| self.portal_admits(world, ids, *portal, mover, fake_links))
            .fold(PortalPass::NONE, |pass, (slot, _)| pass.with(slot));
        crate::diag::diag!(
            "portal: mover {} is admitted to {} of {} link(s)",
            mover.index(),
            (0..PortalPass::SLOTS)
                .filter(|slot| pass.has(*slot))
                .count(),
            layer.links().len()
        );
        pass
    }

    fn portal_admits(
        &self,
        world: &World,
        ids: Ids,
        portal: Entity,
        mover: Entity,
        fake_links: bool,
    ) -> bool {
        let Some(wiring) = self.modules.as_ref() else {
            return false;
        };
        if let Some(gate) = wiring.portal(world, portal) {
            return fake_links && self.gate_link_admits(world, ids, portal, mover, gate);
        }
        let Some(module) = wiring.dynamic_portal(world, portal) else {
            return false;
        };
        if !module.allow_enemies() {
            let (Some(owner), Some(rider)) = (
                world.get(ids.owner, portal).map(|held| held.0),
                world.get(ids.owner, mover).map(|held| held.0),
            ) else {
                return false;
            };
            if self.alliances.stance(owner, rider) != crate::alliance::Stance::Allied {
                return false;
            }
        }
        world
            .get(ids.kind, mover)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                module
                    .filter()
                    .passes(&entry.record.kind_of, &entry.record.name)
            })
    }
}

impl RotwkRules {
    fn gate_link_admits(
        &self,
        world: &World,
        ids: Ids,
        gate: Entity,
        mover: Entity,
        module: &mint_sage_modules::FakePathfindPortalBehaviour,
    ) -> bool {
        let (Some(owner), Some(rider)) = (
            world.get(ids.owner, gate).map(|held| held.0),
            world.get(ids.owner, mover).map(|held| held.0),
        ) else {
            return false;
        };
        (module.allow_enemies()
            || self.alliances.stance(owner, rider) == crate::alliance::Stance::Allied)
            && (module.allow_non_skirmish_ai_units()
                || self.ai_seeds.iter().any(|seed| seed.player == rider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_movement::Facing;

    #[test]
    fn the_shipped_pair_walks_the_bones_in_order_and_types_legs_by_the_bone_they_leave() {
        use WayPointType::{Climb, PreClimb};
        let lines = [
            (0, PreClimb),
            (1, Climb),
            (2, Climb),
            (3, Climb),
            (2, Climb),
            (1, PreClimb),
        ];
        let waypoints: Vec<WayPoint> = lines
            .iter()
            .map(|(index, kind)| WayPoint {
                index: *index,
                kind: *kind,
            })
            .collect();
        let links = [
            Link {
                from: 0,
                via: vec![4, 5],
                to: 3,
            },
            Link {
                from: 3,
                via: vec![1, 2],
                to: 0,
            },
        ];
        let bones: Vec<crate::perches::Perch> = (0..4u8)
            .map(|i| crate::perches::Perch {
                position: [f32::from(i) * 30.0, 0.0, f32::from(i) * 5.0],
                facing: Facing::EAST,
            })
            .collect();
        let site = PortalSite {
            entity: Entity::from_parts(5, 0),
            waypoints: &waypoints,
            links: &links,
            bones: &bones,
            centre: Vec2::new(100.0, 100.0),
            angle: 0.0,
            geometry: CellGeometry::new(10.0, Vec2::ZERO).unwrap(),
            extent: GridExtent::new(64, 64).unwrap(),
        };
        let made = site.links();
        let walked = |link: &PortalLink| -> Vec<f32> {
            link.nodes.iter().map(|n| n.position.x - 100.0).collect()
        };
        let typed = |link: &PortalLink| -> Vec<u32> {
            link.nodes
                .iter()
                .filter_map(|n| n.tag)
                .chain(link.after)
                .map(|tag| tag_owner(tag.id).1)
                .collect()
        };
        assert_eq!(made.len(), 2);
        assert_eq!(walked(&made[0]), [0.0, 30.0, 60.0, 90.0]);
        assert_eq!(typed(&made[0]), [0, 5, 4, 3]);
        assert_eq!(walked(&made[1]), [90.0, 60.0, 30.0, 0.0]);
        assert_eq!(typed(&made[1]), [3, 2, 1, 0]);
        assert_eq!(made[0].nodes[1].tag.map(|t| t.rise), Some(5.0));
    }
}
