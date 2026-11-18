use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_modules::CollapseState;
use mint_sage_shroud::GridExtent;
use mint_sim::{Entity, Frame, PlayerId, Simulation};

use crate::construction::BuildTask;
use crate::render_state::{self, PoseTrack, ProjectileView, RenderState, UnitView};
use crate::rules::contain::Rider;
use crate::rules::map_scripts::ScriptView;
use crate::rules::RotwkRules;
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    frame: u64,
    viewer: PlayerId,
    selector: PlayerId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Motion {
    was: Option<(Vec2, f32, f32)>,
    z: f32,
    offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitFacts {
    pub(crate) opacity: f32,
    pub(crate) casts_shadow: bool,
    pub(crate) geometry_kind: Option<Kind>,
    pub(crate) collapse: Option<(CollapseState, f32, f32)>,
    pub(crate) stealth_hides: bool,
    pub(crate) hidden: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FrameSnapshot {
    key: Option<Key>,
    units: Vec<UnitView>,
    motion: Vec<Motion>,
    facts: Vec<UnitFacts>,
    geometry_overrides: Vec<(Entity, Vec<(Arc<str>, bool)>)>,
    projectiles: Vec<ProjectileView>,
    projectile_was: Vec<Option<(f32, f32, f32)>>,
    clock: Option<u64>,
    enclosed: usize,
    selected_by_local: usize,
    playable: Option<GridExtent>,
    script_view: ScriptView,
    script_bloom: Option<bool>,
    build_orders: Vec<(Entity, PlayerId, BuildTask)>,
    riders: Vec<Rider>,
    follow_at: Option<[f32; 3]>,
    look_object_at: Option<[f32; 3]>,
}

impl FrameSnapshot {
    #[must_use]
    pub(crate) fn build(
        simulation: &Simulation<RotwkRules>,
        previous: &PoseTrack,
        viewer: PlayerId,
        selector: PlayerId,
    ) -> Self {
        let world = simulation.state();
        let rules = simulation.rules();
        let key = Some(Key {
            frame: simulation.frame().get(),
            viewer,
            selector,
        });
        let script_view = rules.script_view(world);
        let script_bloom = rules.script_bloom(world);
        let follow_at = script_view
            .camera_ops
            .follow
            .and_then(|follow| follow.entity)
            .and_then(|entity| rules.entity_follow_point(world, entity));
        let look_object_at = script_view
            .camera_ops
            .look_object
            .and_then(|look| rules.entity_follow_point(world, look.entity));
        let Some(ids) = rules.ids() else {
            return Self {
                key,
                script_view,
                script_bloom,
                follow_at,
                look_object_at,
                ..Self::default()
            };
        };
        let current = RenderState::new(world, 0.0);
        let mut units = current.units(
            ids,
            viewer,
            selector,
            |weapon, state, moved, now, conditions| {
                rules.weapon_cycle_seconds(weapon, state, moved, now, conditions)
            },
            |weapon| rules.weapon_firing_duration(weapon),
        );
        render_state::apply_disguise(&mut units, world, rules, viewer);
        let now = simulation.frame();
        let upgrades = rules.upgrade_state_id();
        let mut motion = Vec::with_capacity(units.len());
        let mut facts = Vec::with_capacity(units.len());
        let mut geometry_overrides = Vec::new();
        for unit in &units {
            let entity = unit.entity;
            motion.push(Motion {
                was: previous.unit(entity),
                z: world.get(ids.elevation, entity).map_or(0.0, |e| e.0),
                offset: world.get(ids.ground_offset, entity).map_or(0.0, |o| o.0),
            });
            let upgrade = upgrades.and_then(|id| world.get(id, entity));
            if let Some(state) = upgrade.filter(|state| !state.geometry.is_empty()) {
                geometry_overrides.push((entity, state.geometry.clone()));
            }
            facts.push(UnitFacts {
                opacity: rules
                    .mount_opacity(world, entity)
                    .min(rules.disguise_opacity(world, entity))
                    .min(rules.corpse_fade_opacity(world, entity, now))
                    .min(rules.join_fade_opacity(world, entity, now)),
                casts_shadow: !upgrade.is_some_and(|held| {
                    held.model_conditions.iter().any(|name| {
                        name.eq_ignore_ascii_case("CLIMBING")
                            || name.eq_ignore_ascii_case("RAPPELLING")
                    })
                }),
                geometry_kind: RotwkRules::geometry_kind(world, ids, entity),
                collapse: unit
                    .dying
                    .then(|| rules.collapse_view(world, entity))
                    .flatten(),
                stealth_hides: rules.stealth_look_hides(world, ids, viewer, entity),
                hidden: rules.hidden_from(world, ids, viewer, entity),
            });
        }
        let projectiles = current.projectiles(ids);
        let projectile_was = projectiles
            .iter()
            .map(|shot| previous.projectile(shot.entity))
            .collect();
        Self {
            key,
            units,
            motion,
            facts,
            geometry_overrides,
            projectiles,
            projectile_was,
            clock: current.match_clock(ids.clock),
            enclosed: current.enclosed(ids),
            selected_by_local: current.selected_count(ids, crate::game::LOCAL_PLAYER),
            playable: crate::shroud::with_grid(world, RotwkRules::shroud_ids(ids), |grid| {
                grid.extent()
            }),
            build_orders: rules.shown_build_orders(world, ids, crate::game::LOCAL_PLAYER),
            riders: rules.riders(world, ids),
            script_view,
            script_bloom,
            follow_at,
            look_object_at,
        }
    }

    #[must_use]
    pub(crate) fn is_for(&self, frame: Frame, viewer: PlayerId, selector: PlayerId) -> bool {
        self.key
            == Some(Key {
                frame: frame.get(),
                viewer,
                selector,
            })
    }

    #[must_use]
    pub(crate) fn sees_for(&self, frame: Frame, viewer: PlayerId) -> bool {
        self.key
            .is_some_and(|key| key.frame == frame.get() && key.viewer == viewer)
    }

    pub(crate) fn invalidate(&mut self) {
        self.key = None;
    }

    #[must_use]
    pub(crate) fn units(&self, blend: Option<f32>) -> Vec<UnitView> {
        let mut units = self.units.clone();
        let Some(blend) = blend.map(render_state::clamp_blend) else {
            return units;
        };
        for (unit, motion) in units.iter_mut().zip(&self.motion) {
            let Some(was) = motion.was else {
                continue;
            };
            let (position, z, facing) =
                render_state::blend_pose(was, (unit.position, motion.z, unit.facing), blend);
            unit.position = position;
            unit.ground = z + motion.offset;
            unit.facing = facing;
        }
        units
    }

    #[must_use]
    pub(crate) fn projectiles(&self, blend: Option<f32>) -> Vec<ProjectileView> {
        let mut shots = self.projectiles.clone();
        let Some(blend) = blend.map(render_state::clamp_blend) else {
            return shots;
        };
        for (shot, was) in shots.iter_mut().zip(&self.projectile_was) {
            if let Some((x, y, z)) = *was {
                shot.x = render_state::lerp(x, shot.x, blend);
                shot.y = render_state::lerp(y, shot.y, blend);
                shot.z = render_state::lerp(z, shot.z, blend);
            }
        }
        shots
    }

    #[must_use]
    pub(crate) fn facts(&self, entity: Entity) -> Option<&UnitFacts> {
        let at = self
            .units
            .binary_search_by_key(&entity.index(), |unit| unit.entity.index())
            .ok()?;
        self.units
            .get(at)
            .filter(|unit| unit.entity == entity)
            .and_then(|_| self.facts.get(at))
    }

    #[must_use]
    pub(crate) fn geometry_overrides(&self, entity: Entity) -> &[(Arc<str>, bool)] {
        self.geometry_overrides
            .binary_search_by_key(&entity.index(), |(held, _)| held.index())
            .ok()
            .and_then(|at| self.geometry_overrides.get(at))
            .filter(|(held, _)| *held == entity)
            .map_or(&[][..], |(_, overrides)| overrides.as_slice())
    }

    #[must_use]
    pub(crate) fn frame(&self) -> Frame {
        Frame::new(self.key.map_or(0, |key| key.frame))
    }

    #[must_use]
    pub(crate) fn build_orders(&self) -> &[(Entity, PlayerId, BuildTask)] {
        &self.build_orders
    }

    #[must_use]
    pub(crate) fn riders(&self) -> &[Rider] {
        &self.riders
    }

    #[must_use]
    pub(crate) const fn clock(&self) -> Option<u64> {
        self.clock
    }

    #[must_use]
    pub(crate) const fn enclosed(&self) -> usize {
        self.enclosed
    }

    #[must_use]
    pub(crate) const fn selected_by_local(&self) -> usize {
        self.selected_by_local
    }

    #[must_use]
    pub(crate) fn playable(&self, x: f32, y: f32) -> bool {
        self.playable.is_none_or(|extent| extent.contains(x, y))
    }

    #[must_use]
    pub(crate) const fn script_view(&self) -> &ScriptView {
        &self.script_view
    }

    #[must_use]
    pub(crate) const fn follow_at(&self) -> Option<[f32; 3]> {
        self.follow_at
    }

    #[must_use]
    pub(crate) const fn look_object_at(&self) -> Option<[f32; 3]> {
        self.look_object_at
    }

    #[must_use]
    pub(crate) const fn script_bloom(&self) -> Option<bool> {
        self.script_bloom
    }
}

static FALLBACKS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn note_fallback() {
    FALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn fallbacks() -> u64 {
    FALLBACKS.load(Ordering::Relaxed)
}

pub(crate) fn ensure(
    snapshot: &mut FrameSnapshot,
    simulation: &Simulation<RotwkRules>,
    previous: &PoseTrack,
    viewer: PlayerId,
    selector: PlayerId,
) -> bool {
    if snapshot.is_for(simulation.frame(), viewer, selector) {
        return false;
    }
    let started = std::time::Instant::now();
    *snapshot = FrameSnapshot::build(simulation, previous, viewer, selector);
    crate::render_perf::add_current(
        crate::render_perf::stage!("snapshot"),
        started.elapsed().as_secs_f64() * 1000.0,
    );
    true
}

#[cfg(test)]
mod tests;
