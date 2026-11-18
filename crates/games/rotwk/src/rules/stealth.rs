use mint_sage_combat::victim::Relation;
use mint_sage_modules::effects::Effect;
use mint_sage_modules::{
    InvisibilityType, InvisibilityUpdate, StealthCondition, StealthDetectorUpdate, StealthUpdate,
};
use mint_sim::module::UpgradeMask;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::diag::diag;
use crate::unit::Kind;

const HIDDEN_CONDITION: &str = "INVISIBLE_STEALTH";

const DEFAULT_DETECTION_MULTIPLIER: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HiderSource {
    Stealth,
    Invisibility,
}

#[derive(Debug, Clone)]
pub(crate) struct Hiding {
    forbidden: Vec<StealthCondition>,
    source: HiderSource,
    period: LogicFrames,
    proximity_range: Option<f32>,
    reveal_distance_from_target: Option<f32>,
    starts_active: bool,
    required_upgrades: UpgradeMask,
    forbidden_upgrades: UpgradeMask,
    allow_near_trees: bool,
    token: &'static str,
    detected_enemy: Option<String>,
    detected_ally: Option<String>,
    detected_owner: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StealthKind {
    pub(crate) hiding: Hiding,
    unmodelled: Option<StealthCondition>,
}

#[derive(Debug, Clone)]
pub(crate) struct Detector {
    module: StealthDetectorUpdate,
    range: f32,
    tag: Option<String>,
}

impl Hiding {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.forbidden.len());
        for condition in &self.forbidden {
            hasher.write_u8(*condition as u8);
        }
        hasher.write_u8(match self.source {
            HiderSource::Stealth => 0,
            HiderSource::Invisibility => 1,
        });
        hasher.write_u64(u64::from(self.period.get()));
        hasher.write_f32(self.proximity_range.unwrap_or(-1.0));
        hasher.write_f32(self.reveal_distance_from_target.unwrap_or(-1.0));
        hasher.write_bool(self.starts_active);
        self.required_upgrades.hash_into(hasher);
        self.forbidden_upgrades.hash_into(hasher);
        hasher.write_bool(self.allow_near_trees);
        hasher.write_bytes(self.token.as_bytes());
        for name in [
            &self.detected_enemy,
            &self.detected_ally,
            &self.detected_owner,
        ] {
            hasher.write_bytes(name.as_deref().unwrap_or("").as_bytes());
        }
    }

    fn from_nugget(nugget: &mint_sage_modules::InvisibilityNugget, period: LogicFrames) -> Self {
        Self {
            forbidden: nugget.forbidden.clone(),
            source: HiderSource::Invisibility,
            period,
            proximity_range: match nugget.kind {
                InvisibilityType::Camouflage => nugget.detection_range.filter(|r| *r > 0.0),
                InvisibilityType::Stealth => None,
            },
            reveal_distance_from_target: None,
            starts_active: false,
            required_upgrades: UpgradeMask::default(),
            forbidden_upgrades: UpgradeMask::default(),
            allow_near_trees: nugget.has(mint_sage_modules::InvisibilityOption::AllowNearTrees),
            token: nugget.kind.token(),
            detected_enemy: None,
            detected_ally: None,
            detected_owner: None,
        }
    }

    fn from_invisibility(module: &InvisibilityUpdate) -> Self {
        let mut hiding = Self::from_nugget(module.nugget(), module.update_period());
        hiding.starts_active = module.starts_active();
        hiding.required_upgrades = module.required_upgrades().clone();
        hiding.forbidden_upgrades = module.forbidden_upgrades().clone();
        hiding.detected_enemy = module.detected_enemy_event().map(str::to_owned);
        hiding.detected_ally = module.detected_ally_event().map(str::to_owned);
        hiding.detected_owner = module.detected_owner_event().map(str::to_owned);
        hiding
    }

    fn from_stealth(module: &StealthUpdate) -> Self {
        Self {
            forbidden: module.forbidden().to_vec(),
            source: HiderSource::Stealth,
            period: module.stealth_delay(),
            proximity_range: module.detected_by_anyone_range(),
            reveal_distance_from_target: module.reveal_distance_from_target(),
            starts_active: module.starts_active() && !module.disguises_as_team(),
            required_upgrades: module.required_upgrades().clone(),
            forbidden_upgrades: UpgradeMask::default(),
            allow_near_trees: false,
            token: InvisibilityType::Stealth.token(),
            detected_enemy: None,
            detected_ally: None,
            detected_owner: None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct StealthTables {
    kinds: Vec<Vec<StealthKind>>,
    detectors: Vec<Vec<Detector>>,
    widest_multiplier: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Stealth {
    pub(crate) hidden: bool,
    declaration: u8,
    allowed_since: Option<Frame>,
    revealed: u16,
    marked: u16,
    marked_until: Frame,
    activated_until: Frame,
    timer: Option<(Frame, Frame)>,
    detectors_flipped: u16,
}

impl Stealth {
    pub(crate) fn detected_at(&self, now: Frame) -> bool {
        self.marked != 0 && self.marked_until > now
    }

    fn revealed_to(self, player: PlayerId) -> bool {
        slot_bit(player).is_some_and(|bit| (self.revealed | self.marked) & bit != 0)
    }
}

impl Component for Stealth {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.hidden);
        hasher.write_u8(self.declaration);
        hasher.write_u64(self.allowed_since.map_or(0, Frame::get));
        hasher.write_u16(self.revealed);
        hasher.write_u16(self.marked);
        hasher.write_u64(self.marked_until.get());
        hasher.write_u64(self.activated_until.get());
        let (from, until) = self.timer.unwrap_or((Frame::FOREVER, Frame::FOREVER));
        hasher.write_u64(from.get());
        hasher.write_u64(until.get());
        hasher.write_u16(self.detectors_flipped);
    }
}

fn slot_bit_at(slot: usize) -> Option<u16> {
    u32::try_from(slot)
        .ok()
        .filter(|index| *index < 16)
        .and_then(|index| 1u16.checked_shl(index))
}

fn slot_bit(player: PlayerId) -> Option<u16> {
    let index = u32::from(player.get());
    (index < 16).then(|| 1u16.checked_shl(index).unwrap_or(0))
}

#[derive(Debug, Clone)]
pub(crate) struct GrantedHiding {
    hiding: Hiding,
    until: Frame,
}

impl Component for GrantedHiding {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.hiding.hash_into(hasher);
        hasher.write_u64(self.until.get());
    }
}

