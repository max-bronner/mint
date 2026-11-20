pub(crate) mod priority;

use crate::rules::map_scripts::{
    ATTITUDE_AGGRESSIVE, ATTITUDE_ALERT, ATTITUDE_PASSIVE, ATTITUDE_PEACEFUL, ATTITUDE_SLEEP,
};

use mint_movement::{Facing, Vec2};
use mint_sage_combat::{
    nearest_target, surface_distance_squared_2d, within_attack_range, ArmorId, ArmorTemplate,
    BezierArc, Candidate, CombatData, DamageFxTypeId, DamageShape, DamageTypeId, EffectNugget,
    HealthPool, Nugget, Point3, ProjectileFlight, SlowDeathOption, WeaponId, WeaponState,
    WeaponStatus,
};
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, StateHasher};

#[cfg(test)]
pub(crate) use crate::game_data::game_data_from_documents;
pub(crate) use crate::game_data::{
    AiBaseDef, AiTiming, AttackUrgencyValues, CameraDefaults, CommandPointTable, FormationLibrary,
    GameData, MultiplayMultipliers, DEFAULT_GRAVITY, DEFAULT_GROUND_STIFFNESS,
    SHIPPED_ARMOR_BONUS_CAP, SHIPPED_SELL_PERCENTAGE,
};

const AIM_HEIGHT_FRACTION: f32 = 0.5;

pub(crate) const DEFAULT_DESTRUCTION_DELAY: LogicFrames = LogicFrames::from_frames(45);

pub(crate) const SPECTACULAR_DESTRUCTION_DELAY: LogicFrames = LogicFrames::from_frames(20);
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Health(pub(crate) HealthPool);

impl Component for Health {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DamageRung {
    Pristine,
    Damaged,
    ReallyDamaged,
    Rubble,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DamageThresholds {
    pub(crate) damaged: f32,
    pub(crate) really: f32,
}

impl DamageThresholds {
    pub(crate) const DEFAULT: Self = Self {
        damaged: 0.5,
        really: 0.1,
    };

    pub(crate) fn of(record: Option<&mint_sage_modules::ObjectRecord>, global: Self) -> Self {
        let Some(record) = record else {
            return global;
        };
        let max = record
            .max_health
            .filter(|max| max.is_finite() && *max > 0.0);
        let defaults = record.use_default_damage_settings.unwrap_or(true);
        let ratio = |value: Option<f32>, fallback: f32| match (
            value.filter(|v| v.is_finite() && *v >= 0.0),
            max,
        ) {
            (Some(value), Some(max)) if value > 0.0 || !defaults => (value / max).min(1.0),
            _ if defaults => fallback,
            _ => 0.0,
        };
        Self {
            damaged: ratio(record.max_health_damaged, global.damaged),
            really: ratio(record.max_health_really_damaged, global.really),
        }
    }

