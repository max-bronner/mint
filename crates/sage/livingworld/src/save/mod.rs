#![allow(missing_docs)]

pub(crate) mod codec;

use std::collections::BTreeMap;

use mint_math::rng::Pcg32;
use mint_sage_save::{
    GameStateBlock, SaveError, SaveFile, CHUNK_GAME_STATE, CHUNK_LIVING_WORLD_LOGIC,
};

use crate::ai::{Skill, StrategicAi};
use crate::model::{
    Army, ArmyId, AutoResolveDialog, Battle, Bonus, BuildingId, Capture, Decision, FallenHero,
    Member, Phase, Player, PlayerId, PlayerKind, PlotBuilding, RegionId, RegionState, TemplateId,
    UnitId, BONUS_FIELDS,
};
use crate::rules::Rules;
use crate::screen_queries::BattleType;
use crate::state::{LivingWorldState, PalantirMarker, Scripted};
use codec::{Dec, Enc};

pub const SESSION_VERSION: u8 = 8;
const TAG: &str = "MintLivingWorld";
const WOTR_SINGLE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRefused {
    BattleAwaited,
    WrongPhase(Phase),
}

impl std::fmt::Display for SaveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BattleAwaited => write!(f, "a battle is awaited or running"),
            Self::WrongPhase(p) => write!(f, "saving is not allowed in {}", p.name()),
        }
    }
}

