#![allow(missing_docs)]

use mint_math::rng::Pcg32;

use mint_sage_ini::living_world::players::BuildingType;

use crate::model::{
    bonus, Army, ArmyId, Battle, BattleMode, BattleResult, Bonus, Capture, Decision, Event, Member,
    PlayerId, PlayerKind, RegionId, Rejected,
};
use crate::rules::RESOLVE_TYPE_HERO;
use crate::state::LivingWorldState;

#[derive(Debug, Clone, PartialEq)]
pub struct SpecSide {
    pub players: Vec<PlayerId>,
    pub armies: Vec<Army>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BattleSpec {
    pub battle: u32,
    pub region: RegionId,
    pub owner: Option<PlayerId>,
    pub sides: Vec<SpecSide>,
    pub map: Option<String>,
    pub region_name: String,
    pub neighbours: Vec<String>,
    pub participants: Vec<Participant>,
    pub seed: u64,
    pub weapon_scale: Vec<(PlayerId, f32)>,
    pub round_cap: Option<u32>,
    pub revival_multipliers: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    pub player: PlayerId,
    pub side_name: String,
    pub faction: Option<String>,
    pub team: i32,
    pub kind: PlayerKind,
    pub defender: bool,
    pub cash: i32,
    pub command_points: i32,
    pub skill: crate::ai::Skill,
    pub skill_points: f32,
    pub sciences: Vec<String>,
    pub upgrades: Vec<String>,
    pub bonus: Bonus,
    pub legendary: i32,
    pub armies: Vec<SpecArmy>,
    pub structures: Vec<SpecStructure>,
    pub bonus_units: Vec<(String, u32)>,
    pub fallen: Vec<SpecFallen>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFallen {
    pub template: String,
    pub level: u32,
    pub experience: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecArmy {
    pub army: ArmyId,
    pub hero: Option<String>,
    pub origin: String,
    pub units: Vec<SpecUnit>,
    pub arrives_after: Option<u32>,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecUnit {
    pub template: String,
    pub count: u32,
    pub level: u32,
    pub upgraded: bool,
    pub hero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecStructure {
    pub plot: u8,
    pub template: String,
    pub army: ArmyId,
}

pub trait AutoResolve {
    fn resolve(&mut self, spec: &BattleSpec, rng: &mut Pcg32) -> BattleResult;
}

pub(crate) fn reborrow<'a>(
    resolver: &'a mut Option<&mut dyn AutoResolve>,
) -> Option<&'a mut dyn AutoResolve> {
    match resolver {
        Some(r) => Some(&mut **r),
        None => None,
    }
}

#[must_use]
pub fn battle_sides(state: &LivingWorldState, battle: &Battle) -> Vec<Vec<PlayerId>> {
    let mut sides: Vec<Vec<PlayerId>> = Vec::new();
    for p in &battle.players {
        match sides
            .iter_mut()
            .find(|side| side.first().is_some_and(|f| state.allied(*f, *p)))
        {
            Some(side) => {
                if !side.contains(p) {
                    side.push(*p);
                }
            }
            None => sides.push(vec![*p]),
        }
    }
    sides
}

impl LivingWorldState {
    pub(crate) fn queue_capture(&mut self, region: RegionId, player: PlayerId, flags: u8) {
        if !self.captures.iter().any(|c| c.region == region) {
            self.captures.push(Capture {
                player,
                region,
                flags,
            });
        }
    }

    pub(crate) fn shared_capture(&mut self, region: RegionId, players: &[PlayerId], flags: u8) {
        match self.decisions.get_mut(&region) {
            Some(d) => {
                d.players.extend_from_slice(players);
                d.flags |= flags;
            }
            None => {
                self.decisions.insert(
                    region,
                    Decision {
                        region,
                        players: players.to_vec(),
                        flags: flags | 0x20,
                        resolved: false,
                    },
                );
            }
        }
    }

    #[must_use]
    pub fn decision_chooser(&self, decision: &Decision) -> Option<PlayerId> {
        let mut humans: Vec<PlayerId> = Vec::new();
        for p in &decision.players {
            if self.kind(*p) == Some(PlayerKind::Human) && !humans.contains(p) {
                humans.push(*p);
            }
        }
        match humans.as_slice() {
            [] => decision.players.first().copied(),
            [only] => Some(*only),
            _ => Some(PlayerId(0)).filter(|p| decision.players.contains(p)),
        }
    }

    pub(crate) fn create_battle(
        &mut self,
        region: RegionId,
        players: Vec<PlayerId>,
        events: &mut Vec<Event>,
    ) {
        if self.battles.iter().any(|b| b.region == region) {
            return;
        }
        self.next_battle_id = self.next_battle_id.saturating_add(1);
        let id = self.next_battle_id;
        let owner = self.owner(region);
        self.battles.push(Battle {
            id,
            region,
            players: players.clone(),
            choices: Vec::new(),
            withdrawn: Vec::new(),
            snapshot: Vec::new(),
        });
        events.push(Event::BattlePending { battle: id, region });
        let Some(owner) = owner.filter(|o| players.contains(o)) else {
            return;
        };
        let has_army = self.region(region).is_some_and(|r| {
            r.present
                .iter()
                .any(|a| self.armies.get(a).is_some_and(|a| a.player == owner))
        });
        if has_army || !self.is_defended(region) {
            return;
        }
        let standing = self
            .armies_of(owner)
            .find(|a| !a.is_hero_army() && a.region == region)
            .map(|a| a.id);
        let defender = match standing {
            Some(id) => id,
            None => self.spawn_army(owner, region, None, &[]),
        };
        if let Some(r) = self.region_mut(region) {
            if !r.present.contains(&defender) {
                r.present.push(defender);
            }
        }
    }

    pub(crate) fn auto_resolve_humanless(
        &mut self,
        mut resolver: Option<&mut dyn AutoResolve>,
        events: &mut Vec<Event>,
    ) -> usize {
        let mut resolved = 0;
        loop {
            let Some(resolver) = reborrow(&mut resolver) else {
                return resolved;
            };
            let next = self.battles.iter().find_map(|b| {
                let human = b
                    .players
                    .iter()
                    .any(|p| self.kind(*p) == Some(PlayerKind::Human));
                (!human).then(|| (b.region, b.players.first().copied()))
            });
            let Some((region, Some(player))) = next else {
                return resolved;
            };
            match self.order_resolve(player, region, BattleMode::Auto, Some(resolver)) {
                Ok(more) => events.extend(more),
                Err(_) => return resolved,
            }
            resolved = resolved.saturating_add(1);
        }
    }

    pub(crate) fn spec_for(&self, battle: &Battle) -> BattleSpec {
        let present: &[ArmyId] = self
            .region(battle.region)
            .map_or(&[], |r| r.present.as_slice());
        let mut players = battle.players.clone();
        players.sort();
        players.dedup();
        let participants = players
            .iter()
            .filter_map(|p| self.participant(battle.region, *p, present))
            .collect();
        let sides = battle_sides(self, battle)
            .into_iter()
            .map(|players| SpecSide {
                armies: present
                    .iter()
                    .filter_map(|id| self.armies.get(id))
                    .filter(|a| players.contains(&a.player))
                    .cloned()
                    .collect(),
                players,
            })
            .collect();
        let rules = self.rules.region_rules(battle.region);
        let name = |id: &RegionId| self.rules.region_rules(*id).map(|r| r.name.clone());
        BattleSpec {
            battle: battle.id,
            region: battle.region,
            owner: self.owner(battle.region),
            sides,
            map: rules.and_then(|r| r.map.clone()),
            region_name: rules.map(|r| r.name.clone()).unwrap_or_default(),
            neighbours: rules
                .map(|r| r.neighbours.iter().filter_map(name).collect())
                .unwrap_or_default(),
            participants,
            seed: self.battle_seed(battle.id),
            weapon_scale: Vec::new(),
            round_cap: None,
            revival_multipliers: (
                self.rules.revival_cost_multiplier,
                self.rules.revival_time_multiplier,
            ),
        }
    }

    fn battle_seed(&self, battle: u32) -> u64 {
        let mut z = self
            .state_hash()
            .wrapping_add(u64::from(battle).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn human_lost_a_decisive_map(&self, battle: &Battle, winner: &[PlayerId]) -> bool {
        let decisive = self
            .rules
            .region_rules(battle.region)
            .is_some_and(|r| r.lose_campaign_if_lose_map);
        let humans: Vec<PlayerId> = battle
            .players
            .iter()
            .copied()
            .filter(|p| self.player(*p).is_some_and(|x| x.kind == PlayerKind::Human))
            .collect();
        decisive && !humans.is_empty() && humans.iter().all(|p| !winner.contains(p))
    }

    fn fortified(&self, region: RegionId) -> bool {
        let auto = self
            .rules
            .region_rules(region)
            .is_some_and(|r| r.create_auto_fort);
        auto || self.region(region).is_some_and(|r| {
            r.plots.iter().flatten().any(|plot| {
                !plot.under_construction
                    && self
                        .rules
                        .building(plot.building)
                        .is_some_and(|b| b.kind == Some(BuildingType::Fortress))
            })
        })
    }

    fn participant(
        &self,
        region: RegionId,
        p: PlayerId,
        present: &[ArmyId],
    ) -> Option<Participant> {
        let player = self.player(p)?;
        let template = self.rules.templates.get(usize::from(player.template.0));
        let defender = self.owner(region) == Some(p);
        let a = player.bonus_a;
        let mut b = a;
        if defender {
            if let Some(r) = self.rules.region_rules(region) {
                b.add(&r.bonus);
            }
        }
        let base = if defender && self.fortified(region) {
            self.rules.starting_cash_rts_with_fort
        } else {
            self.rules.starting_cash_rts
        };
        let unit = |m: &Member| SpecUnit {
            template: self.rules.unit_name(m.unit).to_owned(),
            count: m.count,
            level: m.level,
            upgraded: m.is_upgraded(),
            hero: self.rules.unit(m.unit).hero,
        };
        let period = u32::try_from(self.rules.seconds_per_reinforcement).unwrap_or(0);
        let mut led = 0u32;
        let mut primary_taken = false;
        let mut armies = Vec::new();
        for army in present
            .iter()
            .filter_map(|id| self.armies.get(id))
            .filter(|a| a.player == p)
        {
            let arrives_after = if army.is_hero_army() {
                let k = led;
                led = led.saturating_add(1);
                (k > 0 && period > 0).then(|| k.saturating_mul(period))
            } else {
                None
            };
            let primary = arrives_after.is_none() && !primary_taken;
            primary_taken |= primary;
            armies.push(SpecArmy {
                army: army.id,
                hero: army.hero.map(|h| self.rules.unit_name(h).to_owned()),
                origin: self
                    .rules
                    .region_rules(army.prev_region.unwrap_or(army.region))
                    .map(|r| r.name.clone())
                    .unwrap_or_default(),
                units: army.roster.iter().map(unit).collect(),
                arrives_after,
                primary,
            });
        }
        let garrison = armies
            .iter()
            .find(|a| a.hero.is_none())
            .map(|a| a.army)
            .filter(|_| defender);
        let structures = match (garrison, self.region(region)) {
            (Some(army), Some(state)) => state
                .plots
                .iter()
                .enumerate()
                .filter_map(|(i, plot)| {
                    let plot = plot.as_ref().filter(|b| !b.under_construction)?;
                    let template = self.rules.building(plot.building)?.battle_thing.clone()?;
                    Some(SpecStructure {
                        plot: u8::try_from(i.checked_add(1)?).ok()?,
                        template,
                        army,
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut bonus_units = Vec::new();
        for (name, field) in [
            (template.and_then(|t| t.dozer.clone()), bonus::FREE_BUILDER),
            (
                template.and_then(|t| t.inn_unit.clone()),
                bonus::FREE_INN_UNITS,
            ),
        ] {
            let count = u32::try_from(a.get(field)).unwrap_or(0);
            if let (Some(name), true) = (name, count > 0) {
                bonus_units.push((name, count));
            }
        }
        let side_name = if defender {
            "Player_1".to_owned()
        } else {
            format!("Player_{}", u32::from(p.0).saturating_add(2))
        };
        Some(Participant {
            player: p,
            side_name,
            faction: template.and_then(|t| t.faction.clone()),
            team: player.team,
            kind: player.kind,
            defender,
            cash: base.saturating_add(a.get(bonus::EXTRA_START_RESOURCES)),
            command_points: self.battle_cp_limit(region, p),
            skill: player.skill,
            skill_points: player.skill_points,
            sciences: player.sciences.clone(),
            upgrades: player.upgrades.clone(),
            bonus: b,
            legendary: a.get(bonus::LEGENDARY),
            armies,
            structures,
            bonus_units,
            fallen: player
                .fallen_heroes
                .iter()
                .map(|f| SpecFallen {
                    template: self.rules.unit_name(f.unit).to_owned(),
                    level: f.level,
                    experience: f.experience,
                })
                .collect(),
        })
    }

    fn battle_cp_limit(&self, region: RegionId, p: PlayerId) -> i32 {
        let limit = self.rules.region_rules(region).map_or(0, |r| r.cp_limit);
        let per_hero = self
            .player(p)
            .and_then(|pl| self.rules.templates.get(usize::from(pl.template.0)))
            .map_or(0, |t| t.max_hero_cp);
        let heroes = self
            .armies_of(p)
            .filter(|a| a.is_hero_army() && a.region == region)
            .count();
        limit.saturating_add(per_hero.saturating_mul(i32::try_from(heroes).unwrap_or(i32::MAX)))
    }

    fn battle_at(&self, player: PlayerId, region: RegionId) -> Result<usize, Rejected> {
        let index = self
            .battles
            .iter()
            .position(|b| b.region == region)
            .ok_or(Rejected::NoSuchBattle)?;
        let in_battle = self
            .battles
            .get(index)
            .is_some_and(|b| b.players.contains(&player));
        if in_battle {
            Ok(index)
        } else {
            Err(Rejected::NotInBattle)
        }
    }

    pub(crate) fn order_resolve(
        &mut self,
        player: PlayerId,
        region: RegionId,
        mode: BattleMode,
        resolver: Option<&mut dyn AutoResolve>,
    ) -> Result<Vec<Event>, Rejected> {
        let index = self.battle_at(player, region)?;
        let battle = self.battles.get(index).ok_or(Rejected::NoSuchBattle)?;
        let spec = self.spec_for(battle);
        let id = battle.id;
        match mode {
            BattleMode::Auto => {
                let resolver = resolver.ok_or(Rejected::NoResolver)?;
                let result = resolver.resolve(&spec, &mut self.rng);
                self.dialog = Some(crate::model::AutoResolveDialog {
                    battle: id,
                    ..Default::default()
                });
                let mut events = Vec::new();
                self.finish_battle(index, &result, BattleMode::Auto, &mut events);
                Ok(events)
            }
            BattleMode::RealTime => {
                self.awaiting = Some(id);
                let snapshot = spec
                    .sides
                    .iter()
                    .flat_map(|side| &side.armies)
                    .map(|a| (a.id, a.roster.clone()))
                    .collect();
                if let Some(b) = self.battles.get_mut(index) {
                    b.snapshot = snapshot;
                }
                Ok(vec![Event::BattleSpec(spec)])
            }
        }
    }

    pub(crate) fn order_apply_result(
        &mut self,
        player: PlayerId,
        region: RegionId,
        result: &BattleResult,
    ) -> Result<Vec<Event>, Rejected> {
        let index = self.battle_at(player, region)?;
        if self.battles.get(index).map(|b| b.id) != self.awaiting {
            return Err(Rejected::NoSuchBattle);
        }
        let mut events = Vec::new();
        self.finish_battle(index, result, BattleMode::RealTime, &mut events);
        Ok(events)
    }

    pub(crate) fn apply_survivors(&mut self, result: &BattleResult) {
        for (army, roster) in &result.rosters {
            if let Some(a) = self.armies.get_mut(army) {
                a.roster = with_history(&a.roster, roster);
            }
        }
        self.return_fallen_heroes(&result.fallen_heroes);
        for (p, entry) in &result.fallen_records {
            if let Some(player) = self.player_mut(*p) {
                player.fallen_heroes.push(*entry);
            }
        }
    }

    pub(crate) fn finish_battle(
        &mut self,
        index: usize,
        result: &BattleResult,
        mode: BattleMode,
        events: &mut Vec<Event>,
    ) {
        let battle = self.battles.remove(index);
        self.awaiting = None;
        self.apply_survivors(result);
        for (p, carry) in &result.players {
            if let Some(player) = self.player_mut(*p) {
                player.skill_points = carry.skill_points;
                player.rank_points = carry.rank_points;
                player.sciences.clone_from(&carry.sciences);
                player.upgrades.clone_from(&carry.upgrades);
            }
        }
        if mode == BattleMode::Auto {
            for p in &battle.players {
                let grants = self
                    .player(*p)
                    .map(|player| self.rules.fresh_rank_grants(player.skill_points));
                if let (Some(grants), Some(player)) = (grants, self.player_mut(*p)) {
                    player.rank_points = grants;
                }
            }
        }
        events.push(Event::BattleFinished {
            region: battle.region,
            winner: result.winner,
        });
        let sides = battle_sides(self, &battle);
        let Some(winner) = result
            .winner
            .and_then(|w| sides.get(usize::from(w)))
            .cloned()
        else {
            return;
        };
        if mode == BattleMode::RealTime && self.human_lost_a_decisive_map(&battle, &winner) {
            self.ended = Some(None);
            events.push(Event::SessionEnded { winning_team: None });
            return;
        }
        let present: Vec<ArmyId> = self
            .region(battle.region)
            .map(|r| r.present.clone())
            .unwrap_or_default();
        let on_field = |id: &ArmyId| result.rosters.iter().any(|(a, _)| a == id);
        let real_time = mode == BattleMode::RealTime;
        if real_time {
            for p in &winner {
                let garrisons: Vec<ArmyId> = present
                    .iter()
                    .filter(|id| {
                        self.armies
                            .get(id)
                            .is_some_and(|a| a.player == *p && !a.is_hero_army())
                    })
                    .copied()
                    .collect();
                for g in garrisons {
                    let charged = self.keep_garrison(g, *p, &battle, &present, &on_field, events);
                    events.push(Event::GarrisonKept { army: g, charged });
                }
            }
        }
        for side in sides.iter().filter(|s| **s != winner) {
            for id in &present {
                let Some(army) = self.armies.get(id) else {
                    continue;
                };
                if !side.contains(&army.player) {
                    continue;
                }
                if army.is_hero_army() {
                    if real_time && on_field(id) {
                        let rules = &self.rules;
                        if let Some(a) = self.armies.get_mut(id) {
                            a.roster.retain(|m| rules.unit(m.unit).hero);
                        }
                    }
                    if !self.retreat_entries.contains(id) {
                        self.retreat_entries.push(*id);
                    }
                } else {
                    self.queue_destroy(*id);
                }
            }
        }
        let owner = self.owner(battle.region);
        let Some(first) = winner.first().copied() else {
            return;
        };
        if self.allied_owner(owner, first) {
            return;
        }
        let human = winner
            .iter()
            .any(|p| self.kind(*p) == Some(PlayerKind::Human));
        let mut flags = u8::from(owner.is_some());
        if !battle.withdrawn.is_empty() {
            flags |= if owner.is_some_and(|o| battle.withdrawn.contains(&o)) {
                0x8
            } else {
                0x10
            };
        }
        if winner.len() == 1 {
            self.queue_capture(battle.region, first, flags);
        } else if human {
            self.shared_capture(battle.region, &winner, flags | 0x20);
        } else {
            self.queue_capture(battle.region, first, flags | 0x20);
        }
    }

    pub(crate) fn order_choose_capture(
        &mut self,
        player: PlayerId,
        region: RegionId,
        chosen: PlayerId,
    ) -> Result<Vec<Event>, Rejected> {
        let decision = self.decisions.get(&region).ok_or(Rejected::NoDecision)?;
        if decision.resolved || !decision.players.contains(&chosen) {
            return Err(Rejected::NoDecision);
        }
        if self.decision_chooser(decision) != Some(player) {
            return Err(Rejected::NotChooser);
        }
        let flags = decision.flags;
        if let Some(d) = self.decisions.get_mut(&region) {
            d.resolved = true;
        }
        self.queue_capture(region, chosen, flags);
        Ok(Vec::new())
    }

    pub(crate) fn set_region_owner(&mut self, region: RegionId, new: Option<PlayerId>) {
        let old = self.owner(region);
        let keeps_alliance = match (old, new) {
            (Some(o), Some(n)) => self.allied(o, n),
            _ => false,
        };
        let turn = self.turn;
        if let Some(r) = self.region_mut(region) {
            r.owner = new;
            if !keeps_alliance {
                r.changed_turn = turn;
            }
        }
    }

    pub(crate) fn apply_captures(&mut self, events: &mut Vec<Event>) {
        let queue = std::mem::take(&mut self.captures);
        for c in queue {
            let live = self.player(c.player).is_some_and(|p| !p.defeated);
            if live && self.owner(c.region) != Some(c.player) {
                self.set_region_owner(c.region, Some(c.player));
                events.push(Event::RegionCaptured {
                    region: c.region,
                    player: c.player,
                });
            }
        }
    }
}

fn units_of(roster: &[Member]) -> Vec<Member> {
    roster
        .iter()
        .flat_map(|m| {
            (0..m.count.max(1)).map(move |_| Member {
                count: 1,
                ..m.clone()
            })
        })
        .collect()
}

pub(crate) fn with_history(before: &[Member], after: &[Member]) -> Vec<Member> {
    let mut left: Vec<u32> = before.iter().map(|m| m.count.max(1)).collect();
    after
        .iter()
        .map(|m| {
            let prior = before
                .iter()
                .zip(left.iter_mut())
                .find(|(b, n)| b.unit == m.unit && **n > 0);
            let (battles, kills) = prior.map_or((0, 0), |(b, n)| {
                *n = n.saturating_sub(m.count.max(1));
                (b.battles, b.kills)
            });
            Member {
                battles: battles.saturating_add(1),
                kills: kills.saturating_add(m.kills_battle),
                ..m.clone()
            }
        })
        .collect()
}

fn count_of(roster: &[Member], unit: crate::model::UnitId) -> u32 {
    roster
        .iter()
        .filter(|m| m.unit == unit)
        .fold(0u32, |n, m| n.saturating_add(m.count))
}

impl LivingWorldState {
    fn return_fallen_heroes(&mut self, fallen: &[(PlayerId, crate::model::FallenHero)]) {
        for (p, entry) in fallen {
            let hero = &entry.unit;
            let army = self
                .armies_of(*p)
                .find(|a| a.hero == Some(*hero))
                .map(|a| a.id);
            let Some(id) = army else {
                if let Some(player) = self.player_mut(*p) {
                    player.fallen_heroes.push(*entry);
                }
                continue;
            };
            let member = Member {
                id: self.fresh_member_id(),
                unit: *hero,
                level: 1,
                count: 1,
                experience: 0,
                battles: 0,
                kills: 0,
                kills_battle: 0,
                upgraded_at: None,
            };
            if let Some(a) = self.armies.get_mut(&id) {
                if !a.roster.iter().any(|m| m.unit == *hero) {
                    a.roster.push(member);
                }
            }
        }
    }

    fn unit_cp(&self, m: &Member) -> i32 {
        self.rules
            .unit(m.unit)
            .command_points
            .saturating_mul(i32::try_from(m.count).unwrap_or(i32::MAX))
    }

    fn move_unit(
        &mut self,
        p: PlayerId,
        pool: &mut Vec<Member>,
        at: usize,
        into: ArmyId,
        paid: Option<&mut Vec<Event>>,
    ) -> i32 {
        if at >= pool.len() {
            return 0;
        }
        let mut unit = pool.remove(at);
        unit.id = self.fresh_member_id();
        let price = paid.map_or(0, |events| {
            let price = self.unit_price(p, unit.unit);
            events.push(Event::MemberCharged {
                army: into,
                member: unit.id,
                price,
            });
            price
        });
        if let Some(a) = self.armies.get_mut(&into) {
            a.roster.push(unit);
        }
        price
    }

    fn keep_garrison(
        &mut self,
        garrison: ArmyId,
        p: PlayerId,
        battle: &Battle,
        present: &[ArmyId],
        on_field: &dyn Fn(&ArmyId) -> bool,
        events: &mut Vec<Event>,
    ) -> i32 {
        let Some(g) = self.armies.get(&garrison) else {
            return 0;
        };
        let mut pool = units_of(&g.roster);
        let snapshot_of = |id: ArmyId| -> Vec<Member> {
            battle
                .snapshot
                .iter()
                .find(|(a, _)| *a == id)
                .map(|(_, r)| r.clone())
                .unwrap_or_default()
        };
        let led: Vec<ArmyId> = present
            .iter()
            .filter(|id| on_field(id))
            .filter(|id| {
                self.armies
                    .get(id)
                    .is_some_and(|a| a.player == p && a.is_hero_army())
            })
            .copied()
            .collect();
        let mut total = 0i32;
        for h in &led {
            let snapshot = snapshot_of(*h);
            for record in &snapshot {
                let t = record.unit;
                let budget = self.unit_cp(record);
                let want = count_of(&snapshot, t);
                let have = |state: &Self| state.armies.get(h).map_or(0, |a| count_of(&a.roster, t));
                for _ in 0..want.saturating_sub(have(self)) {
                    let Some(at) = pool.iter().rposition(|m| m.unit == t) else {
                        break;
                    };
                    self.move_unit(p, &mut pool, at, *h, None);
                }
                let kind = self.rules.unit(t).unit_type;
                if kind == Some(RESOLVE_TYPE_HERO) {
                    continue;
                }
                let fits = |state: &Self, m: &Member| {
                    state.rules.unit(m.unit).unit_type == kind && state.unit_cp(m) <= budget
                };
                let available = pool.iter().filter(|m| fits(self, m)).count();
                let missing =
                    usize::try_from(want.saturating_sub(have(self))).unwrap_or(usize::MAX);
                for _ in 0..missing.min(available) {
                    let Some(at) = pool.iter().rposition(|m| fits(self, m)) else {
                        break;
                    };
                    let price = self.move_unit(p, &mut pool, at, *h, Some(&mut *events));
                    total = total.saturating_add(price);
                }
            }
        }
        pool.sort_by(|a, b| {
            a.experience
                .cmp(&b.experience)
                .then_with(|| self.unit_cp(a).cmp(&self.unit_cp(b)))
        });
        for h in &led {
            let cap = snapshot_of(*h)
                .iter()
                .fold(0i32, |sum, m| sum.saturating_add(self.unit_cp(m)));
            let mut used = self.armies.get(h).map_or(0, |a| self.army_cp(a));
            while used < cap {
                let Some(last) = pool.last() else { break };
                let cp = self.unit_cp(last);
                if used.saturating_add(cp) > cap || self.rules.unit(last.unit).hero {
                    break;
                }
                let at = pool.len().saturating_sub(1);
                let price = self.move_unit(p, &mut pool, at, *h, Some(&mut *events));
                total = total.saturating_add(price);
                used = used.saturating_add(cp);
            }
        }
        let mut own = vec![false; pool.len()];
        for g2 in present {
            let leaderless = self
                .armies
                .get(g2)
                .is_some_and(|a| a.player == p && !a.is_hero_army());
            if !leaderless {
                continue;
            }
            for record in snapshot_of(*g2) {
                let first = pool
                    .iter()
                    .zip(&own)
                    .position(|(m, taken)| !taken && m.unit == record.unit);
                if let Some(slot) = first.and_then(|at| own.get_mut(at)) {
                    *slot = true;
                }
            }
        }
        for unit in &mut pool {
            unit.id = self.fresh_member_id();
        }
        for (unit, own) in pool.iter().zip(&own) {
            if !own {
                let price = self.unit_price(p, unit.unit);
                total = total.saturating_add(price);
                events.push(Event::MemberCharged {
                    army: garrison,
                    member: unit.id,
                    price,
                });
            }
        }
        if let Some(g) = self.armies.get_mut(&garrison) {
            g.roster = pool;
        }
        self.add_resources(p, total.saturating_neg());
        total
    }
}