impl RotwkRules {
    pub(super) fn setup_stealth(&mut self) {
        let bits = std::sync::Arc::clone(&self.upgrade_bits);
        let mut widest = DEFAULT_DETECTION_MULTIPLIER;
        let mut unmodelled: Vec<(String, StealthCondition)> = Vec::new();
        let mut inactive = 0usize;
        let kinds: Vec<Vec<StealthKind>> = (0..self.kinds.len())
            .map(|index| {
                let Some(entry) = u16::try_from(index)
                    .ok()
                    .and_then(|index| self.kinds.get(Kind(index)))
                else {
                    return Vec::new();
                };
                if let Some(multiplier) = entry.record.camouflage_detection_multiplier {
                    if multiplier.is_finite() && multiplier > widest {
                        widest = multiplier;
                    }
                }
                entry
                    .record
                    .declared_modules
                    .iter()
                    .filter_map(|module| {
                        let source = module.source(&bits);
                        let hiding = if module.name.eq_ignore_ascii_case("InvisibilityUpdate") {
                            Hiding::from_invisibility(
                                &mint_sage_modules::invisibility_update_from_fields(
                                    &source,
                                    Frame::ZERO,
                                )
                                .ok()?,
                            )
                        } else if module.name.eq_ignore_ascii_case("StealthUpdate") {
                            Hiding::from_stealth(
                                &mint_sage_modules::stealth_update_from_fields(
                                    &source,
                                    Frame::ZERO,
                                )
                                .ok()?,
                            )
                        } else {
                            return None;
                        };
                        let missing = hiding
                            .forbidden
                            .iter()
                            .copied()
                            .find(|condition| !modelled(*condition));
                        if let Some(condition) = missing {
                            unmodelled.push((entry.record.name.clone(), condition));
                        }
                        if !hiding.starts_active {
                            inactive = inactive.saturating_add(1);
                        }
                        Some(StealthKind {
                            hiding,
                            unmodelled: missing,
                        })
                    })
                    .collect()
            })
            .collect();
        let declared: usize = kinds.iter().map(Vec::len).sum();
        let templates = kinds.iter().filter(|declared| !declared.is_empty()).count();
        println!(
            "stealth: {declared} hiding declaration(s) over {templates} template(s), \
             {inactive} of them inactive until an ability turns them on"
        );
        for (name, condition) in unmodelled {
            println!(
                "stealth: {name} forbids hiding on {}, which mint cannot answer - it never hides",
                condition.token()
            );
        }
        let detectors = self.read_detectors(&bits);
        self.stealth = StealthTables {
            kinds,
            detectors,
            widest_multiplier: widest,
        };
    }

    fn read_detectors(&self, bits: &mint_sage_modules::UpgradeBits) -> Vec<Vec<Detector>> {
        let mut declared = 0usize;
        let table: Vec<Vec<Detector>> = (0..self.kinds.len())
            .map(|index| {
                let Some(entry) = u16::try_from(index)
                    .ok()
                    .and_then(|index| self.kinds.get(Kind(index)))
                else {
                    return Vec::new();
                };
                entry
                    .record
                    .declared_modules
                    .iter()
                    .filter(|module| module.name.eq_ignore_ascii_case("StealthDetectorUpdate"))
                    .filter_map(|module| {
                        let read = mint_sage_modules::stealth_detector_update_from_fields(
                            &module.source(bits),
                            Frame::ZERO,
                        )
                        .ok()?;
                        let range = read
                            .detection_range()
                            .or(entry.record.vision_range)
                            .filter(|range| range.is_finite() && *range > 0.0)
                            .unwrap_or(0.0);
                        declared = declared.saturating_add(1);
                        Some(Detector {
                            module: read,
                            range,
                            tag: module.tag.clone(),
                        })
                    })
                    .collect()
            })
            .collect();
        let templates = table.iter().filter(|list| !list.is_empty()).count();
        println!(
            "stealth: {declared} StealthDetectorUpdate declaration(s) over {templates} template(s)"
        );
        table
    }