impl std::error::Error for SaveRefused {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Format(SaveError),
    Invalid(&'static str),
    RulesMismatch,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(e) => write!(f, "{e}"),
            Self::Invalid(what) => write!(f, "invalid session save: {what}"),
            Self::RulesMismatch => write!(f, "the save was made on other rules"),
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKey {
    pub campaign: String,
    pub scenario: usize,
    pub victory_type: usize,
}

fn put_key(e: &mut Enc, rules: &Rules) {
    e.str(&rules.campaign);
    e.u32(u32::try_from(rules.scenario).unwrap_or(u32::MAX));
    e.u32(u32::try_from(rules.victory_type).unwrap_or(u32::MAX));
    for n in [
        rules.regions.len(),
        rules.units.len(),
        rules.buildings.len(),
        rules.templates.len(),
    ] {
        e.len(n);
    }
}

fn get_key(d: &mut Dec) -> Result<(SessionKey, [u32; 4]), LoadError> {
    let key = SessionKey {
        campaign: d.str()?,
        scenario: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
        victory_type: usize::try_from(d.u32()?).unwrap_or(usize::MAX),
    };
    Ok((key, [d.u32()?, d.u32()?, d.u32()?, d.u32()?]))
}

fn put_bonus(e: &mut Enc, b: &Bonus) {
    for v in b.0 {
        e.i32(v);
    }
}

fn get_bonus(d: &mut Dec) -> Result<Bonus, LoadError> {
    let mut b = Bonus([0; BONUS_FIELDS]);
    for slot in &mut b.0 {
        *slot = d.i32()?;
    }
    Ok(b)
}

fn phase_from(ordinal: u8) -> Option<Phase> {
    [
        Phase::Planning,
        Phase::MoveArmies,
        Phase::ResolveBattles,
        Phase::FakePhaseOne,
        Phase::PlanRetreats,
        Phase::RetreatArmies,
        Phase::Complete,
    ]
    .into_iter()
    .find(|p| p.ordinal() == ordinal)
}

fn kind_from(code: u8) -> Option<PlayerKind> {
    [PlayerKind::Human, PlayerKind::Computer, PlayerKind::Inert]
        .into_iter()
        .find(|k| *k as u8 == code)
}

fn put_player(e: &mut Enc, p: &Player) {
    e.u8(p.id.0);
    e.u8(p.template.0);
    e.u8(p.kind as u8);
    e.i32(p.team);
    e.opt(p.base_region, |e, r| e.u16(r.0));
    e.i32(p.resources);
    e.i32(p.reserved_cp);
    e.bool(p.ready);
    e.bool(p.defeated);
    put_bonus(e, &p.bonus_a);
    e.list(&p.territories, |e, t| e.u16(*t));
    e.list(&p.previous_territories, |e, t| e.u16(*t));
    e.list(&p.fallen_heroes, |e, f| {
        e.u16(f.unit.0);
        e.u32(f.level);
        e.u32(f.experience);
    });
    e.u8(skill_code(p.skill));
    e.u32(p.skill_points.to_bits());
    e.list(&p.sciences, |e, s| e.str(s));
    e.list(&p.upgrades, |e, s| e.str(s));
    e.i32(p.rank_points);
}

fn skill_code(skill: Skill) -> u8 {
    skill.difficulty()
}

fn skill_from(code: u8) -> Option<Skill> {
    [Skill::Easy, Skill::Medium, Skill::Hard, Skill::Brutal]
        .into_iter()
        .find(|s| s.difficulty() == code)
}

fn get_player(d: &mut Dec, version: u8) -> Result<Player, LoadError> {
    Ok(Player {
        id: PlayerId(d.u8()?),
        template: TemplateId(d.u8()?),
        kind: kind_from(d.u8()?).ok_or(LoadError::Invalid("player kind"))?,
        team: d.i32()?,
        base_region: d.opt(|d| Ok(RegionId(d.u16()?)))?,
        resources: d.i32()?,
        reserved_cp: d.i32()?,
        ready: d.bool()?,
        defeated: d.bool()?,
        bonus_a: get_bonus(d)?,
        territories: d.list(|d| d.u16())?,
        previous_territories: d.list(|d| d.u16())?,
        fallen_heroes: d.list(|d| {
            let unit = UnitId(d.u16()?);
            let (level, experience) = if version >= 8 {
                (d.u32()?, d.u32()?)
            } else {
                (1, 0)
            };
            Ok(FallenHero {
                unit,
                level,
                experience,
            })
        })?,
        skill: skill_from(d.u8()?).ok_or(LoadError::Invalid("player skill"))?,
        skill_points: f32::from_bits(d.u32()?),
        sciences: d.list(|d| d.str())?,
        upgrades: d.list(|d| d.str())?,
        rank_points: if version >= 7 { d.i32()? } else { 0 },
    })
}

fn put_plot(e: &mut Enc, b: &PlotBuilding) {
    e.u16(b.building.0);
    e.i32(b.turns_left);
    e.bool(b.under_construction);
    e.list(&b.queue, |e, q| e.u8(*q));
    e.u32(b.queue_turns);
    e.i32(b.upgrades_used);
}

fn get_plot(d: &mut Dec) -> Result<PlotBuilding, LoadError> {
    Ok(PlotBuilding {
        building: BuildingId(d.u16()?),
        turns_left: d.i32()?,
        under_construction: d.bool()?,
        queue: d.list(|d| d.u8())?,
        queue_turns: d.u32()?,
        upgrades_used: d.i32()?,
    })
}

fn put_region(e: &mut Enc, r: &RegionState) {
    e.opt(r.owner, |e, o| e.u8(o.0));
    e.bool(r.enabled);
    e.u32(r.changed_turn);
    e.list(&r.present, |e, a| e.u32(a.0));
    e.list(&r.plots, |e, p| e.opt(p.as_ref(), put_plot));
}

fn get_region(d: &mut Dec) -> Result<RegionState, LoadError> {
    Ok(RegionState {
        owner: d.opt(|d| Ok(PlayerId(d.u8()?)))?,
        enabled: d.bool()?,
        changed_turn: d.u32()?,
        present: d.list(|d| Ok(ArmyId(d.u32()?)))?,
        plots: d.list(|d| d.opt(get_plot))?,
    })
}

fn put_member(e: &mut Enc, m: &Member) {
    e.u32(m.id);
    e.u16(m.unit.0);
    e.u32(m.level);
    e.u32(m.count);
    e.u32(m.experience);
    e.u32(m.battles);
    e.u32(m.kills);
    e.u32(m.kills_battle);
    e.opt(m.upgraded_at, |e, (region, plot)| {
        e.u16(region.0);
        e.u8(plot);
    });
}

pub(crate) fn get_member(d: &mut Dec) -> Result<Member, LoadError> {
    Ok(Member {
        id: d.u32()?,
        unit: UnitId(d.u16()?),
        level: d.u32()?,
        count: d.u32()?,
        experience: d.u32()?,
        battles: d.u32()?,
        kills: d.u32()?,
        kills_battle: d.u32()?,
        upgraded_at: d.opt(|d| Ok((RegionId(d.u16()?), d.u8()?)))?,
    })
}

fn put_army(e: &mut Enc, a: &Army) {
    e.u32(a.id.0);
    e.u8(a.player.0);
    e.opt(a.hero, |e, h| e.u16(h.0));
    e.list(&a.roster, put_member);
    e.u16(a.region.0);
    e.opt(a.prev_region, |e, r| e.u16(r.0));
    e.u32(a.turns_here);
    e.opt(a.order, |e, r| e.u16(r.0));
    e.bool(a.destroy_queued);
}

fn get_army(d: &mut Dec) -> Result<Army, LoadError> {
    Ok(Army {
        id: ArmyId(d.u32()?),
        player: PlayerId(d.u8()?),
        hero: d.opt(|d| Ok(UnitId(d.u16()?)))?,
        roster: d.list(get_member)?,
        region: RegionId(d.u16()?),
        prev_region: d.opt(|d| Ok(RegionId(d.u16()?)))?,
        turns_here: d.u32()?,
        order: d.opt(|d| Ok(RegionId(d.u16()?)))?,
        destroy_queued: d.bool()?,
    })
}

fn put_players(e: &mut Enc, list: &[PlayerId]) {
    e.list(list, |e, p| e.u8(p.0));
}

fn get_players(d: &mut Dec) -> Result<Vec<PlayerId>, LoadError> {
    d.list(|d| Ok(PlayerId(d.u8()?)))
}

impl LivingWorldState {
    pub fn check_saveable(&self) -> Result<(), SaveRefused> {
        if self.awaiting.is_some() {
            return Err(SaveRefused::BattleAwaited);
        }
        match self.phase {
            Phase::Planning | Phase::PlanRetreats => Ok(()),
            other => Err(SaveRefused::WrongPhase(other)),
        }
    }

    pub fn save_payload(&self, ai: Option<&StrategicAi>) -> Result<Vec<u8>, SaveRefused> {
        self.check_saveable()?;
        let mut e = Enc::new();
        e.str(TAG);
        e.u8(SESSION_VERSION);
        put_key(&mut e, &self.rules);
        e.u32(self.turn);
        e.u8(self.phase.ordinal());
        let (state, stream) = self.rng.to_parts();
        e.u64(state);
        e.u64(stream);
        e.u32(self.next_army_id);
        e.u32(self.next_member_id);
        e.u32(self.next_battle_id);
        e.opt(self.awaiting, |e, b| e.u32(b));
        e.opt(self.ended, |e, team| e.opt(team, |e, t| e.i32(t)));
        e.list(&self.players, put_player);
        e.list(&self.regions, put_region);
        e.len(self.armies.len());
        for army in self.armies.values() {
            put_army(&mut e, army);
        }
        e.list(&self.battles, |e, b| {
            e.u32(b.id);
            e.u16(b.region.0);
            put_players(e, &b.players);
            e.list(&b.choices, |e, (p, m)| {
                e.u8(p.0);
                e.u8(*m);
            });
            put_players(e, &b.withdrawn);
            e.list(&b.snapshot, |e, (army, roster)| {
                e.u32(army.0);
                e.list(roster, put_member);
            });
        });
        e.list(&self.captures, |e, c| {
            e.u8(c.player.0);
            e.u16(c.region.0);
            e.u8(c.flags);
        });
        e.len(self.decisions.len());
        for (key, d) in &self.decisions {
            e.u16(key.0);
            e.u16(d.region.0);
            put_players(&mut e, &d.players);
            e.u8(d.flags);
            e.bool(d.resolved);
        }
        e.list(&self.retreat_entries, |e, a| e.u32(a.0));
        e.list(&self.destroy_pending, |e, a| e.u32(a.0));
        e.opt(self.dialog.as_ref(), |e, dialog| {
            e.u32(dialog.battle);
            e.list(&dialog.rounds, |e, (p, r)| {
                e.u8(p.0);
                e.u32(*r);
            });
            put_players(e, &dialog.skipped);
        });
        e.opt(self.scripted.as_ref(), |e, scripted| {
            e.list(&scripted.armies, |e, (name, army)| {
                e.str(name);
                e.u32(army.0);
            });
        });
        e.u8(self.battle_type.ordinal());
        e.list(&self.seat_colours, |e, colour| e.u32(*colour));
        e.list(&self.palantir_markers, |e, marker| {
            e.str(&marker.event);
            e.str(&marker.movie);
            e.u32(marker.position[0].to_bits());
            e.u32(marker.position[1].to_bits());
            e.bool(marker.play_next_act_after_movie);
        });
        e.opt(ai, |e, ai| ai.write_save(e));
        Ok(e.into_bytes())
    }

    pub fn payload_key(payload: &[u8], payload_offset: usize) -> Result<SessionKey, LoadError> {
        let mut d = Dec::new(payload, payload_offset);
        head(&mut d)?;
        Ok(get_key(&mut d)?.0)
    }

    pub fn load_payload(
        rules: Rules,
        payload: &[u8],
        payload_offset: usize,
    ) -> Result<(Self, Option<StrategicAi>), LoadError> {
        let mut d = Dec::new(payload, payload_offset);
        let version = head(&mut d)?;
        let (key, shape) = get_key(&mut d)?;
        let same_key = key.campaign == rules.campaign
            && key.scenario == rules.scenario
            && key.victory_type == rules.victory_type;
        let counts = [
            rules.regions.len(),
            rules.units.len(),
            rules.buildings.len(),
            rules.templates.len(),
        ];
        let same_shape = counts
            .iter()
            .zip(shape)
            .all(|(have, saved)| u32::try_from(*have).is_ok_and(|h| h == saved));
        if !same_key || !same_shape {
            return Err(LoadError::RulesMismatch);
        }
        let turn = d.u32()?;
        let phase = phase_from(d.u8()?).ok_or(LoadError::Invalid("phase"))?;
        let rng = Pcg32::from_parts(d.u64()?, d.u64()?);
        let next_army_id = d.u32()?;
        let next_member_id = d.u32()?;
        let next_battle_id = d.u32()?;
        let awaiting = d.opt(|d| d.u32())?;
        let ended = d.opt(|d| d.opt(|d| d.i32()))?;
        let players = d.list(|d| get_player(d, version))?;
        let regions = d.list(get_region)?;
        let mut armies = BTreeMap::new();
        for army in d.list(get_army)? {
            armies.insert(army.id, army);
        }
        let battles = d.list(|d| {
            Ok(Battle {
                id: d.u32()?,
                region: RegionId(d.u16()?),
                players: get_players(d)?,
                choices: d.list(|d| Ok((PlayerId(d.u8()?), d.u8()?)))?,
                withdrawn: if version >= 6 {
                    get_players(d)?
                } else {
                    Vec::new()
                },
                snapshot: d.list(|d| Ok((ArmyId(d.u32()?), d.list(get_member)?)))?,
            })
        })?;
        let captures = d.list(|d| {
            Ok(Capture {
                player: PlayerId(d.u8()?),
                region: RegionId(d.u16()?),
                flags: d.u8()?,
            })
        })?;
        let mut decisions = BTreeMap::new();
        for (key, decision) in d.list(|d| {
            Ok((
                RegionId(d.u16()?),
                Decision {
                    region: RegionId(d.u16()?),
                    players: get_players(d)?,
                    flags: d.u8()?,
                    resolved: d.bool()?,
                },
            ))
        })? {
            decisions.insert(key, decision);
        }
        let retreat_entries = d.list(|d| Ok(ArmyId(d.u32()?)))?;
        let destroy_pending = d.list(|d| Ok(ArmyId(d.u32()?)))?;
        let dialog = d.opt(|d| {
            Ok(AutoResolveDialog {
                battle: d.u32()?,
                rounds: d.list(|d| Ok((PlayerId(d.u8()?), d.u32()?)))?,
                skipped: get_players(d)?,
            })
        })?;

        let scripted = d.opt(|d| {
            Ok(Scripted {
                armies: d.list(|d| Ok((d.str()?, ArmyId(d.u32()?))))?,
            })
        })?;

        let battle_type = if version >= 3 {
            BattleType::from_ordinal(d.u8()?).ok_or(LoadError::Invalid("battle type"))?
        } else {
            BattleType::default()
        };
        let seat_colours = if version >= 4 {
            d.list(|d| d.u32())?
        } else {
            Vec::new()
        };
        let palantir_markers = if version >= 5 {
            d.list(|d| {
                Ok(PalantirMarker {
                    event: d.str()?,
                    movie: d.str()?,
                    position: [f32::from_bits(d.u32()?), f32::from_bits(d.u32()?)],
                    play_next_act_after_movie: d.bool()?,
                })
            })?
        } else {
            Vec::new()
        };

        if players.iter().enumerate().any(|(i, p)| p.id.index() != i) {
            return Err(LoadError::Invalid("player ids"));
        }
        if regions.len() != rules.regions.len() {
            return Err(LoadError::Invalid("region count"));
        }
        let state = Self {
            rules,
            turn,
            phase,
            rng,
            players,
            regions,
            armies,
            next_army_id,
            next_member_id,
            next_battle_id,
            battles,
            captures,
            decisions,
            retreat_entries,
            destroy_pending,
            awaiting,
            ended,
            dialog,
            scripted,
            battle_type,
            seat_colours,
            palantir_markers,
        };
        let ai = d.opt(|d| StrategicAi::read_save(&state, d))?;
        d.finish("MintLivingWorld")?;
        Ok((state, ai))
    }
}

fn head(d: &mut Dec) -> Result<u8, LoadError> {
    if d.str()? != TAG {
        return Err(LoadError::Invalid("not a mint session"));
    }
    d.version("MintLivingWorld", SESSION_VERSION)
}

pub fn save_file(
    state: &LivingWorldState,
    ai: Option<&StrategicAi>,
    description: &str,
    date: [u16; 8],
) -> Result<Vec<u8>, SaveRefused> {
    let payload = state.save_payload(ai)?;
    let info = GameStateBlock {
        save_file_type: WOTR_SINGLE,
        date,
        description: description.into(),
        ..GameStateBlock::default()
    };
    let mut file = SaveFile::default();
    let built = file
        .push_with(CHUNK_GAME_STATE, |_| info.encode())
        .and_then(|()| file.push_with(CHUNK_LIVING_WORLD_LOGIC, |_| payload));
    debug_assert!(built.is_ok());
    Ok(file.to_bytes())
}

fn session_block(file: &SaveFile) -> Result<(&[u8], usize), LoadError> {
    let (index, block) = file
        .find(CHUNK_LIVING_WORLD_LOGIC)
        .ok_or(LoadError::Format(SaveError::MissingBlock(
            "CHUNK_LivingWorldLogic",
        )))?;
    let offset = file.payload_offset(index).unwrap_or(0);
    Ok((block.payload(), offset))
}

pub fn file_key(bytes: &[u8]) -> Result<SessionKey, LoadError> {
    let file = SaveFile::parse(bytes)?;
    let (payload, offset) = session_block(&file)?;
    LivingWorldState::payload_key(payload, offset)
}

pub fn load_file(
    rules: Rules,
    bytes: &[u8],
) -> Result<(LivingWorldState, Option<StrategicAi>), LoadError> {
    let file = SaveFile::parse(bytes)?;
    let (payload, offset) = session_block(&file)?;
    LivingWorldState::load_payload(rules, payload, offset)
}

#[cfg(test)]
mod tests;
