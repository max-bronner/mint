use mint_movement::{Facing, Vec2};
use mint_sage_combat::{BodyKind, HealthPool};
use mint_sim::{Component, Entity, Frame, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::Health;
use crate::diag::diag;
use crate::horde::HordeSpawn;
use crate::unit::Kind;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DebugCommand {
    Spawn {
        kind: u16,
        x: f32,
        y: f32,
        facing: f32,
        role: SpawnRole,
    },
    LevelUp {
        quarters: u8,
    },
    ResetPowers,
    Damage {
        percent: u8,
    },
    DummiesInvincible {
        on: bool,
    },
    SetStatus {
        status: String,
    },
    GrantUpgrade {
        upgrade: String,
    },
    FireNamedPower {
        power: String,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    },
    SetScriptFlag {
        name: String,
        value: bool,
    },
    SetScriptCounter {
        name: String,
        value: i32,
    },
    SetScriptTimer {
        name: String,
        frames: i32,
    },
    Kill {
        target: DebugTarget,
    },
    Indestructible {
        target: DebugTarget,
        on: bool,
    },
    Complete {
        target: DebugTarget,
    },
    Capture {
        target: DebugTarget,
    },
    Upgrade {
        target: DebugTarget,
        upgrade: String,
    },
    Teleport {
        target: DebugTarget,
        x: f32,
        y: f32,
    },
    KillBy {
        target: DebugTarget,
        killer: DebugTarget,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DebugTarget {
    Named(String),
    Team(String),
    Near { kind: u16, x: f32, y: f32 },
    All,
    Player(String),
    Kind { kind: u16, side: Option<String> },
}

impl DebugTarget {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Self::Named(name) => {
                hasher.write_u8(0);
                hasher.write_bytes(name.as_bytes());
            }
            Self::Team(name) => {
                hasher.write_u8(1);
                hasher.write_bytes(name.as_bytes());
            }
            Self::Near { kind, x, y } => {
                hasher.write_u8(2);
                hasher.write_u16(*kind);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
            }
            Self::All => hasher.write_u8(3),
            Self::Player(side) => {
                hasher.write_u8(4);
                hasher.write_bytes(side.as_bytes());
            }
            Self::Kind { kind, side } => {
                hasher.write_u8(5);
                hasher.write_u16(*kind);
                hasher.write_bytes(side.as_deref().unwrap_or("").as_bytes());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpawnRole {
    Normal,
    Dummy,
    FightingDummy,
}

impl SpawnRole {
    pub(crate) fn is_dummy(self) -> bool {
        self != Self::Normal
    }
}

impl DebugCommand {
    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        match self {
            Self::Spawn {
                kind,
                x,
                y,
                facing,
                role,
            } => {
                hasher.write_u8(0);
                hasher.write_u16(*kind);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
                hasher.write_f32(*facing);
                hasher.write_u8(match role {
                    SpawnRole::Normal => 0,
                    SpawnRole::Dummy => 1,
                    SpawnRole::FightingDummy => 2,
                });
            }
            Self::LevelUp { quarters } => {
                hasher.write_u8(1);
                hasher.write_u8(*quarters);
            }
            Self::ResetPowers => hasher.write_u8(2),
            Self::Damage { percent } => {
                hasher.write_u8(3);
                hasher.write_u8(*percent);
            }
            Self::DummiesInvincible { on } => {
                hasher.write_u8(4);
                hasher.write_bool(*on);
            }
            Self::SetStatus { status } => {
                hasher.write_u8(5);
                hasher.write_bytes(status.as_bytes());
            }
            Self::GrantUpgrade { upgrade } => {
                hasher.write_u8(6);
                hasher.write_bytes(upgrade.as_bytes());
            }
            Self::SetScriptFlag { name, value } => {
                hasher.write_u8(8);
                hasher.write_bytes(name.as_bytes());
                hasher.write_bool(*value);
            }
            Self::SetScriptCounter { name, value } => {
                hasher.write_u8(9);
                hasher.write_bytes(name.as_bytes());
                hasher.write_i32(*value);
            }
            Self::SetScriptTimer { name, frames } => {
                hasher.write_u8(10);
                hasher.write_bytes(name.as_bytes());
                hasher.write_i32(*frames);
            }
            Self::Kill { target } => {
                hasher.write_u8(11);
                target.hash_into(hasher);
            }
            Self::Indestructible { target, on } => {
                hasher.write_u8(12);
                target.hash_into(hasher);
                hasher.write_bool(*on);
            }
            Self::Complete { target } => {
                hasher.write_u8(14);
                target.hash_into(hasher);
            }
            Self::Capture { target } => {
                hasher.write_u8(15);
                target.hash_into(hasher);
            }
            Self::Upgrade { target, upgrade } => {
                hasher.write_u8(16);
                target.hash_into(hasher);
                hasher.write_bytes(upgrade.as_bytes());
            }
            Self::Teleport { target, x, y } => {
                hasher.write_u8(13);
                target.hash_into(hasher);
                hasher.write_f32(*x);
                hasher.write_f32(*y);
            }
            Self::FireNamedPower { power, target, at } => {
                hasher.write_u8(7);
                hasher.write_bytes(power.as_bytes());
                hasher.write_u32(target.map_or(u32::MAX, Entity::index));
                let (x, y) = at.unwrap_or((f32::NAN, f32::NAN));
                hasher.write_f32(x);
                hasher.write_f32(y);
            }
            Self::KillBy { target, killer } => {
                hasher.write_u8(17);
                target.hash_into(hasher);
                killer.hash_into(hasher);
            }
        }
    }
}

pub(crate) fn quarter_gain(floor: u32, required: u32, points: u32, quarters: u8) -> u32 {
    let span = u64::from(required.saturating_sub(floor));
    let mark = |k: u64| u64::from(floor).saturating_add(span.saturating_mul(k).div_ceil(4));
    let next = (1..=4).find(|k| mark(*k) > u64::from(points)).unwrap_or(4);
    let target = next
        .saturating_add(u64::from(quarters))
        .saturating_sub(1)
        .min(4);
    u32::try_from(mark(target))
        .unwrap_or(required)
        .saturating_sub(points)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoAttack;

impl Component for NoAttack {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dummy;

impl Component for Dummy {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone)]
pub(crate) struct UnitEntry {
    pub(crate) kind: Kind,
    pub(crate) label: String,
    pub(crate) side: Option<String>,
    pub(crate) listed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LevelStep {
    pub(crate) holder: Entity,
    pub(crate) rank: u16,
    pub(crate) points: u32,
    pub(crate) floor: u32,
    pub(crate) next: Option<(u16, u32)>,
}

impl RotwkRules {
    pub(super) fn apply_debug(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        command: &DebugCommand,
    ) -> Result<()> {
        match command {
            DebugCommand::Spawn {
                kind,
                x,
                y,
                facing,
                role,
            } => self.debug_spawn(
                tick,
                ids,
                player,
                Kind(*kind),
                Vec2::new(*x, *y),
                Facing::from_radians(*facing),
                *role,
            ),
            DebugCommand::LevelUp { quarters } => {
                for step in self.level_steps(tick.state(), player) {
                    let Some((_, required)) = step.next else {
                        continue;
                    };
                    let gain = quarter_gain(step.floor, required, step.points, *quarters);
                    self.add_experience(tick, ids, step.holder, gain)?;
                }
                Ok(())
            }
            DebugCommand::ResetPowers => self.reset_powers(tick, ids, player),
            DebugCommand::Damage { percent } => {
                let mut killed: Vec<(Entity, f32)> = Vec::new();
                for entity in self.selection(tick, ids, player) {
                    let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
                        continue;
                    };
                    let left = health.0.current() - health.0.max() * f32::from(*percent) / 100.0;
                    health.0.set_current(left.max(0.0));
                    tick.state_mut().insert(ids.health, entity, health)?;
                    self.note_hurt(tick, ids, entity);
                    if left <= 0.0 {
                        killed.push((entity, 0.0));
                    }
                }
                self.resolve_deaths(tick, ids, &killed)
            }
            DebugCommand::Kill { target } => {
                let targets = self.debug_targets(tick, ids, player, target);
                let mut killed: Vec<(Entity, f32)> = Vec::new();
                for entity in self.selection_with_hordes(tick, ids, &targets) {
                    let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
                        continue;
                    };
                    if health.0.is_dead() || killed.iter().any(|(e, _)| *e == entity) {
                        continue;
                    }
                    health.0.set_current(0.0);
                    tick.state_mut().insert(ids.health, entity, health)?;
                    killed.push((entity, 0.0));
                }
                diag!("debug: kill {target:?} - {} object(s)", killed.len());
                self.resolve_deaths(tick, ids, &killed)
            }
            DebugCommand::Indestructible { target, on } => {
                let targets = self.debug_targets(tick, ids, player, target);
                for &entity in &targets {
                    Self::set_indestructible_with_contents(tick, ids, entity, *on, 0)?;
                }
                diag!(
                    "debug: indestructible {on} for {target:?} - {} object(s)",
                    targets.len()
                );
                Ok(())
            }
            DebugCommand::Complete { target } => {
                let mut done = 0usize;
                for site in self.debug_targets(tick, ids, player, target) {
                    if !tick.state().contains(ids.construction, site) {
                        continue;
                    }
                    let Some(kind) = tick.state().get(ids.kind, site).copied() else {
                        continue;
                    };
                    let Some(record) = self.kinds.get(kind).map(|entry| entry.record.clone())
                    else {
                        continue;
                    };
                    if let Some(mut health) = tick.state().get(ids.health, site).copied() {
                        health.0.set_current(health.0.max());
                        tick.state_mut().insert(ids.health, site, health)?;
                    }
                    self.complete_construction(tick, ids, site, kind, &record)?;
                    done = done.saturating_add(1);
                }
                diag!("debug: complete {target:?} - {done} site(s)");
                Ok(())
            }
            DebugCommand::Capture { target } => {
                let mut taken = 0usize;
                for entity in self.debug_targets(tick, ids, player, target) {
                    if self.change_owner(tick, ids, entity, player)? {
                        taken = taken.saturating_add(1);
                    }
                }
                diag!("debug: capture {target:?} - {taken} object(s)");
                Ok(())
            }
            DebugCommand::Upgrade { target, upgrade } => {
                let Some(bit) = self.upgrade_bits.bit(upgrade) else {
                    diag!("upgrade: {upgrade} is not an Upgrade - nothing granted");
                    return Ok(());
                };
                if self.upgrade_bits.is_player(bit) {
                    return self.grant_player_upgrade(tick, ids, player, bit);
                }
                for entity in self.debug_targets(tick, ids, player, target) {
                    self.grant_object_upgrade(tick, ids, entity, bit)?;
                }
                Ok(())
            }
            DebugCommand::Teleport { target, x, y } => {
                let targets = self.debug_targets(tick, ids, player, target);
                for &entity in &targets {
                    self.script_teleport(tick, ids, entity, Vec2::new(*x, *y))?;
                }
                diag!(
                    "debug: teleport {target:?} to ({x}, {y}) - {} object(s)",
                    targets.len()
                );
                Ok(())
            }
            DebugCommand::DummiesInvincible { on } => {
                let kind = if *on {
                    BodyKind::Immortal
                } else {
                    BodyKind::Active
                };
                for (entity, health) in Self::live_dummies(tick.state(), ids) {
                    if let Some(mut pool) = HealthPool::new(health.0.max(), kind) {
                        pool.set_current(health.0.current());
                        tick.state_mut().insert(ids.health, entity, Health(pool))?;
                    }
                }
                Ok(())
            }
            DebugCommand::SetStatus { status } => {
                let set: mint_sage_modules::effects::Names =
                    vec![std::sync::Arc::<str>::from(status.as_str())].into();
                let entries = self
                    .selection(tick, ids, player)
                    .into_iter()
                    .map(|entity| {
                        (
                            entity,
                            mint_sage_modules::effects::Effect::StatusBits {
                                set: set.clone(),
                                clear: Vec::new().into(),
                            },
                        )
                    })
                    .collect();
                self.apply_effects(tick, ids, entries)
            }
            DebugCommand::SetScriptFlag { name, value } => self.debug_set_script_value(
                tick,
                ids,
                name,
                super::map_scripts::DebugScriptValue::Flag(*value),
            ),
            DebugCommand::SetScriptCounter { name, value } => self.debug_set_script_value(
                tick,
                ids,
                name,
                super::map_scripts::DebugScriptValue::Counter(*value),
            ),
            DebugCommand::SetScriptTimer { name, frames } => self.debug_set_script_value(
                tick,
                ids,
                name,
                super::map_scripts::DebugScriptValue::Timer(*frames),
            ),
            DebugCommand::GrantUpgrade { upgrade } => {
                let Some(bit) = self.upgrade_bits.bit(upgrade) else {
                    diag!("upgrade: {upgrade} is not an Upgrade - nothing granted");
                    return Ok(());
                };
                if self.upgrade_bits.is_player(bit) {
                    self.grant_player_upgrade(tick, ids, player, bit)?;
                    return Ok(());
                }
                for entity in self.selection(tick, ids, player) {
                    self.grant_object_upgrade(tick, ids, entity, bit)?;
                }
                Ok(())
            }
            DebugCommand::FireNamedPower { power, target, at } => {
                for entity in self.selection(tick, ids, player) {
                    let target_kind = match (*target, *at) {
                        (Some(_), None) => crate::templates::TargetKind::Object,
                        (None, Some(_)) => crate::templates::TargetKind::Position,
                        _ => crate::templates::TargetKind::None,
                    };
                    self.fire_power_from(
                        tick,
                        ids,
                        player,
                        entity,
                        power,
                        target_kind,
                        &crate::defeat::ObjectFilter::default(),
                        *target,
                        *at,
                        false,
                    )?;
                }
                Ok(())
            }
            DebugCommand::KillBy { target, killer } => {
                let Some(&by) = self.debug_targets(tick, ids, player, killer).first() else {
                    diag!("debug: kill {target:?} by {killer:?} - no killer");
                    return Ok(());
                };
                let targets = self.debug_targets(tick, ids, player, target);
                let mut killed: Vec<(Entity, f32)> = Vec::new();
                let mut kills: Vec<(Entity, Entity)> = Vec::new();
                for entity in self.selection_with_hordes(tick, ids, &targets) {
                    let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
                        continue;
                    };
                    if health.0.is_dead() || killed.iter().any(|(e, _)| *e == entity) {
                        continue;
                    }
                    let dealt = health.0.current();
                    health.0.set_current(0.0);
                    tick.state_mut().insert(ids.health, entity, health)?;
                    tick.state_mut().insert(
                        ids.killing_blow,
                        entity,
                        super::death::KillingBlow {
                            killer: Some(by),
                            damage_type: None,
                            dealt,
                            death: crate::combat::DeathNaming::Normal,
                            sub_type: None,
                        },
                    )?;
                    killed.push((entity, 0.0));
                    kills.push((entity, by));
                }
                diag!(
                    "debug: kill {target:?} by entity {} - {} object(s)",
                    by.index(),
                    killed.len()
                );
                self.award_kills(tick, ids, &kills)?;
                self.cheer_kills(tick, ids, &kills)?;
                self.resolve_deaths(tick, ids, &killed)
            }
        }
    }

    fn live_dummies(world: &World, ids: Ids) -> Vec<(Entity, Health)> {
        let mut dummies: Vec<(Entity, Health)> = world
            .store(ids.dummy)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        world.is_live(*entity) && !world.contains(ids.doomed, *entity)
                    })
                    .filter_map(|(entity, _)| {
                        let health = world.get(ids.health, entity).copied()?;
                        (!health.0.is_dead()).then_some((entity, health))
                    })
                    .collect()
            })
            .unwrap_or_default();
        dummies.sort_unstable_by_key(|(entity, _)| entity.index());
        dummies
    }

    pub(crate) fn dummy_invincibility(&self, world: &World) -> (usize, bool) {
        let Some(ids) = self.ids else {
            return (0, false);
        };
        let dummies = Self::live_dummies(world, ids);
        let invincible = dummies
            .iter()
            .any(|(_, health)| health.0.kind() == BodyKind::Immortal);
        (dummies.len(), invincible)
    }

    fn reset_powers(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) -> Result<()> {
        let world = tick.state();
        let books: Vec<(Entity, crate::powers::PowerBook)> = world
            .store(ids.power_book)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        world
                            .get(ids.owner, *entity)
                            .is_some_and(|owner| owner.0 == player)
                    })
                    .map(|(entity, book)| (entity, book.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, mut book) in books {
            for instance in &mut book.0 {
                instance.available_on = Frame::ZERO;
            }
            tick.state_mut().insert(ids.power_book, entity, book)?;
        }
        if let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        {
            if tick
                .state()
                .contains(ids.player_power_cooldowns, player_entity)
            {
                tick.state_mut().insert(
                    ids.player_power_cooldowns,
                    player_entity,
                    crate::powers::PlayerPowerCooldowns::default(),
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn unit_catalogue(&self) -> Vec<UnitEntry> {
        let listed_kinds: Vec<Kind> = self
            .buildables
            .iter()
            .chain(&self.hero_buildables)
            .map(|buildable| buildable.kind)
            .collect();
        self.kinds
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.record.has_kind("STRUCTURE"))
            .filter_map(|(at, entry)| {
                let kind = Kind(u16::try_from(at).ok()?);
                let listed = listed_kinds.contains(&kind);
                Some(UnitEntry {
                    kind,
                    label: entry.record.name.clone(),
                    side: entry.record.side.clone().filter(|_| listed),
                    listed,
                })
            })
            .collect()
    }

    pub(crate) fn level_steps(&self, world: &World, player: PlayerId) -> Vec<LevelStep> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let mut holders: Vec<Entity> = world
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, _)| self.sink_of(world, ids, entity))
                    .filter(|holder| world.is_live(*holder) && !world.contains(ids.doomed, *holder))
                    .collect()
            })
            .unwrap_or_default();
        holders.sort_unstable_by_key(|holder| holder.index());
        holders.dedup();
        holders
            .into_iter()
            .filter_map(|holder| {
                let table = self.level_table_of(world, ids, holder)?;
                let (index, points) = self.experience_of(world, ids, holder)?;
                let next = table
                    .level(index.saturating_add(1))
                    .map(|level| (level.rank, level.required));
                let level = table.level(index)?;
                Some(LevelStep {
                    holder,
                    rank: level.rank,
                    points,
                    floor: level.required,
                    next,
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn debug_spawn(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: Kind,
        at: Vec2,
        facing: Facing,
        role: SpawnRole,
    ) -> Result<()> {
        let spawned = self.spawn_template(tick, ids, player, kind, at, facing)?;
        if role.is_dummy() {
            Self::make_dummy(tick, ids, &spawned, role == SpawnRole::FightingDummy)?;
        }
        Ok(())
    }

    pub(super) fn spawn_template(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        kind: Kind,
        at: Vec2,
        facing: Facing,
    ) -> Result<Vec<Entity>> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(Vec::new());
        };
        if entry.record.has_kind("STRUCTURE") {
            return Ok(Vec::new());
        }
        let spawned = match (entry.formation, entry.member) {
            (Some(formation), Some(member_kind)) => {
                let Some(member) = self
                    .kinds
                    .spawn(&self.combat, member_kind, at, facing, player)
                else {
                    return Ok(Vec::new());
                };
                let count = self
                    .formations
                    .get(usize::from(formation))
                    .map_or(0, |table| u32::try_from(table.len()).unwrap_or(0));
                let spawn = HordeSpawn {
                    formation,
                    origin: at,
                    facing,
                    member,
                    count,
                    vision: crate::templates::vision(&entry.record)
                        .unwrap_or(crate::shroud::HORDE_SHROUD_RANGE),
                };
                let Some(horde) = self.spawn_horde_entity(tick, ids, &spawn)? else {
                    return Ok(Vec::new());
                };
                tick.state_mut().insert(ids.kind, horde, kind)?;
                self.attach_upgrade_state(tick, ids, horde, Some(kind))?;
                let mut spawned = tick
                    .state()
                    .get(ids.horde, horde)
                    .map(|horde| horde.members.clone())
                    .unwrap_or_default();
                spawned.push(horde);
                spawned
            }
            _ => {
                let kind = self.resolve_build_variation(tick, kind);
                let Some(spawn) = self.kinds.spawn(&self.combat, kind, at, facing, player) else {
                    return Ok(Vec::new());
                };
                let entity = tick.state_mut().spawn()?;
                Self::insert_spawn(
                    tick,
                    ids,
                    entity,
                    &crate::unit::Spawn {
                        upkeep: self.kinds.upkeep(kind),
                        ..spawn
                    },
                    &self.combat,
                )?;
                self.attach_upgrade_state(tick, ids, entity, Some(kind))?;
                vec![entity]
            }
        };
        Ok(spawned)
    }

    fn debug_targets(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: &DebugTarget,
    ) -> Vec<Entity> {
        let world = tick.state();
        let alive = |entity: Entity| world.is_live(entity) && !world.contains(ids.doomed, entity);
        match target {
            DebugTarget::Named(name) => self.debug_script_targets(tick, ids, name, false),
            DebugTarget::Team(name) => self.debug_script_targets(tick, ids, name, true),
            DebugTarget::Near { kind, x, y } => world
                .store(ids.kind)
                .into_iter()
                .flat_map(|store| store.iter())
                .filter(|&(entity, found)| found.0 == *kind && alive(entity))
                .filter_map(|(entity, _)| {
                    let at = world.get(ids.body, entity)?.0.position;
                    let (dx, dy) = (at.x - x, at.y - y);
                    Some((dx * dx + dy * dy, entity))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                .map(|(_, entity)| entity)
                .into_iter()
                .collect(),
            DebugTarget::Player(side) => {
                let Some(owner) = self.resolve_side_player(side) else {
                    return Vec::new();
                };
                let mut owned: Vec<Entity> = world
                    .store(ids.owner)
                    .into_iter()
                    .flat_map(|store| store.iter())
                    .filter(|&(entity, found)| found.0 == owner && alive(entity))
                    .map(|(entity, _)| entity)
                    .collect();
                owned.sort_unstable_by_key(|entity| entity.index());
                owned
            }
            DebugTarget::Kind { kind, side } => {
                let owner = match side {
                    Some(side) => match self.resolve_side_player(side) {
                        Some(owner) => Some(owner),
                        None => return Vec::new(),
                    },
                    None => None,
                };
                let mut found: Vec<Entity> = world
                    .store(ids.kind)
                    .into_iter()
                    .flat_map(|store| store.iter())
                    .filter(|&(entity, found)| found.0 == *kind && alive(entity))
                    .filter(|&(entity, _)| {
                        owner.is_none_or(|owner| {
                            world.get(ids.owner, entity).is_some_and(|o| o.0 == owner)
                        })
                    })
                    .map(|(entity, _)| entity)
                    .collect();
                found.sort_unstable_by_key(|entity| entity.index());
                found
            }
            DebugTarget::All => {
                let mut owned: Vec<Entity> = world
                    .store(ids.owner)
                    .into_iter()
                    .flat_map(|store| store.iter())
                    .filter(|&(entity, owner)| owner.0 == player && alive(entity))
                    .map(|(entity, _)| entity)
                    .collect();
                owned.sort_unstable_by_key(|entity| entity.index());
                owned
            }
        }
    }

    fn make_dummy(tick: &mut Tick<'_>, ids: Ids, entities: &[Entity], attacks: bool) -> Result<()> {
        for &entity in entities {
            tick.state_mut().insert(ids.dummy, entity, Dummy)?;
            if !attacks {
                tick.state_mut().insert(ids.no_attack, entity, NoAttack)?;
            }
        }
        Ok(())
    }
}