    pub(crate) fn sees(&self, world: &World, ids: Ids, player: PlayerId, entity: Entity) -> bool {
        self.not_hidden_from(world, ids, player, entity)
    }

    pub(crate) fn sees_in_scan(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> bool {
        self.sees(world, ids, player, entity) && self.out_of_fog(world, ids, player, entity)
    }

    fn out_of_fog(&self, world: &World, ids: Ids, player: PlayerId, entity: Entity) -> bool {
        if self.computer_controlled(player) {
            return true;
        }
        world
            .get(ids.seen, entity)
            .is_none_or(|seen| seen.visibility(player) == mint_sage_shroud::ObjectVisibility::Clear)
    }

    pub(super) fn not_hidden_from(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> bool {
        !self.hidden_or_horde_hidden(world, ids, player, entity)
            || world
                .get(ids.owner, entity)
                .is_none_or(|owner| owner.0 == player)
            || (self.computer_controlled(player) && attacking_recently(world, ids, entity))
    }

    fn hidden_or_horde_hidden(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> bool {
        let horde = world.get(ids.member, entity).map(|member| member.horde);
        let hidden_by = |carrier: Entity| {
            world
                .get(ids.stealth, carrier)
                .filter(|stealth| stealth.hidden && !stealth.revealed_to(player))
                .is_some()
        };
        hidden_by(entity) || horde.is_some_and(hidden_by)
    }

    pub(crate) fn stealth_look_hides(
        &self,
        world: &World,
        ids: Ids,
        viewer: PlayerId,
        entity: Entity,
    ) -> bool {
        self.hidden_or_horde_hidden(world, ids, viewer, entity)
            && world.contains(ids.owner, entity)
            && self.view_relation(world, viewer, entity).victim() != Relation::Allies
    }

    pub(super) fn seen_victim(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        foe: &Foe,
    ) -> mint_sage_combat::Victim {
        mint_sage_combat::Victim {
            seen: self.sees(world, ids, player, foe.entity),
            ..crate::combat::combat_victim(foe)
        }
    }

    pub(super) fn scanned_victim(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        foe: &Foe,
    ) -> mint_sage_combat::Victim {
        mint_sage_combat::Victim {
            seen: self.sees_in_scan(world, ids, player, foe.entity),
            ..crate::combat::combat_victim(foe)
        }
    }

    pub(super) fn is_hidden(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.stealth, entity)
            .is_some_and(|stealth| stealth.hidden)
    }

    pub(super) fn hidden_undetected(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.stealth, entity)
            .is_some_and(|stealth| stealth.hidden && stealth.marked == 0)
    }

    pub(crate) fn detected_event_names(
        &self,
        world: &World,
        entity: Entity,
    ) -> (Option<&str>, Option<&str>, Option<&str>) {
        let none = (None, None, None);
        let Some(ids) = self.ids else { return none };
        let Some(kind) = world.get(ids.kind, entity).copied() else {
            return none;
        };
        let Some(state) = world.get(ids.stealth, entity).copied() else {
            return none;
        };
        let Some(stealth_kind) = self
            .stealth
            .kinds
            .get(usize::from(kind.0))
            .and_then(|declared| declared.get(usize::from(state.declaration)))
        else {
            return none;
        };
        (
            stealth_kind.hiding.detected_owner.as_deref(),
            stealth_kind.hiding.detected_ally.as_deref(),
            stealth_kind.hiding.detected_enemy.as_deref(),
        )
    }

    pub(crate) fn stealthed_from(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> bool {
        self.hidden_from(world, ids, player, entity)
            && world
                .get(ids.owner, entity)
                .is_none_or(|owner| owner.0 != player)
    }

    pub(crate) fn hidden_from(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> bool {
        world
            .get(ids.stealth, entity)
            .is_some_and(|stealth| stealth.hidden && !stealth.revealed_to(player))
    }

    pub(super) fn advance_stealth(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        if self.stealth.kinds.is_empty() {
            return self.advance_granted_hiding(tick, ids, living, grid);
        }
        let now = tick.frame();
        let Some(store) = tick.state().store(ids.kind) else {
            return self.advance_granted_hiding(tick, ids, living, grid);
        };
        let carriers: Vec<(Entity, Kind)> = store
            .iter()
            .filter(|(_, kind)| {
                self.stealth
                    .kinds
                    .get(usize::from(kind.0))
                    .is_some_and(|declared| !declared.is_empty())
            })
            .map(|(entity, kind)| (entity, *kind))
            .collect();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for (entity, kind) in carriers {
            let Some(declared) = self.stealth.kinds.get(usize::from(kind.0)) else {
                continue;
            };
            let mut state = tick
                .state()
                .get(ids.stealth, entity)
                .copied()
                .unwrap_or(Stealth {
                    declaration: u8::MAX,
                    ..Stealth::default()
                });
            let activated = now < state.activated_until;
            let eligible = declared
                .iter()
                .position(|declaration| {
                    (declaration.hiding.starts_active || activated)
                        && self.hiding_allowed(tick, ids, entity, declaration)
                })
                .and_then(|at| u8::try_from(at).ok());
            let stealth_kind = eligible
                .and_then(|at| declared.get(usize::from(at)))
                .or_else(|| declared.get(usize::from(state.declaration)));
            match eligible {
                Some(at) => {
                    if state.declaration != at || state.allowed_since.is_none() {
                        state.allowed_since = Some(now);
                        state.declaration = at;
                    }
                }
                None => {
                    state.allowed_since = None;
                    if state.hidden {
                        state.hidden = false;
                        state.revealed = 0;
                        diag!("stealth: {} revealed by its own conditions", entity.index());
                        effects.push((entity, conditions(false)));
                        self.emit_voice(
                            tick,
                            ids,
                            entity,
                            crate::voice::VoiceKind::StealthOff,
                            None,
                        );
                    }
                }
            }
            let Some(stealth_kind) = stealth_kind else {
                tick.state_mut().insert(ids.stealth, entity, state)?;
                continue;
            };
            if !state.hidden
                && eligible.is_some()
                && self.hide_is_due(entity, stealth_kind, state, now)
            {
                state.hidden = true;
                diag!(
                    "stealth: {} hidden ({})",
                    entity.index(),
                    stealth_kind.hiding.token
                );
                effects.push((entity, conditions(true)));
                self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::StealthOn, None);
            }
            if state.hidden {
                let was = state.revealed;
                let (revealed, detector) =
                    self.proximity_reveal(tick, ids, entity, stealth_kind, living, grid, was);
                state.revealed = revealed;
                if let Some(detector) = detector {
                    Self::note_combat_fact(tick, ids, |facts, now| {
                        facts.push_detection(now, entity, detector);
                    });
                }
                if self.at_its_own_victim(tick, ids, entity, stealth_kind) {
                    state.revealed = u16::MAX;
                }
            }
            tick.state_mut().insert(ids.stealth, entity, state)?;
        }
        self.apply_effects(tick, ids, effects)?;
        self.advance_granted_hiding(tick, ids, living, grid)
    }

    fn advance_granted_hiding(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let now = tick.frame();
        let Some(store) = tick.state().store(ids.granted_hiding) else {
            return Ok(());
        };
        let grants: Vec<(Entity, GrantedHiding)> = store
            .iter()
            .map(|(entity, grant)| (entity, grant.clone()))
            .collect();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        let mut expired: Vec<Entity> = Vec::new();
        for (entity, grant) in grants {
            let mut state = tick
                .state()
                .get(ids.stealth, entity)
                .copied()
                .unwrap_or_default();
            if now >= grant.until {
                expired.push(entity);
                if state.hidden {
                    state.hidden = false;
                    state.revealed = 0;
                    diag!(
                        "stealth: {} granted-hide revealed (expired)",
                        entity.index()
                    );
                    effects.push((entity, conditions(false)));
                    self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::StealthOff, None);
                }
                tick.state_mut().insert(ids.stealth, entity, state)?;
                continue;
            }
            let unmodelled = grant
                .hiding
                .forbidden
                .iter()
                .copied()
                .find(|condition| !modelled(*condition));
            let kind = StealthKind {
                hiding: grant.hiding,
                unmodelled,
            };
            let allowed = unmodelled.is_none() && self.hiding_allowed(tick, ids, entity, &kind);
            if allowed {
                if state.allowed_since.is_none() {
                    state.allowed_since = Some(now);
                }
            } else {
                state.allowed_since = None;
                if state.hidden {
                    state.hidden = false;
                    state.revealed = 0;
                    diag!(
                        "stealth: {} granted-hide revealed (not allowed)",
                        entity.index()
                    );
                    effects.push((entity, conditions(false)));
                    self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::StealthOff, None);
                }
            }
            if !state.hidden && allowed && self.hide_is_due(entity, &kind, state, now) {
                state.hidden = true;
                diag!("stealth: {} granted-hide hidden", entity.index());
                effects.push((entity, conditions(true)));
                self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::StealthOn, None);
            }
            if state.hidden {
                let was = state.revealed;
                let (revealed, detector) =
                    self.proximity_reveal(tick, ids, entity, &kind, living, grid, was);
                state.revealed = revealed;
                if let Some(detector) = detector {
                    Self::note_combat_fact(tick, ids, |facts, now| {
                        facts.push_detection(now, entity, detector);
                    });
                }
                if self.at_its_own_victim(tick, ids, entity, &kind) {
                    state.revealed = u16::MAX;
                }
            }
            tick.state_mut().insert(ids.stealth, entity, state)?;
        }
        for entity in expired {
            tick.state_mut().remove(ids.granted_hiding, entity);
        }
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn grant_invisibility(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        targets: &[Entity],
        duration: LogicFrames,
        nugget: &mint_sage_modules::InvisibilityNugget,
    ) -> Result<usize> {
        let now = tick.frame();
        let until = now.checked_add(duration).unwrap_or(Frame::FOREVER);
        let hiding = Hiding::from_nugget(nugget, LogicFrames::ZERO);
        let mut granted = 0usize;
        for &entity in targets {
            let has_own = tick.state().get(ids.kind, entity).is_some_and(|kind| {
                self.stealth
                    .kinds
                    .get(usize::from(kind.0))
                    .is_some_and(|declared| !declared.is_empty())
            });
            if has_own {
                continue;
            }
            tick.state_mut().insert(
                ids.granted_hiding,
                entity,
                GrantedHiding {
                    hiding: hiding.clone(),
                    until,
                },
            )?;
            granted = granted.saturating_add(1);
        }
        Ok(granted)
    }