    pub(crate) fn rung(self, fraction: f32) -> DamageRung {
        if fraction <= 0.0 {
            DamageRung::Rubble
        } else if fraction <= self.really {
            DamageRung::ReallyDamaged
        } else if fraction <= self.damaged {
            DamageRung::Damaged
        } else {
            DamageRung::Pristine
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Armour(pub(crate) ArmorId);

impl Component for Armour {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Armed {
    pub(crate) weapon: WeaponId,
    pub(crate) state: WeaponState,
    pub(crate) target: Option<Entity>,
    pub(crate) winding: bool,
    pub(crate) shots_at_target: u32,
    pub(crate) last_shot_at: Option<Vec2>,
    pub(crate) next_acquire: Frame,
    pub(crate) reacquire_ready: Option<Frame>,
    pub(crate) squish_reacquire: bool,
    pub(crate) position_shot: Option<Vec2>,
    pub(crate) last_shot_horde: Option<Entity>,
}

impl Component for Armed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.weapon.hash_into(hasher);
        self.state.hash_into(hasher);
        match self.target {
            Some(entity) => {
                hasher.write_u8(1);
                entity.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.winding);
        hasher.write_u32(self.shots_at_target);
        match self.last_shot_at {
            Some(at) => {
                hasher.write_u8(1);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
            }
            None => hasher.write_u8(0),
        }
        self.next_acquire.hash_into(hasher);
        match self.reacquire_ready {
            Some(ready) => {
                hasher.write_u8(1);
                ready.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        if self.squish_reacquire {
            hasher.write_u8(2);
        }
        if let Some(at) = self.position_shot {
            hasher.write_u8(3);
            hasher.write_f32(at.x);
            hasher.write_f32(at.y);
        }
        if let Some(horde) = self.last_shot_horde {
            hasher.write_u8(4);
            horde.hash_into(hasher);
        }
    }
}

impl Armed {
    pub(crate) fn retarget(&mut self, target: Option<Entity>) {
        if self.target != target {
            self.target = target;
            self.winding = false;
            self.position_shot = None;
            self.shots_at_target = 0;
            self.state.end_leech();
            if target.is_some() {
                self.reacquire_ready = None;
                self.squish_reacquire = false;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WeaponSlots {
    pub(crate) set: u8,
    pub(crate) current: u8,
    pub(crate) states: [Option<mint_sage_combat::WeaponState>; mint_sage_combat::SLOT_COUNT],
}

impl Component for WeaponSlots {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.set);
        hasher.write_u8(self.current);
        for state in &self.states {
            match state {
                Some(state) => {
                    hasher.write_u8(1);
                    state.hash_into(hasher);
                }
                None => hasher.write_u8(0),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttackOrder {
    pub(crate) target: Entity,
    pub(crate) planned: bool,
    pub(crate) horde: Option<Entity>,
    pub(crate) guard: bool,
    pub(crate) relation_free: bool,
    pub(crate) order_owner: Option<PlayerId>,
}

impl Component for AttackOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.target.hash_into(hasher);
        hasher.write_bool(self.planned);
        hasher.write_bool(self.guard);
        hasher.write_bool(self.relation_free);
        match self.horde {
            Some(horde) => {
                hasher.write_u8(1);
                horde.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.order_owner {
            Some(owner) => {
                hasher.write_u8(1);
                hasher.write_u8(owner.get());
            }
            None => hasher.write_u8(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HordeTarget {
    pub(crate) target: Option<Entity>,
    pub(crate) next_acquire: Frame,
}

impl Component for HordeTarget {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.target {
            Some(entity) => {
                hasher.write_u8(1);
                entity.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        self.next_acquire.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AttackMoveOrder {
    pub(crate) destination: Vec2,
    pub(crate) facing: Option<Facing>,
    pub(crate) object: Option<(Entity, Vec2, f32)>,
    pub(crate) retries_left: u8,
    pub(crate) hold_until: Option<u64>,
    pub(crate) short_end: Option<Vec2>,
    pub(crate) follow: bool,
    pub(crate) non_final: bool,
}

impl AttackMoveOrder {
    const RETRIES: u8 = 5;

    pub(crate) fn new(
        destination: Vec2,
        facing: Option<Facing>,
        object: Option<(Entity, Vec2, f32)>,
    ) -> Self {
        Self {
            destination,
            facing,
            object,
            retries_left: Self::RETRIES,
            hold_until: None,
            short_end: None,
            follow: false,
            non_final: false,
        }
    }

    pub(crate) fn follow_leg(destination: Vec2, non_final: bool) -> Self {
        Self {
            retries_left: 0,
            follow: true,
            non_final,
            ..Self::new(destination, None, None)
        }
    }
}

impl Component for AttackMoveOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.destination.x);
        hasher.write_f32(self.destination.y);
        hasher.write_bool(self.object.is_some());
        if let Some((object, seen, height)) = self.object {
            object.hash_into(hasher);
            hasher.write_f32(seen.x);
            hasher.write_f32(seen.y);
            hasher.write_f32(height);
        }
        hasher.write_bool(self.facing.is_some());
        hasher.write_f32(self.facing.map_or(0.0, Facing::radians));
        hasher.write_u8(self.retries_left);
        hasher.write_bool(self.follow);
        hasher.write_bool(self.non_final);
        hasher.write_bool(self.hold_until.is_some());
        hasher.write_u64(self.hold_until.unwrap_or(0));
        hasher.write_bool(self.short_end.is_some());
        if let Some(end) = self.short_end {
            hasher.write_f32(end.x);
            hasher.write_f32(end.y);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueuedLeg {
    pub(crate) point: Vec2,
    pub(crate) attack: bool,
    pub(crate) resolve: bool,
    pub(crate) tag: Option<OrderTag>,
    pub(crate) non_final: bool,
    pub(crate) follow: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OrderTag {
    pub(crate) id: u32,
    pub(crate) click: Vec2,
    pub(crate) members: Vec<Entity>,
    pub(crate) copy_of: Option<u32>,
    pub(crate) lap_clone: Option<u32>,
}

impl QueuedLeg {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.point.x);
        hasher.write_f32(self.point.y);
        hasher.write_bool(self.attack);
        hasher.write_bool(self.resolve);
        hasher.write_bool(self.non_final);
        hasher.write_bool(self.follow);
        hasher.write_bool(self.tag.is_some());
        if let Some(tag) = &self.tag {
            hasher.write_u32(tag.id);
            hasher.write_f32(tag.click.x);
            hasher.write_f32(tag.click.y);
            hasher.write_u32(tag.copy_of.map_or(0, |id| id.saturating_add(1)));
            hasher.write_u32(tag.lap_clone.map_or(0, |id| id.saturating_add(1)));
            hasher.write_usize(tag.members.len());
            for member in &tag.members {
                member.hash_into(hasher);
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OrderQueue {
    pub(crate) current: Option<QueuedLeg>,
    pub(crate) queued: Vec<QueuedLeg>,
    pub(crate) idle_checks: u8,
    pub(crate) loop_start: Option<usize>,
}

impl Component for OrderQueue {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.current.is_some());
        if let Some(leg) = &self.current {
            leg.hash_into(hasher);
        }
        hasher.write_usize(self.queued.len());
        for leg in &self.queued {
            leg.hash_into(hasher);
        }
        hasher.write_u8(self.idle_checks);
        hasher.write_usize(self.loop_start.map_or(0, |at| at.saturating_add(1)));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroundOrder {
    pub(crate) slot: u8,
    pub(crate) at: Vec2,
}

impl Component for GroundOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.slot);
        hasher.write_f32(self.at.x);
        hasher.write_f32(self.at.y);
    }
}

pub(crate) fn projectile_template_names(combat: &CombatData) -> Vec<String> {
    let mut names: Vec<String> = combat
        .weapons
        .iter()
        .flat_map(|(_, template)| template.nuggets.iter())
        .filter_map(|nugget| match nugget {
            Nugget::Projectile(projectile) => projectile.projectile_template.clone(),
            _ => None,
        })
        .filter(|name| !name.trim().is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectileKind {
    pub(crate) name: String,
    pub(crate) arc: BezierArc,
    pub(crate) bounce_arc: BezierArc,
    pub(crate) bounce_count: u8,
    pub(crate) bounce_distance: f32,
    pub(crate) ground_hit_fx: Option<String>,
    pub(crate) ground_hit_weapon: Option<WeaponId>,
    pub(crate) ground_bounce_fx: Option<String>,
    pub(crate) ground_bounce_weapon: Option<WeaponId>,
    pub(crate) die_on_impact: bool,
    pub(crate) detonate_calls_kill: bool,
    pub(crate) fx_dies: Vec<mint_sage_modules::FxListDie>,
    pub(crate) flight_path_adjust: f32,
    pub(crate) pre_landing: Option<LandingEmotion>,
    pub(crate) post_landing: Option<LandingEmotion>,
    pub(crate) crush_style: bool,
    pub(crate) pre_landing_time: LogicFrames,
    pub(crate) final_stuck: LogicFrames,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CrushHits(pub(crate) Vec<Entity>);

impl Component for CrushHits {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for entity in &self.0 {
            entity.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LandingEmotion {
    pub(crate) kind: mint_sage_ini::EmotionType,
    pub(crate) radius: f32,
    pub(crate) time: LogicFrames,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Flight {
    pub(crate) kind: Option<crate::unit::Kind>,
    pub(crate) speed: f32,
    pub(crate) bounces_done: u8,
    pub(crate) flight: ProjectileFlight,
    pub(crate) warhead: Option<WeaponId>,
    pub(crate) launcher: Entity,
    pub(crate) victim: Option<Entity>,
    pub(crate) struck: bool,
    pub(crate) source: Option<WeaponId>,
    pub(crate) nugget: u16,
    pub(crate) passenger_roll: Option<bool>,
    pub(crate) carried: Option<Entity>,
    pub(crate) carried_velocity: mint_sage_combat::Point3,
    pub(crate) carried_ballistic: bool,
    pub(crate) falls_to_death: bool,
    pub(crate) object: bool,
}

impl Component for Flight {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.kind.is_some());
        hasher.write_u16(self.kind.map_or(0, |kind| kind.0));
        hasher.write_f32(self.speed);
        hasher.write_u8(self.bounces_done);
        self.flight.hash_into(hasher);
        hasher.write_bool(self.warhead.is_some());
        if let Some(warhead) = self.warhead {
            warhead.hash_into(hasher);
        }
        self.launcher.hash_into(hasher);
        hasher.write_bool(self.victim.is_some());
        if let Some(victim) = self.victim {
            victim.hash_into(hasher);
        }
        hasher.write_bool(self.struck);
        hasher.write_bool(self.source.is_some());
        if let Some(source) = self.source {
            source.hash_into(hasher);
        }
        hasher.write_u16(self.nugget);
        hasher.write_u8(match self.passenger_roll {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        hasher.write_bool(self.carried.is_some());
        if let Some(carried) = self.carried {
            carried.hash_into(hasher);
        }
        self.carried_velocity.hash_into(hasher);
        hasher.write_bool(self.carried_ballistic);
        hasher.write_bool(self.falls_to_death);
        hasher.write_bool(self.object);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Doomed {
    pub(crate) destroy_at: Frame,
    pub(crate) death_index: u32,
    pub(crate) death_conditions: mint_sage_scene::Conditions,
}

impl Component for Doomed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.destroy_at.hash_into(hasher);
        hasher.write_u32(self.death_index);
        let [low, high] = self.death_conditions.bits();
        for word in std::iter::once(low).chain((high != 0).then_some(high)) {
            hasher.write_u64(u64::try_from(word & u128::from(u64::MAX)).unwrap_or(0));
            hasher.write_u64(u64::try_from(word >> 64).unwrap_or(0));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DeathNaming {
    #[default]
    Normal,
    Crushed,
    Detonated,
    Faded,
    Named(mint_sage_combat::DeathTypeId),
}

impl DeathNaming {
    pub(crate) fn hash_into(self, hasher: &mut StateHasher) {
        match self {
            Self::Normal => hasher.write_u8(0),
            Self::Crushed => hasher.write_u8(1),
            Self::Detonated => hasher.write_u8(4),
            Self::Faded => hasher.write_u8(3),
            Self::Named(id) => {
                hasher.write_u8(2);
                id.hash_into(hasher);
            }
        }
    }

    pub(crate) fn of(death_type: Option<mint_sage_combat::DeathTypeId>) -> Self {
        death_type.map_or(Self::Normal, Self::Named)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DamageEvent {
    pub(crate) victim: Entity,
    pub(crate) attacker: Entity,
    pub(crate) amount: f32,
    pub(crate) damage_type: DamageTypeId,
    pub(crate) scalar: f32,
    pub(crate) fx_type: Option<DamageFxTypeId>,
    pub(crate) dealt: f32,
    pub(crate) death: DeathNaming,
    pub(crate) sub_type: Option<DamageTypeId>,
    pub(crate) flanking_bonus: f32,
    pub(crate) weapon: Option<WeaponId>,
    pub(crate) launched: Option<Launched>,
    pub(crate) nugget: u16,
    pub(crate) accepts_add: bool,
    pub(crate) bonuses_applied: bool,
    pub(crate) force_kill: bool,
    pub(crate) drain_life: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Launched {
    pub(crate) trigger: mint_sage_combat::FxTrigger,
    pub(crate) from: Vec2,
}

pub(crate) fn sort_damage(events: &mut [DamageEvent]) {
    events.sort_unstable_by(|a, b| {
        a.victim
            .index()
            .cmp(&b.victim.index())
            .then_with(|| a.attacker.index().cmp(&b.attacker.index()))
            .then_with(|| a.nugget.cmp(&b.nugget))
            .then_with(|| a.amount.to_bits().cmp(&b.amount.to_bits()))
            .then_with(|| a.damage_type.index().cmp(&b.damage_type.index()))
            .then_with(|| a.scalar.to_bits().cmp(&b.scalar.to_bits()))
    });
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Foe {
    pub(crate) entity: Entity,
    pub(crate) position: Vec2,
    pub(crate) z: f32,
    pub(crate) radius: f32,
    pub(crate) height: f32,
    pub(crate) is_structure: bool,
    pub(crate) airborne: Option<crate::templates::AirborneCategory>,
    pub(crate) unattackable: bool,
    pub(crate) not_autoacquirable: bool,
    pub(crate) enclosed: bool,
    pub(crate) ai_kind_of: Option<mint_sage_modules::AiKindOf>,
    pub(crate) volume: Option<mint_sage_combat::BoxVolume>,
}

impl Foe {
    pub(crate) fn reach_volume(&self) -> ReachVolume {
        (self.position, self.radius, self.volume)
    }

    pub(crate) fn aim_point(&self) -> Point3 {
        Point3::new(
            self.position.x,
            self.position.y,
            self.z + self.height * AIM_HEIGHT_FRACTION,
        )
    }
}

pub(crate) fn acquire_target(
    shooter: Vec2,
    shooter_radius: f32,
    foes: &[Foe],
    min_range: f32,
    max_range: f32,
) -> Option<Entity> {
    let here = Point3::new(shooter.x, shooter.y, 0.0);
    let mut candidates: Vec<Candidate> = Vec::new();
    for foe in foes {
        let there = Point3::new(foe.position.x, foe.position.y, 0.0);
        let distance_squared = surface_distance_squared_2d(here, shooter_radius, there, foe.radius);
        if !within_attack_range(distance_squared, min_range, max_range) {
            continue;
        }
        candidates.push(Candidate {
            index: foe.entity.index(),
            distance_squared,
            priority: 0,
        });
    }
    let winner = nearest_target(&mut candidates)?;
    foes.iter()
        .find(|foe| foe.entity.index() == winner.index)
        .map(|foe| foe.entity)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CombatChainTable {
    chains: Vec<(
        mint_sage_modules::AiKindOf,
        Vec<(mint_sage_modules::AiKindOf, f32)>,
    )>,
}

impl CombatChainTable {
    pub(crate) fn new(
        chains: Vec<(
            mint_sage_modules::AiKindOf,
            Vec<(mint_sage_modules::AiKindOf, f32)>,
        )>,
    ) -> Self {
        Self { chains }
    }

    pub(super) fn modifier(
        &self,
        attacker: mint_sage_modules::AiKindOf,
        target: mint_sage_modules::AiKindOf,
    ) -> Option<f32> {
        let Some((_, targets)) = self.chains.iter().find(|(kind, _)| *kind == attacker) else {
            return Some(0.0);
        };
        match targets.iter().find(|(kind, _)| *kind == target) {
            None => Some(0.0),
            Some((_, m)) if *m <= -1.0 => None,
            Some((_, m)) => Some(*m),
        }
    }
}

pub(crate) fn in_range(
    shooter: Vec2,
    shooter_radius: f32,
    target: Vec2,
    target_radius: f32,
    min_range: f32,
    max_range: f32,
) -> bool {
    in_range_volumes(
        (shooter, shooter_radius, None),
        (target, target_radius, None),
        0.0,
        min_range,
        max_range,
    )
}

pub(crate) fn on_odd_retail_frame(frame: u64, period: u64) -> bool {
    frame
        .checked_div(period)
        .is_some_and(|retail_frame| retail_frame % 2 == 1)
}

pub(crate) type ReachVolume = (Vec2, f32, Option<mint_sage_combat::BoxVolume>);

pub(crate) fn in_range_volumes(
    shooter: ReachVolume,
    target: ReachVolume,
    extra: f32,
    min_range: f32,
    max_range: f32,
) -> bool {
    let distance_squared = mint_sage_combat::border_distance_squared(
        (shooter.0.x, shooter.0.y),
        shooter.1,
        shooter.2.as_ref(),
        (target.0.x, target.0.y),
        target.1,
        target.2.as_ref(),
    );
    within_attack_range(distance_squared + extra * extra, min_range, max_range)
}

pub(crate) fn target_allowed(mask: mint_sage_combat::AntiMask, foe: &Foe) -> bool {
    class_allowed(mask, foe.is_structure, foe.airborne)
}

pub(crate) fn class_allowed(
    mask: mint_sage_combat::AntiMask,
    is_structure: bool,
    airborne: Option<crate::templates::AirborneCategory>,
) -> bool {
    if is_structure {
        return mask.contains(mint_sage_combat::AntiMask::GROUND)
            || mask.contains(mint_sage_combat::AntiMask::STRUCTURE);
    }
    if let Some(category) = airborne {
        let bit = match category {
            crate::templates::AirborneCategory::Vehicle => {
                mint_sage_combat::AntiMask::AIRBORNE_VEHICLE
            }
            crate::templates::AirborneCategory::Infantry => {
                mint_sage_combat::AntiMask::AIRBORNE_INFANTRY
            }
            crate::templates::AirborneCategory::Monster => {
                mint_sage_combat::AntiMask::AIRBORNE_MONSTER
            }
        };
        return mask.contains(bit);
    }
    mask.contains(mint_sage_combat::AntiMask::GROUND)
}

pub(crate) fn auto_acquirable(relation: mint_sage_combat::victim::Relation) -> bool {
    matches!(relation, mint_sage_combat::victim::Relation::Enemies)
}

pub(crate) fn combat_victim(foe: &Foe) -> mint_sage_combat::Victim {
    let mut kind_of = mint_sage_asset::KindOf::NONE;
    if foe.unattackable {
        kind_of = kind_of.union(mint_sage_asset::KindOf::UNATTACKABLE);
    }
    if foe.not_autoacquirable {
        kind_of = kind_of.union(mint_sage_asset::KindOf::NOT_AUTOACQUIRABLE);
    }
    mint_sage_combat::Victim {
        live: true,
        concealed: false,
        contained: false,
        kind_of,
        is_structure: foe.is_structure,
        seen: true,
        script_targetable: false,
    }
}

pub(crate) const MOOD_ATTACK_CHECK: LogicFrames = mint_sage_modules::retail_duration_ticks(500);

pub(crate) fn staggered_first_acquire(entity: Entity, now: Frame) -> Frame {
    let cadence = MOOD_ATTACK_CHECK.get();
    let stagger = if cadence == 0 {
        0
    } else {
        entity.index() % cadence
    };
    now.checked_add(LogicFrames::from_frames(stagger))
        .unwrap_or(now)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuardRanges {
    pub(crate) chase: LogicFrames,
    pub(crate) enemy_scan: LogicFrames,
    pub(crate) return_scan: LogicFrames,
    pub(crate) repulsed_distance: f32,
    pub(crate) enable_repulsors: bool,
    pub(crate) crushes_infantry: bool,
}

impl Default for GuardRanges {
    fn default() -> Self {
        Self {
            chase: mint_sage_modules::retail_duration_ticks(15_000),
            enemy_scan: mint_sage_modules::retail_duration_ticks(500),
            return_scan: mint_sage_modules::retail_duration_ticks(1_000),
            repulsed_distance: 120.0,
            enable_repulsors: true,
            crushes_infantry: true,
        }
    }
}

pub(crate) fn read_guard_ranges(
    block: &mint_sage_ini::Block,
    errors: &mint_sage_ini::ValueErrors,
) -> (GuardRanges, usize) {
    let mut ranges = GuardRanges::default();
    let mut read = 0usize;
    let mut number = |name: &str| -> Option<f32> {
        let field = block.field(name)?;
        read = read.saturating_add(1);
        let value = field
            .value()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0);
        if value.is_none() {
            errors.field("AIData", field, "a non-negative number");
        }
        value
    };
    for unread in [
        "GuardInnerModifierAI",
        "GuardOuterModifierAI",
        "GuardInnerModifierHuman",
        "GuardOuterModifierHuman",
        "MaxRetaliateDistance",
        "RetaliateFriendsRadius",
    ] {
        let _ = number(unread);
    }
    if let Some(value) = number("RepulsedDistance") {
        ranges.repulsed_distance = value;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = |value: f32| {
        mint_sage_modules::retail_duration_ticks(value.min(f32::from(u16::MAX) * 1000.0) as u32)
    };
    if let Some(value) = number("GuardChaseUnitsDuration") {
        ranges.chase = frames(value);
    }
    if let Some(value) = number("GuardEnemyScanRate") {
        ranges.enemy_scan = frames(value);
    }
    if let Some(value) = number("GuardEnemyReturnScanRate") {
        ranges.return_scan = frames(value);
    }
    if let Some(field) = block.field("EnableRepulsors") {
        read = read.saturating_add(1);
        match field.value().and_then(mint_sage_ini::value::parse_bool) {
            Some(value) => ranges.enable_repulsors = value,
            None => errors.field("AIData", field, "Yes or No"),
        }
    }
    if let Some(field) = block.field("AICrushesInfantry") {
        read = read.saturating_add(1);
        match field.value().and_then(mint_sage_ini::value::parse_bool) {
            Some(value) => ranges.crushes_infantry = value,
            None => errors.field("AIData", field, "Yes or No"),
        }
    }
    (ranges, read)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IdleScan {
    pub(crate) reach: f32,
    pub(crate) pursues: bool,
    pub(crate) structures: bool,
    pub(crate) cadence: LogicFrames,
    pub(crate) retaliate_only: bool,
}

impl IdleScan {
    pub(crate) fn finds(&self, from: ReachVolume, to: ReachVolume) -> bool {
        in_range_volumes(from, to, 0.0, 0.0, self.reach)
    }
}

const DEFAULT_MOOD_CHECK_MS: u32 = 2000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanInput {
    pub(crate) during_idle: bool,
    pub(crate) flags: mint_sage_modules::AutoAcquire,
    pub(crate) vision: Option<f32>,
    pub(crate) vision_bonus: f32,
    pub(crate) height_bonus: f32,
    pub(crate) mood_check_ms: Option<f32>,
    pub(crate) computer: bool,
    pub(crate) attitude: i8,
    pub(crate) ranges: priority::AttitudeRanges,
}

pub(crate) fn idle_scan(input: ScanInput) -> Option<IdleScan> {
    use mint_sage_modules::AutoAcquire;
    if input.during_idle && !input.flags.contains(AutoAcquire::IDLE) {
        return None;
    }
    let vision = input.vision.filter(|v| v.is_finite() && *v > 0.0)?;
    let reach = vision_reach(
        vision * (1.0 + input.vision_bonus).max(0.0),
        input.height_bonus,
    );
    let reach = match input.attitude {
        ATTITUDE_ALERT => reach * input.ranges.alert,
        ATTITUDE_AGGRESSIVE => reach * input.ranges.aggressive,
        ATTITUDE_SLEEP | ATTITUDE_PEACEFUL => return None,
        _ => reach,
    };
    if !reach.is_finite() || reach <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cadence_ms = input
        .mood_check_ms
        .filter(|ms| ms.is_finite() && *ms >= 0.0)
        .map_or(DEFAULT_MOOD_CHECK_MS, |ms| ms.min(u32::MAX as f32) as u32);
    Some(IdleScan {
        reach,
        pursues: true,
        structures: input.computer || input.flags.contains(AutoAcquire::ATTACK_BUILDINGS),
        cadence: mint_sage_modules::retail_duration_ticks(cadence_ms),
        retaliate_only: input.attitude == ATTITUDE_PASSIVE,
    })
}

pub(crate) fn ground_rise(
    bonus: &mint_sage_modules::VisionBonus,
    ground: &impl Fn(f32, f32) -> Option<f32>,
    x: f32,
    y: f32,
) -> Option<f32> {
    let here = ground(x, y)?;
    let step = core::f32::consts::TAU / f32::from(bonus.test_segments);
    let (mut sum, mut count) = (0.0f32, 0u16);
    for index in 0..bonus.test_segments {
        let (sin, cos) = mint_math::f32::sin_cos(step * f32::from(index));
        let at = (x + bonus.test_radius * cos, y + bonus.test_radius * sin);
        if let Some(height) = ground(at.0, at.1) {
            sum += height;
            count = count.saturating_add(1);
        }
    }
    let rise = here - sum / f32::from(count);
    (count > 0 && rise.is_finite()).then_some(rise)
}

pub(crate) fn height_bonus(bonus: &mint_sage_modules::VisionBonus, rise: f32) -> f32 {
    let value = rise * bonus.per_foot;
    if value.is_finite() {
        value.max(bonus.min).min(bonus.max)
    } else {
        0.0
    }
}

pub(crate) fn vision_reach(base: f32, height_bonus: f32) -> f32 {
    base * (1.0 + height_bonus).max(0.0)
}

pub(crate) const TARGET_GRID_CELL: f32 = 40.0;

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn grid_cell(coord: f32) -> i32 {
    if !coord.is_finite() {
        return 0;
    }
    (coord / TARGET_GRID_CELL)
        .floor()
        .clamp(-2_000_000.0, 2_000_000.0) as i32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn grid_radius_cells(world_radius: f32) -> i32 {
    if !world_radius.is_finite() || world_radius <= 0.0 {
        return 0;
    }
    (world_radius / TARGET_GRID_CELL)
        .ceil()
        .clamp(0.0, 100_000.0) as i32
}

pub(crate) struct TargetGrid {
    entries: Vec<(i32, i32, u32)>,
    rows: Option<(i32, i32)>,
}

impl TargetGrid {
    pub(crate) fn build(living: &[(Foe, PlayerId)]) -> Self {
        let mut entries: Vec<(i32, i32, u32)> = Vec::with_capacity(living.len());
        let mut rows: Option<(i32, i32)> = None;
        for (at, (foe, _)) in living.iter().enumerate() {
            let Ok(index) = u32::try_from(at) else {
                continue;
            };
            let (cell_x, cell_y) = (grid_cell(foe.position.x), grid_cell(foe.position.y));
            rows = Some(rows.map_or((cell_y, cell_y), |(lo, hi)| {
                (lo.min(cell_y), hi.max(cell_y))
            }));
            entries.push((cell_x, cell_y, index));
        }
        entries.sort_unstable();
        Self { entries, rows }
    }

    pub(crate) fn occupied_extent_cells(&self, cx: i32, cy: i32) -> i32 {
        let (Some(&(first_x, _, _)), Some(&(last_x, _, _)), Some((first_y, last_y))) =
            (self.entries.first(), self.entries.last(), self.rows)
        else {
            return 0;
        };
        let reach = |centre: i32, lo: i32, hi: i32| {
            let centre = i64::from(centre);
            (centre - i64::from(lo))
                .abs()
                .max((centre - i64::from(hi)).abs())
        };
        let cells = reach(cx, first_x, last_x).max(reach(cy, first_y, last_y));
        i32::try_from(cells.clamp(0, 100_000)).unwrap_or(100_000)
    }

    pub(crate) fn search_radius(
        max_range: f32,
        shooter_radius: f32,
        global_max_radius: f32,
    ) -> f32 {
        max_range.max(0.0) + shooter_radius.max(0.0) + global_max_radius.max(0.0)
    }

    pub(crate) fn query_into(&self, cx: i32, cy: i32, radius_cells: i32, out: &mut Vec<u32>) {
        let lo_cy = cy.saturating_sub(radius_cells);
        let hi_cy = cy.saturating_add(radius_cells);
        let (Some(&(first_x, _, _)), Some(&(last_x, _, _))) =
            (self.entries.first(), self.entries.last())
        else {
            return;
        };
        let hi_cx = cx.saturating_add(radius_cells).min(last_x);
        let mut x = cx.saturating_sub(radius_cells).max(first_x);
        while x <= hi_cx {
            let col_start = self.entries.partition_point(|&(ex, _, _)| ex < x);
            let col_end = self.entries.partition_point(|&(ex, _, _)| ex <= x);
            if let Some(column) = self.entries.get(col_start..col_end) {
                let row_start = column.partition_point(|&(_, ey, _)| ey < lo_cy);
                let row_end = column.partition_point(|&(_, ey, _)| ey <= hi_cy);
                if let Some(row) = column.get(row_start..row_end) {
                    out.extend(row.iter().map(|&(_, _, index)| index));
                }
            }
            let Some(next) = x.checked_add(1) else {
                break;
            };
            x = next;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CrushState {
    pub(crate) crusher_level: u8,
    pub(crate) crushable_level: i16,
    pub(crate) crushes_allies: bool,
    pub(crate) velocity: Vec2,
    pub(crate) facing: Facing,
    pub(crate) volume: Volume,
    pub(crate) too_slow: bool,
}

impl CrushState {
    pub(crate) fn is_ready_to_crush(&self) -> bool {
        self.crusher_level > 0 && !self.too_slow
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CrushSpeedCap {
    pub(crate) cap: f32,
    pub(crate) until: u64,
}

impl Component for CrushSpeedCap {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.cap.to_bits());
        hasher.write_u64(self.until);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CrushContacts(pub(crate) Vec<Entity>);

impl Component for CrushContacts {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for entity in &self.0 {
            entity.hash_into(hasher);
        }
    }
}

pub(crate) const CRUSH_KILL_DAMAGE: f32 = 1.0e7;

const CRUSH_CONTACT_RADIUS: f32 = 5.0;

impl Volume {
    fn shrunk_to(self, radius: f32) -> Self {
        match self {
            Self::Circle(_) => Self::Circle(radius),
            Self::Box(..) => Self::Box(radius, radius),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Volume {
    Circle(f32),
    Box(f32, f32),
}

impl Volume {
    pub(crate) fn reach(self) -> f32 {
        match self {
            Self::Circle(r) => r,
            Self::Box(a, b) => (a * a + b * b).sqrt(),
        }
    }
}

fn half_extent(volume: Volume, facing: Facing, axis: Vec2) -> f32 {
    match volume {
        Volume::Circle(r) => r,
        Volume::Box(a, b) => {
            let f = facing.to_vector();
            let l = Vec2::new(-f.y, f.x);
            a * f.dot(axis).abs() + b * l.dot(axis).abs()
        }
    }
}

pub(crate) fn overlaps(a: (Vec2, Facing, Volume), b: (Vec2, Facing, Volume)) -> bool {
    let d = b.0.minus(a.0);
    if let (Volume::Circle(ra), Volume::Circle(rb)) = (a.2, b.2) {
        return d.length() < ra + rb;
    }
    let mut axes: Vec<Vec2> = Vec::with_capacity(5);
    for (_, facing, volume) in [a, b] {
        if let Volume::Box(..) = volume {
            let f = facing.to_vector();
            axes.push(f);
            axes.push(Vec2::new(-f.y, f.x));
        }
    }
    if let Some(n) = d.normalised() {
        axes.push(n);
    }
    axes.iter().all(|axis| {
        d.dot(*axis).abs() < half_extent(a.2, a.1, *axis) + half_extent(b.2, b.1, *axis)
    })
}

pub(crate) fn run_crush(
    living: &[(Foe, PlayerId)],
    crush: &[CrushState],
    grid: &TargetGrid,
    allied: &dyn Fn(Entity, Entity) -> bool,
) -> Vec<(u32, u32)> {
    let mut hits = Vec::new();
    let mut candidates = Vec::new();
    let victim_reach = Volume::Box(CRUSH_CONTACT_RADIUS, CRUSH_CONTACT_RADIUS).reach();
    for (at, (foe, _)) in living.iter().enumerate() {
        let Some(state) = crush.get(at) else {
            continue;
        };
        if state.crusher_level == 0 || state.velocity.length_squared() <= f32::EPSILON {
            continue;
        }
        let radius_cells = grid_radius_cells(state.volume.reach() + victim_reach);
        candidates.clear();
        grid.query_into(
            grid_cell(foe.position.x),
            grid_cell(foe.position.y),
            radius_cells,
            &mut candidates,
        );
        for &candidate in &candidates {
            if candidate as usize == at {
                continue;
            }
            let (Some((victim_foe, _)), Some(victim_state)) = (
                living.get(candidate as usize),
                crush.get(candidate as usize),
            ) else {
                continue;
            };
            if i16::from(state.crusher_level) <= victim_state.crushable_level
                || (!state.crushes_allies && allied(foe.entity, victim_foe.entity))
            {
                continue;
            }
            let victim = victim_state.volume.shrunk_to(CRUSH_CONTACT_RADIUS);
            if !overlaps(
                (foe.position, state.facing, state.volume),
                (victim_foe.position, victim_state.facing, victim),
            ) {
                continue;
            }
            let to = victim_foe.position.minus(foe.position);
            if foe.z > victim_foe.z + victim_foe.height || victim_foe.z > foe.z + foe.height {
                continue;
            }
            if to.dot(state.velocity) <= 0.0 {
                continue;
            }
            let Ok(crusher_index) = u32::try_from(at) else {
                continue;
            };
            hits.push((crusher_index, candidate));
        }
    }
    hits.sort_unstable();
    hits.dedup();
    hits
}

pub(crate) fn crush_damage_events(
    living: &[(Foe, PlayerId)],
    hits: &[(u32, u32)],
    crush_damage_type: DamageTypeId,
) -> Vec<DamageEvent> {
    hits.iter()
        .filter_map(|&(crusher, crushed)| {
            let attacker = living.get(crusher as usize)?.0.entity;
            let victim = living.get(crushed as usize)?.0.entity;
            Some(DamageEvent {
                victim,
                attacker,
                amount: CRUSH_KILL_DAMAGE,
                damage_type: crush_damage_type,
                scalar: 1.0,
                fx_type: None,
                dealt: 0.0,
                death: DeathNaming::Crushed,
                sub_type: None,
                flanking_bonus: 0.0,
                weapon: None,
                launched: None,
                nugget: 0,
                accepts_add: true,
                bonuses_applied: false,
                force_kill: false,
                drain_life: 0.0,
            })
        })
        .collect()
}

#[derive(Debug, Default)]
pub(crate) struct FireResult {
    pub(crate) damage: Vec<DamageEvent>,
    pub(crate) projectiles: Vec<PendingProjectile>,
    pub(crate) delayed: Vec<u16>,
    pub(crate) travelling: Vec<(DamageEvent, u32)>,
    pub(crate) dots: Vec<DotHit>,
    pub(crate) modifiers: Vec<(Entity, usize)>,
    pub(crate) knockbacks: Vec<(Entity, usize)>,
    pub(crate) projectiles_unaimed: usize,
    pub(crate) creation_lists: Vec<usize>,
    pub(crate) slave_attacks: Vec<Option<Entity>>,
    pub(crate) conditions: Vec<(Entity, usize)>,
    pub(crate) emotion_pulses: Vec<(Entity, usize)>,
    pub(crate) gates_opened: Vec<Entity>,
    pub(crate) money_stolen: Vec<(Entity, usize)>,
    pub(crate) grabs: Vec<(Entity, usize)>,
    pub(crate) paralyses: Vec<(Entity, usize)>,
    pub(crate) unmodelled: Vec<usize>,
    pub(crate) fire_lightings: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DotHit {
    pub(crate) victim: Entity,
    pub(crate) attacker: Entity,
    pub(crate) amount: f32,
    pub(crate) damage_type: DamageTypeId,
    pub(crate) interval: LogicFrames,
    pub(crate) duration: LogicFrames,
    pub(crate) death: DeathNaming,
    pub(crate) sub_type: Option<DamageTypeId>,
    pub(crate) accepts_add: bool,
    pub(crate) from_nugget: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingProjectile {
    pub(crate) warhead: WeaponId,
    pub(crate) victim: Option<Entity>,
    pub(crate) struck: bool,
    pub(crate) from: Point3,
    pub(crate) to: Point3,
    pub(crate) speed: f32,
    pub(crate) source: WeaponId,
    pub(crate) nugget: u16,
    pub(crate) launch_slot: Option<mint_sage_combat::WeaponSlot>,
    pub(crate) always_attack_offset: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Shot {
    pub(crate) weapon: WeaponId,
    pub(crate) attacker: Entity,
    pub(crate) origin: Point3,
    pub(crate) victim: Option<(Entity, Vec2, f32)>,
    pub(crate) missed: Option<Aim>,
    pub(crate) centre: Vec2,
    pub(crate) centre_height: f32,
    pub(crate) owner_if_gone: Option<PlayerId>,
}

impl Shot {
    pub(crate) fn at_victim(
        combat: &CombatData,
        weapon: WeaponId,
        attacker: Entity,
        origin: Point3,
        victim: (Entity, Vec2, f32),
        victim_feet: f32,
    ) -> Self {
        let at_self = combat
            .weapons
            .get(weapon)
            .is_some_and(|template| template.damage_at_self);
        Self {
            weapon,
            attacker,
            origin,
            victim: (!at_self).then_some(victim),
            missed: None,
            centre: if at_self {
                Vec2::new(origin.x, origin.y)
            } else {
                victim.1
            },
            centre_height: if at_self { origin.z } else { victim_feet },
            owner_if_gone: None,
        }
    }

    pub(crate) fn missing(
        weapon: WeaponId,
        attacker: Entity,
        origin: Point3,
        aimed_at: Entity,
        landing: Vec2,
        landing_height: f32,
    ) -> Self {
        Self {
            weapon,
            attacker,
            origin,
            victim: None,
            missed: Some(Aim::Object(aimed_at)),
            centre: landing,
            centre_height: landing_height,
            owner_if_gone: None,
        }
    }

    pub(crate) fn at_ground(
        weapon: WeaponId,
        attacker: Entity,
        origin: Point3,
        landing: Vec2,
        landing_height: f32,
    ) -> Self {
        Self {
            weapon,
            attacker,
            origin,
            victim: None,
            missed: Some(Aim::Ground),
            centre: landing,
            centre_height: landing_height,
            owner_if_gone: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Aim {
    Object(Entity),
    Ground,
}

impl Aim {
    pub(crate) fn object(self) -> Option<Entity> {
        match self {
            Self::Object(entity) => Some(entity),
            Self::Ground => None,
        }
    }
}

pub(crate) trait Judge {
    fn judge(&self, nugget: usize, candidate: Entity, primary: bool) -> Option<f32>;

    fn force_kills(&self, _nugget: usize, _candidate: Entity) -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) struct Everyone;

#[cfg(test)]
impl Judge for Everyone {
    fn judge(&self, _nugget: usize, _candidate: Entity, _primary: bool) -> Option<f32> {
        Some(1.0)
    }
}

#[cfg(test)]
pub(crate) fn run_nuggets(
    combat: &CombatData,
    weapon: WeaponId,
    attacker: Entity,
    origin: Point3,
    victim: Entity,
    victim_position: Vec2,
    victim_height: f32,
    others: &[Foe],
) -> FireResult {
    run_nuggets_for(
        combat,
        &Shot::at_victim(
            combat,
            weapon,
            attacker,
            origin,
            (victim, victim_position, victim_height * AIM_HEIGHT_FRACTION),
            0.0,
        ),
        others.iter().copied(),
        &mint_sim::UpgradeMask::EMPTY,
        &Everyone,
    )
}

fn bounding_sphere_radius(foe: &Foe) -> f32 {
    let half_height = foe.height * 0.5;
    if foe.radius > half_height {
        foe.radius
    } else {
        half_height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    Cylinder,
    Sphere,
    Overlap,
}

impl Reach {
    pub(crate) fn of_nugget(cylinder_aoe: bool, radius: f32) -> Self {
        if cylinder_aoe {
            Self::Cylinder
        } else if radius < 10.0 {
            Self::Sphere
        } else {
            Self::Overlap
        }
    }
}

fn overlaps_cylinder(centre: Point3, radius: f32, foe: &Foe) -> bool {
    let (dx, dy) = (centre.x - foe.position.x, centre.y - foe.position.y);
    let horizontal = ((dx * dx + dy * dy).sqrt() - foe.radius).max(0.0);
    let top = foe.z + foe.height;
    let vertical = if centre.z < foe.z {
        foe.z - centre.z
    } else if centre.z > top {
        centre.z - top
    } else {
        0.0
    };
    horizontal * horizontal + vertical * vertical <= radius * radius
}

fn in_radius(
    others: impl Iterator<Item = Foe>,
    centre: Vec2,
    centre_height: f32,
    radius: f32,
    mode: Reach,
) -> Vec<(u32, Entity)> {
    let _perf = crate::perf::sub(crate::perf::sub_row!("area_victims"));
    let here = Point3::new(centre.x, centre.y, centre_height);
    let mut hit: Vec<(u32, Entity)> = others
        .filter(|foe| !foe.enclosed)
        .filter(|foe| {
            let there = Point3::new(foe.position.x, foe.position.y, foe.z + foe.height * 0.5);
            match mode {
                Reach::Cylinder => {
                    let reach = radius + foe.radius;
                    return here.distance_squared_2d(there) <= reach * reach;
                }
                Reach::Overlap => return overlaps_cylinder(here, radius, foe),
                Reach::Sphere => {}
            }
            let reach = radius + bounding_sphere_radius(foe);
            let reached = here.distance_squared_3d(there) <= reach * reach;
            if here.distance_squared_2d(there) <= reach * reach {
                crate::diag::diag!(
                    "weapon: area at ({:.0},{:.0},{:.1}) {} {} at z={:.1}: 3D {:.1}, 2D {:.1}, reach {reach:.1}",
                    here.x,
                    here.y,
                    here.z,
                    if reached { "reaches" } else { "misses" },
                    foe.entity.index(),
                    foe.z,
                    here.distance_squared_3d(there).sqrt(),
                    here.distance_squared_2d(there).sqrt(),
                );
            }
            reached
        })
        .map(|foe| (foe.entity.index(), foe.entity))
        .collect();
    hit.sort_unstable_by_key(|(index, _)| *index);
    hit
}

fn nugget_victims<I>(
    radius: f32,
    shot: &Shot,
    primary: Option<Entity>,
    others: I,
) -> Vec<(u32, Entity)>
where
    I: Iterator<Item = Foe>,
{
    if shot.victim.is_none() {
        in_radius(
            others,
            shot.centre,
            shot.centre_height,
            radius.max(1.0),
            Reach::Sphere,
        )
    } else if radius > 0.0 {
        in_radius(
            others,
            shot.centre,
            shot.centre_height,
            radius,
            Reach::Sphere,
        )
    } else {
        primary.map(|v| (v.index(), v)).into_iter().collect()
    }
}

pub(crate) fn run_nuggets_for<I>(
    combat: &CombatData,
    shot: &Shot,
    others: I,
    held: &mint_sim::UpgradeMask,
    judge: &dyn Judge,
) -> FireResult
where
    I: Iterator<Item = Foe> + Clone,
{
    run_nuggets_scoped(combat, shot, others, held, judge, NuggetScope::All)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NuggetScope {
    All,
    Landing(u16),
}

pub(crate) fn run_nuggets_scoped<I>(
    combat: &CombatData,
    shot: &Shot,
    others: I,
    held: &mint_sim::UpgradeMask,
    judge: &dyn Judge,
    scope: NuggetScope,
) -> FireResult
where
    I: Iterator<Item = Foe> + Clone,
{
    let mut result = FireResult::default();
    let weapon = shot.weapon;
    let attacker = shot.attacker;
    let Some(template) = combat.weapons.get(weapon) else {
        return result;
    };
    let primary = shot.victim.map(|(entity, ..)| entity);
    for (index, nugget) in template.nuggets.iter().enumerate() {
        if let NuggetScope::Landing(only) = scope {
            if u16::try_from(index) != Ok(only) {
                continue;
            }
        }
        if !combat.nugget_passes(weapon, index, held) {
            continue;
        }
        match nugget {
            Nugget::Damage(damage) => {
                if scope == NuggetScope::All && !damage.delay.is_zero() {
                    if let Ok(index) = u16::try_from(index) {
                        result.delayed.push(index);
                    }
                    continue;
                }
                let victims = if damage.radius > 0.0 || shot.victim.is_none() {
                    let radius = if shot.victim.is_none() {
                        damage.radius.max(1.0)
                    } else {
                        damage.radius
                    };
                    let reached = in_radius(
                        others.clone(),
                        shot.centre,
                        shot.centre_height,
                        radius,
                        Reach::of_nugget(damage.shape.cylinder_aoe, radius),
                    );
                    crate::diag::diag!(
                        "weapon: damage Radius={:.0} at ({:.0},{:.0}) reached {}                          (surface distance, each victim's own sphere added)",
                        radius,
                        shot.centre.x,
                        shot.centre.y,
                        reached.len()
                    );
                    reached
                } else {
                    primary.map(|v| (v.index(), v)).into_iter().collect()
                };
                for (_, entity) in victims {
                    let Some(scalar) = judge.judge(index, entity, Some(entity) == primary) else {
                        continue;
                    };
                    let mut amount = damage.damage;
                    let mut travel = 0_u32;
                    if damage.shape != DamageShape::default() {
                        let Some(foe) = others.clone().find(|foe| foe.entity == entity) else {
                            continue;
                        };
                        let (dx, dy) = (
                            foe.position.x - shot.centre.x,
                            foe.position.y - shot.centre.y,
                        );
                        let flat = (dx * dx + dy * dy).sqrt();
                        if flat < damage.shape.min_radius
                            || damage
                                .shape
                                .max_height
                                .is_some_and(|max| foe.z - shot.origin.z > max)
                        {
                            continue;
                        }
                        if let (Some(rim), true) = (damage.shape.taper_off, damage.radius > 0.0) {
                            let min = damage.shape.min_radius;
                            let span = damage.radius - min;
                            let t = if span > 0.0 {
                                (flat.clamp(min, damage.radius) - min) / span
                            } else {
                                1.0
                            };
                            amount = damage.damage - t * (damage.damage - rim);
                        }
                        if damage.shape.speed > 0.0 {
                            let from = foe.position;
                            let (sx, sy, sz) = (
                                from.x - shot.origin.x,
                                from.y - shot.origin.y,
                                foe.z - shot.origin.z,
                            );
                            let far = (sx * sx + sy * sy + sz * sz).sqrt();
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let frames = (far / damage.shape.speed) as u32;
                            travel = frames;
                        }
                    }
                    let event = DamageEvent {
                        victim: entity,
                        attacker,
                        amount,
                        damage_type: damage.damage_type,
                        scalar: damage.damage_scalar * scalar,
                        fx_type: combat.damage_fx_types.for_hit(weapon.index(), index),
                        dealt: 0.0,
                        death: DeathNaming::of(damage.death_type),
                        sub_type: damage.sub_type,
                        flanking_bonus: damage.flanking_bonus,
                        weapon: Some(weapon),
                        launched: None,
                        nugget: u16::try_from(index).unwrap_or(u16::MAX),
                        accepts_add: damage.accepts_add,
                        bonuses_applied: false,
                        force_kill: judge.force_kills(index, entity),
                        drain_life: if damage.shape.drain_life {
                            damage.shape.drain_multiplier
                        } else {
                            0.0
                        },
                    };
                    if travel > 0 {
                        result.travelling.push((event, travel));
                    } else {
                        result.damage.push(event);
                    }
                }
            }
            Nugget::Dot(dot) => {
                let victims = nugget_victims(dot.radius, shot, primary, others.clone());
                for (_, entity) in victims {
                    let Some(scalar) = judge.judge(index, entity, Some(entity) == primary) else {
                        continue;
                    };
                    result.damage.push(DamageEvent {
                        victim: entity,
                        attacker,
                        amount: dot.damage * scalar,
                        damage_type: dot.damage_type,
                        scalar: 1.0,
                        fx_type: combat.damage_fx_types.for_hit(weapon.index(), index),
                        dealt: 0.0,
                        death: DeathNaming::of(dot.death_type),
                        sub_type: dot.sub_type,
                        flanking_bonus: 0.0,
                        weapon: Some(weapon),
                        launched: None,
                        nugget: u16::try_from(index).unwrap_or(u16::MAX),
                        accepts_add: dot.accepts_add,
                        bonuses_applied: false,
                        force_kill: false,
                        drain_life: 0.0,
                    });
                    result.dots.push(DotHit {
                        victim: entity,
                        attacker,
                        amount: dot.damage * scalar,
                        damage_type: dot.damage_type,
                        interval: dot.interval,
                        duration: dot.duration,
                        death: DeathNaming::of(dot.death_type),
                        sub_type: dot.sub_type,
                        accepts_add: dot.accepts_add,
                        from_nugget: true,
                    });
                }
            }
            Nugget::Modifier(modifier) => {
                let victims = nugget_victims(modifier.radius, shot, primary, others.clone());
                for (_, entity) in victims {
                    if judge
                        .judge(index, entity, Some(entity) == primary)
                        .is_some()
                    {
                        result.modifiers.push((entity, index));
                    }
                }
            }
            Nugget::Projectile(projectile) => {
                let aim = shot
                    .victim
                    .map(|(victim, position, height)| (Some(victim), position, height, true))
                    .or_else(|| {
                        shot.missed
                            .map(|aim| (aim.object(), shot.centre, shot.centre_height, false))
                    });
                let Some((victim, victim_position, aimed_at, struck)) = aim else {
                    result.projectiles_unaimed = result.projectiles_unaimed.saturating_add(1);
                    continue;
                };
                let Some(warhead) = combat.weapons.id(&projectile.warhead) else {
                    continue;
                };
                let to = Point3::new(victim_position.x, victim_position.y, aimed_at);
                let range = Vec2::new(to.x, to.y).minus(Vec2::new(shot.origin.x, shot.origin.y));
                result.projectiles.push(PendingProjectile {
                    warhead,
                    victim,
                    struck,
                    from: shot.origin,
                    to,
                    speed: template.launch_speed(range.length()),
                    source: weapon,
                    launch_slot: projectile.launch_slot,
                    always_attack_offset: projectile.always_attack_offset,
                    nugget: match u16::try_from(index) {
                        Ok(nugget) => nugget,
                        Err(_) => continue,
                    },
                });
            }
            Nugget::MetaImpact(impact) => {
                if scope == NuggetScope::All && !impact.delay.is_zero() {
                    if let Ok(index) = u16::try_from(index) {
                        result.delayed.push(index);
                    }
                    continue;
                }
                let victims = if impact.radius > 0.0 {
                    in_radius(
                        others.clone(),
                        shot.centre,
                        shot.centre_height,
                        impact.radius,
                        Reach::Sphere,
                    )
                } else {
                    primary.map(|v| (v.index(), v)).into_iter().collect()
                };
                for (_, entity) in victims {
                    if judge
                        .judge(index, entity, Some(entity) == primary)
                        .is_some()
                    {
                        result.knockbacks.push((entity, index));
                    }
                }
            }
            Nugget::Unmodelled(_) => {
                let reached = |radius: f32| -> Vec<(u32, Entity)> {
                    if radius > 0.0 {
                        in_radius(
                            others.clone(),
                            shot.centre,
                            shot.centre_height,
                            radius,
                            Reach::Sphere,
                        )
                    } else {
                        primary.map(|v| (v.index(), v)).into_iter().collect()
                    }
                };
                match combat.effects.get(weapon.index(), index) {
                    Some(EffectNugget::WeaponOcl(_)) => result.creation_lists.push(index),
                    Some(EffectNugget::SlaveAttack) => match primary {
                        Some(victim) => {
                            if judge.judge(index, victim, true).is_some() {
                                result.slave_attacks.push(Some(victim));
                            }
                        }
                        None => result.slave_attacks.push(None),
                    },
                    Some(EffectNugget::SpecialModelCondition(_)) => {
                        for (_, entity) in reached(0.0) {
                            if judge
                                .judge(index, entity, Some(entity) == primary)
                                .is_some()
                            {
                                result.conditions.push((entity, index));
                            }
                        }
                    }
                    Some(EffectNugget::EmotionWeapon(nugget)) => {
                        for (_, entity) in reached(nugget.radius) {
                            if judge
                                .judge(index, entity, Some(entity) == primary)
                                .is_some()
                            {
                                result.emotion_pulses.push((entity, index));
                            }
                        }
                    }
                    Some(EffectNugget::OpenGate(nugget)) => {
                        for (_, entity) in reached(nugget.radius) {
                            if judge
                                .judge(index, entity, Some(entity) == primary)
                                .is_some()
                            {
                                result.gates_opened.push(entity);
                            }
                        }
                    }
                    Some(EffectNugget::StealMoney(_)) => {
                        for (_, entity) in reached(0.0) {
                            if judge
                                .judge(index, entity, Some(entity) == primary)
                                .is_some()
                            {
                                result.money_stolen.push((entity, index));
                            }
                        }
                    }
                    Some(EffectNugget::Grab(_)) => {
                        if let Some(victim) = primary {
                            if judge.judge(index, victim, true).is_some() {
                                result.grabs.push((victim, index));
                            }
                        }
                    }
                    Some(EffectNugget::FireLogic(_)) => result.fire_lightings.push(index),
                    Some(EffectNugget::Paralyze(nugget)) => {
                        let primary_hit = primary
                            .filter(|victim| judge.judge(index, *victim, true).is_some())
                            .into_iter();
                        let area = if nugget.radius > 0.0 {
                            reached(nugget.radius)
                        } else {
                            Vec::new()
                        };
                        let area = area.into_iter().map(|(_, entity)| entity).filter(|entity| {
                            Some(*entity) != primary && judge.judge(index, *entity, false).is_some()
                        });
                        for entity in primary_hit.chain(area) {
                            result.paralyses.push((entity, index));
                        }
                    }
                    Some(
                        EffectNugget::HordeAttack(_)
                        | EffectNugget::SpawnAndFade(_)
                        | EffectNugget::DamageContained(_),
                    )
                    | None => {
                        result.unmodelled.push(index);
                    }
                }
            }
        }
    }
    result
}

pub(crate) fn launch_point(position: Vec2, feet: f32) -> Point3 {
    Point3::new(position.x, position.y, feet)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShotEvent {
    Fired,
    WindUpStarted,
    Nothing,
}

impl ShotEvent {
    pub(crate) const fn fired(self) -> bool {
        matches!(self, Self::Fired)
    }
}

pub(crate) fn tick_weapon(
    combat: &CombatData,
    armed: &mut Armed,
    now: Frame,
    new_target: bool,
    at: Vec2,
    rate: f32,
    conditions: mint_sage_combat::BonusConditions,
    rng: &mut mint_sim::Pcg32,
) -> ShotEvent {
    let Some(template) = combat.weapons.get(armed.weapon) else {
        return ShotEvent::Nothing;
    };
    let bonus = mint_sage_combat::compute_bonus(&combat.weapon_bonus, &template.bonus, conditions);
    let rate = rate * bonus.rate_of_fire();
    if new_target {
        armed.winding = false;
        armed.shots_at_target = 0;
    }
    match armed.state.status(now) {
        WeaponStatus::ReadyToFire => {
            let first_shot = armed.shots_at_target == 0;
            let moved = armed.last_shot_at != Some(at);
            if !armed.winding {
                armed.state.start_cycle(now);
                if template.winds_up() {
                    armed.state.begin_attack(
                        template,
                        now,
                        first_shot,
                        moved,
                        bonus.pre_attack(),
                        rng,
                    );
                    armed.winding = true;
                    if armed.state.status(now) == WeaponStatus::PreAttack {
                        return ShotEvent::WindUpStarted;
                    }
                }
            }
            armed.winding = false;
            armed.shots_at_target = armed.shots_at_target.saturating_add(1);
            armed.last_shot_at = Some(at);
            let entry = armed.state.cycle_started_at();
            let contact = entry
                .checked_add(template.fit_wind_up(&armed.state, moved, bonus.pre_attack()))
                .unwrap_or(Frame::FOREVER);
            armed.state.fire_at_rate(template, now, rate, rng);
            crate::diag::diag!(
                "swing: weapon {} blow={} entry={} contact={} offset={}",
                armed.weapon.index(),
                now.get(),
                entry.get(),
                contact.get(),
                i64::try_from(now.get()).unwrap_or(i64::MAX)
                    - i64::try_from(contact.get()).unwrap_or(i64::MAX),
            );
            ShotEvent::Fired
        }
        WeaponStatus::PreAttack
        | WeaponStatus::BetweenFiringShots
        | WeaponStatus::ReloadingClip
        | WeaponStatus::OutOfAmmo => ShotEvent::Nothing,
    }
}

pub(crate) fn armour_of<'a>(
    combat: &'a CombatData,
    armour: Option<Armour>,
    fallback: &'a ArmorTemplate,
) -> &'a ArmorTemplate {
    armour
        .and_then(|id| combat.armors.get(id.0))
        .unwrap_or(fallback)
}

pub(crate) fn slow_death_options() -> [SlowDeathOption; 2] {
    [
        SlowDeathOption {
            probability_modifier: 3,
            modifier_bonus_per_overkill_percent: 0,
            destruction_delay: DEFAULT_DESTRUCTION_DELAY,
            ..SlowDeathOption::default()
        },
        SlowDeathOption {
            probability_modifier: 1,
            modifier_bonus_per_overkill_percent: 4,
            destruction_delay: SPECTACULAR_DESTRUCTION_DELAY,
            ..SlowDeathOption::default()
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