    fn hiding_allowed(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: &StealthKind,
    ) -> bool {
        if kind.unmodelled.is_some()
            || self.script_status_bit(
                tick.state(),
                ids,
                entity,
                super::map_scripts::ScriptStatusBit::Unstealthed,
            )
        {
            return false;
        }
        let hiding = &kind.hiding;
        if !hiding.required_upgrades.is_empty() || !hiding.forbidden_upgrades.is_empty() {
            let held = self.effective_upgrades(tick, ids, entity);
            if !hiding.required_upgrades.is_empty() && !held.contains_all(&hiding.required_upgrades)
            {
                return false;
            }
            if held.intersects(&hiding.forbidden_upgrades) {
                return false;
            }
        }
        let allowed_by_other_conditions = hiding.forbidden.iter().all(|condition| {
            *condition == StealthCondition::AwayFromTrees
                || !self.condition_holds(tick, ids, entity, *condition, hiding.period)
        });
        let away_forbidden = hiding.forbidden.contains(&StealthCondition::AwayFromTrees);
        let needs_tree_test =
            (!allowed_by_other_conditions && hiding.allow_near_trees) || away_forbidden;
        let near = needs_tree_test && self.near_trees(tick, ids, entity);
        tree_gated_result(
            allowed_by_other_conditions,
            away_forbidden,
            hiding.allow_near_trees,
            near,
        )
    }

    fn near_trees(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        const NEAR_TREES_RANGE: f32 = 50.0;
        let Some(pos) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return false;
        };
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return false;
        };
        tick.state()
            .get(ids.forest, forest_entity)
            .is_some_and(|forest| {
                forest
                    .find_closest(pos, NEAR_TREES_RANGE, true, crate::forest::TreeQuery::Any)
                    .is_some()
            })
    }

    fn condition_holds(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        condition: StealthCondition,
        period: mint_sim::LogicFrames,
    ) -> bool {
        match condition {
            StealthCondition::Moving => tick
                .state()
                .get(ids.body, entity)
                .is_some_and(|body| body.0.speed > 0.0),
            StealthCondition::Attacking | StealthCondition::FiringAny => {
                tick.state()
                    .get(ids.armed, entity)
                    .is_some_and(|armed| armed.target.is_some())
                    || tick.state().contains(ids.attack_order, entity)
            }
            StealthCondition::UsingAbility => tick.state().contains(ids.using_ability, entity),
            StealthCondition::TakingDamage => tick
                .state()
                .get(ids.last_hurt, entity)
                .is_some_and(|hurt| !hurt.quiet_for(period, tick.frame())),
            StealthCondition::AwayFromTrees => false,
            StealthCondition::HordebrainNotStealthed => true,
        }
    }

    fn hide_is_due(&self, entity: Entity, kind: &StealthKind, state: Stealth, now: Frame) -> bool {
        let period = kind.hiding.period;
        let Some(since) = state.allowed_since else {
            return false;
        };
        let due = since.checked_add(period).unwrap_or(now);
        if now < due {
            return false;
        }
        let span = u64::from(period.get().max(1));
        now.get()
            .wrapping_add(u64::from(entity.index()))
            .checked_rem(span)
            == Some(0)
    }

    fn proximity_reveal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: &StealthKind,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        was: u16,
    ) -> (u16, Option<Entity>) {
        let Some(range) = kind.hiding.proximity_range else {
            return (0, None);
        };
        let Some(body) = tick.state().get(ids.body, entity) else {
            return (0, None);
        };
        let owner = tick.state().get(ids.owner, entity).map(|owner| owner.0);
        let at = body.0.position;
        let widest = range * self.stealth.widest_multiplier;
        let mut candidates: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(at.x),
            grid_cell(at.y),
            grid_radius_cells(widest),
            &mut candidates,
        );
        let mut revealed = 0u16;
        let mut detector = None;
        for index in candidates {
            #[allow(clippy::cast_possible_truncation)]
            let index = index as usize;
            let Some((looker, player)) = living.get(index) else {
                continue;
            };
            if looker.entity == entity {
                continue;
            }
            if owner.is_none()
                || self.view_relation(tick.state(), *player, entity).victim() != Relation::Enemies
            {
                continue;
            }
            let Some(bit) = slot_bit(*player) else {
                continue;
            };
            if revealed & bit != 0 {
                continue;
            }
            let reach = range * self.detection_multiplier(tick, ids, looker.entity);
            let distance = looker.position.minus(at).length();
            if distance <= reach {
                revealed |= bit;
                if was & bit == 0 {
                    diag!(
                        "stealth: {} revealed by {} at {distance:.1}",
                        entity.index(),
                        looker.entity.index()
                    );
                }
                if was == 0 && detector.is_none() {
                    detector = Some(looker.entity);
                }
            }
        }
        (revealed, detector)
    }

    fn at_its_own_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: &StealthKind,
    ) -> bool {
        let Some(reach) = kind.hiding.reveal_distance_from_target else {
            return false;
        };
        let Some(victim) = tick
            .state()
            .get(ids.armed, entity)
            .and_then(|armed| armed.target)
        else {
            return false;
        };
        let Some(at) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return false;
        };
        tick.state()
            .get(ids.body, victim)
            .is_some_and(|body| body.0.position.minus(at).length() <= reach)
    }

    pub(super) fn advance_detectors(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let now = tick.frame();
        let lapsed: Vec<Entity> = tick
            .state()
            .store(ids.stealth)
            .map_or_else(Vec::new, |store| {
                store
                    .iter()
                    .filter(|(_, state)| state.marked != 0 && now >= state.marked_until)
                    .map(|(entity, _)| entity)
                    .collect()
            });
        for entity in lapsed {
            if let Some(state) = tick.state_mut().get_mut(ids.stealth, entity) {
                state.marked = 0;
            }
            diag!("detect: {}'s mark lapsed", entity.index());
            self.emit_voice(
                tick,
                ids,
                entity,
                crate::voice::VoiceKind::StealthBack,
                None,
            );
        }
        if self.stealth.detectors.is_empty() {
            return Ok(());
        }
        let Some(store) = tick.state().store(ids.kind) else {
            return Ok(());
        };
        let carriers: Vec<(Entity, Kind)> = store
            .iter()
            .filter(|(_, kind)| {
                self.stealth
                    .detectors
                    .get(usize::from(kind.0))
                    .is_some_and(|declared| !declared.is_empty())
            })
            .map(|(entity, kind)| (entity, *kind))
            .collect();
        let mut marks: Vec<(Entity, Entity, u16, LogicFrames)> = Vec::new();
        for (entity, kind) in carriers {
            let Some(declared) = self.stealth.detectors.get(usize::from(kind.0)) else {
                continue;
            };
            let Some(player) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
                continue;
            };
            let bit = self.allied_slots(player);
            let Some(at) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            for (slot, detector) in declared.iter().enumerate() {
                if !self.detector_scans_now(tick, ids, entity, detector, slot, now) {
                    continue;
                }
                let range = detector.range;
                let mut candidates: Vec<u32> = Vec::new();
                grid.query_into(
                    grid_cell(at.x),
                    grid_cell(at.y),
                    grid_radius_cells(range),
                    &mut candidates,
                );
                for index in candidates {
                    #[allow(clippy::cast_possible_truncation)]
                    let index = index as usize;
                    let Some((victim, _)) = living.get(index) else {
                        continue;
                    };
                    if victim.entity == entity {
                        continue;
                    }
                    let allied = tick.state().contains(ids.owner, victim.entity)
                        && self
                            .view_relation(tick.state(), player, victim.entity)
                            .victim()
                            == Relation::Allies;
                    if allied || victim.position.minus(at).length() > range {
                        continue;
                    }
                    let disguised = self.modules.as_ref().is_some_and(|wiring| {
                        tick.state()
                            .get(wiring.state, victim.entity)
                            .is_some_and(|state| state.has_model_condition("DISGUISED"))
                    });
                    if disguised && !tick.state().contains(ids.doomed, victim.entity) {
                        self.break_disguise(tick, ids, victim.entity, true)?;
                    }
                    if !tick
                        .state()
                        .get(ids.stealth, victim.entity)
                        .is_some_and(|state| state.hidden)
                    {
                        continue;
                    }
                    marks.push((entity, victim.entity, bit, detector.module.mark_frames()));
                }
            }
        }
        for (detector, victim, bit, frames) in marks {
            let mut state = tick
                .state()
                .get(ids.stealth, victim)
                .copied()
                .unwrap_or_default();
            let fresh = state.marked & bit != bit;
            let detected = state.marked == 0;
            state.marked |= bit;
            let until = now.checked_add(frames).unwrap_or(Frame::FOREVER);
            if until > state.marked_until {
                state.marked_until = until;
            }
            tick.state_mut().insert(ids.stealth, victim, state)?;
            if detected {
                self.emit_voice(tick, ids, victim, crate::voice::VoiceKind::StealthOff, None);
                Self::note_combat_fact(tick, ids, |facts, now| {
                    facts.push_detection(now, victim, detector);
                });
            }
            if fresh {
                diag!(
                    "detect: {} marked {} for {} frames",
                    detector.index(),
                    victim.index(),
                    frames.get()
                );
            }
        }
        Ok(())
    }

    pub(super) fn flip_detector(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        tag: &str,
    ) -> Result<bool> {
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(false);
        };
        let found = self
            .stealth
            .detectors
            .get(usize::from(kind.0))
            .and_then(|declared| {
                declared
                    .iter()
                    .enumerate()
                    .find(|(_, detector)| detector.tag.as_deref() == Some(tag))
            });
        let Some((bit, detector)) =
            found.and_then(|(slot, detector)| slot_bit_at(slot).map(|bit: u16| (bit, detector)))
        else {
            return Ok(false);
        };
        let mut state = tick
            .state()
            .get(ids.stealth, entity)
            .copied()
            .unwrap_or_default();
        state.detectors_flipped ^= bit;
        tick.state_mut().insert(ids.stealth, entity, state)?;
        let on = detector.module.initially_disabled() == (state.detectors_flipped & bit != 0);
        diag!(
            "detect: {}'s {tag} is now {}",
            entity.index(),
            if on { "on" } else { "off" }
        );
        Ok(true)
    }

    fn allied_slots(&self, player: PlayerId) -> u16 {
        (0..16u8)
            .map(PlayerId::new)
            .filter(|slot| self.alliances.relation(player, *slot) == Relation::Allies)
            .filter_map(slot_bit)
            .fold(0u16, |mask, bit| mask | bit)
    }

    fn detector_scans_now(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        detector: &Detector,
        slot: usize,
        now: Frame,
    ) -> bool {
        let module = &detector.module;
        let flipped = slot_bit_at(slot).is_some_and(|bit| {
            tick.state()
                .get(ids.stealth, entity)
                .is_some_and(|state| state.detectors_flipped & bit != 0)
        });
        let enabled = module.initially_disabled() == flipped;
        if !enabled || detector.range <= 0.0 {
            return false;
        }
        if !module.required_upgrade().is_empty()
            && !self
                .effective_upgrades(tick, ids, entity)
                .contains_all(module.required_upgrade())
        {
            return false;
        }
        if let Some(contained) = tick.state().get(ids.contained, entity) {
            let garrisoned = tick
                .state()
                .get(ids.kind, contained.container)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
            let allowed = if garrisoned {
                module.can_detect_while_garrisoned()
            } else {
                module.can_detect_while_contained()
            };
            if !allowed {
                return false;
            }
        }
        let rate = u64::from(module.detection_rate().get().max(1));
        now.get().checked_rem(rate) == u64::from(entity.index()).checked_rem(rate)
    }

    pub(super) fn toggle_stealth(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        lasts: Option<LogicFrames>,
        palantir_timer: bool,
    ) -> Result<()> {
        let now = tick.frame();
        let mut state = tick
            .state()
            .get(ids.stealth, entity)
            .copied()
            .unwrap_or(Stealth {
                declaration: u8::MAX,
                ..Stealth::default()
            });
        if now < state.activated_until {
            state.activated_until = Frame::ZERO;
            state.timer = None;
            diag!("stealth: {} cloak off", entity.index());
        } else {
            state.activated_until = lasts
                .and_then(|lasts| now.checked_add(lasts))
                .unwrap_or(Frame::FOREVER);
            state.timer = palantir_timer.then(|| {
                let until = now.checked_add(lasts.unwrap_or(LogicFrames::ZERO));
                (now, until.unwrap_or(Frame::FOREVER))
            });
            diag!(
                "stealth: {} cloak on until frame {}",
                entity.index(),
                state.activated_until.get()
            );
        }
        tick.state_mut().insert(ids.stealth, entity, state)?;
        Ok(())
    }

    pub(super) fn apply_lose_stealth_trigger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let now = tick.frame();
        let reveal_frames = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .zip(tick.state().get(ids.stealth, entity).copied())
            .and_then(|(kind, state)| {
                self.stealth
                    .kinds
                    .get(usize::from(kind.0))?
                    .get(usize::from(state.declaration))
            })
            .map_or(LogicFrames::ZERO, |declared| match declared.hiding.source {
                HiderSource::Stealth => declared.hiding.period,
                HiderSource::Invisibility => LogicFrames::ZERO,
            });
        let mut state = tick
            .state()
            .get(ids.stealth, entity)
            .copied()
            .unwrap_or_default();
        let until = now.checked_add(reveal_frames).unwrap_or(Frame::FOREVER);
        state.marked = u16::MAX;
        if until > state.marked_until {
            state.marked_until = until;
        }
        tick.state_mut().insert(ids.stealth, entity, state)?;
        diag!(
            "stealth: {} revealed by its own trigger until frame {}",
            entity.index(),
            state.marked_until.get()
        );
        Ok(())
    }

    pub(super) fn cloak_timer(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        now: Frame,
    ) -> Option<(Frame, Frame)> {
        let state = world.get(ids.stealth, entity)?;
        (now < state.activated_until)
            .then_some(state.timer)
            .flatten()
    }

    fn detection_multiplier(&self, tick: &Tick<'_>, ids: Ids, looker: Entity) -> f32 {
        tick.state()
            .get(ids.kind, looker)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.camouflage_detection_multiplier)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_DETECTION_MULTIPLIER)
    }
}

fn tree_gated_result(
    allowed_by_other_conditions: bool,
    away_forbidden: bool,
    allow_near_trees: bool,
    near: bool,
) -> bool {
    let rescue = !allowed_by_other_conditions && allow_near_trees;
    if rescue {
        return near;
    }
    if away_forbidden {
        return allowed_by_other_conditions && near;
    }
    allowed_by_other_conditions
}

fn modelled(condition: StealthCondition) -> bool {
    !matches!(condition, StealthCondition::HordebrainNotStealthed)
}

fn conditions(hiding: bool) -> Effect {
    let name = mint_sage_modules::effects::names(vec![HIDDEN_CONDITION.to_owned()]);
    let empty = mint_sage_modules::effects::names(Vec::new());
    if hiding {
        Effect::ModelConditions {
            add: name,
            remove: empty,
        }
    } else {
        Effect::ModelConditions {
            add: empty,
            remove: name,
        }
    }
}

fn attacking_recently(world: &World, ids: Ids, entity: Entity) -> bool {
    let Some(mut armed) = world.get(ids.armed, entity).copied() else {
        return false;
    };
    let now = world
        .store(ids.clock)
        .and_then(|store| store.iter().next().map(|(_, clock)| Frame::new(clock.0)))
        .unwrap_or(Frame::ZERO);
    armed.state.status(now) != mint_sage_combat::WeaponStatus::ReadyToFire
        || armed.state.last_shot().is_some_and(|shot| {
            shot.get() > 2
                && shot
                    .checked_add(super::death::frames_of_ms(2000.0))
                    .is_some_and(|until| until > now)
        })
}

#[cfg(test)]
mod tests {
    use super::{tree_gated_result, GrantedHiding, Hiding};
    use mint_sage_modules::{InvisibilityNugget, InvisibilityType};
    use mint_sim::{Component, Frame, LogicFrames, StateHasher};

    #[test]
    fn a_disguise_stealth_waits_for_its_disguise() {
        struct Fields(&'static str);
        impl mint_sim::ModuleFieldSource for Fields {
            fn field(&self, keyword: &str) -> Option<&str> {
                keyword
                    .eq_ignore_ascii_case("DisguisesAsTeam")
                    .then_some(self.0)
            }
            fn field_args(&self, keyword: &str) -> Vec<&str> {
                self.field(keyword).into_iter().collect()
            }
        }
        let starts = |disguises| {
            let module =
                mint_sage_modules::stealth_update_from_fields(&Fields(disguises), Frame::ZERO)
                    .unwrap();
            Hiding::from_stealth(&module).starts_active
        };
        assert!(!starts("Yes"));
        assert!(starts("No"));
    }

    fn hash_of(grant: &GrantedHiding) -> u64 {
        let mut hasher = StateHasher::new();
        grant.hash_into(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn granted_hidings_hiding_shape_is_part_of_the_hash() {
        let narrow = InvisibilityNugget {
            kind: InvisibilityType::Camouflage,
            forbidden: Vec::new(),
            forbidden_weapon_conditions: Vec::new(),
            detection_range: Some(40.0),
            options: Vec::new(),
            become_stealthed_fx: None,
            exit_stealth_fx: None,
            hint_detectable: Vec::new(),
        };
        let wide = InvisibilityNugget {
            detection_range: Some(400.0),
            ..narrow.clone()
        };
        let a = GrantedHiding {
            hiding: Hiding::from_nugget(&narrow, LogicFrames::ZERO),
            until: Frame::new(100),
        };
        let b = GrantedHiding {
            hiding: Hiding::from_nugget(&wide, LogicFrames::ZERO),
            until: Frame::new(100),
        };
        assert_ne!(
            hash_of(&a),
            hash_of(&b),
            "a different DetectionRange must change the hash"
        );
    }

    #[test]
    fn away_from_trees_only_allows_hiding_near_a_tree() {
        assert!(!tree_gated_result(true, true, false, false));
        assert!(tree_gated_result(true, true, false, true));
    }

    #[test]
    fn allow_near_trees_rescues_an_otherwise_forbidding_condition() {
        assert!(!tree_gated_result(false, false, true, false));
        assert!(tree_gated_result(false, false, true, true));
    }

    #[test]
    fn ordinary_conditions_ignore_trees_entirely() {
        assert!(tree_gated_result(true, false, false, false));
        assert!(!tree_gated_result(false, false, false, true));
    }
}
