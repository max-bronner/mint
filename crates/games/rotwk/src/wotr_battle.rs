use mint_movement::Vec2;
use mint_sage_combat::{AttributeKind, Modifier, ModifierList};
use mint_sage_livingworld::battle::{BattleSpec, Participant};
use mint_sage_livingworld::{
    bonus, ArmyId, BattleResult, FallenHero, Member, PlayerCarry, PlayerKind, Rules, Skill,
};
use mint_sage_map::Object;
use mint_sim::{PlayerId, World};

use crate::game_data::GameData;
use crate::lobby::{Difficulty, MatchRules};
use crate::match_values::{
    DevDeploy, MatchDifficulty, MatchStart, PlayerSeat, StartAt, StartingEntities,
};
use crate::rules::wotr_battle::{WotrArmy, WotrSeat, WotrStart, WotrStructure};
use crate::rules::RotwkRules;

const SKILL_SCALAR_TABLE: &str = "PlayerSkillPointsScalarTable";

const PLOT_SPACING: f32 = 280.0;

#[derive(Debug, Clone)]
pub(crate) struct SpecHandle(pub(crate) std::sync::Arc<BattleSpec>);

impl PartialEq for SpecHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SpecHandle {}

impl std::hash::Hash for SpecHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.battle.hash(state);
        self.0.region.0.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrBattle {
    pub(crate) region: String,
    pub(crate) neighbours: Vec<String>,
    pub(crate) seats: Vec<BattleSeat>,
    pub(crate) skill_scalars: Vec<f32>,
    pub(crate) seed: u64,
    pub(crate) two_by_one: bool,
    pub(crate) revival_multipliers: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BattleSeat {
    pub(crate) player: PlayerId,
    pub(crate) owner: bool,
    pub(crate) owner_side: bool,
    pub(crate) armies: Vec<BattleArmy>,
    pub(crate) structures: Vec<(u8, String, u32)>,
    pub(crate) modifiers: ModifierList,
    pub(crate) purchase_points: u32,
    pub(crate) skill_points: f32,
    pub(crate) sciences: Vec<String>,
    pub(crate) upgrades: Vec<String>,
    pub(crate) fallen: Vec<(String, u32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BattleArmy {
    pub(crate) army: u32,
    pub(crate) origin: String,
    pub(crate) units: Vec<(String, u32, u32)>,
    pub(crate) arrives_after: Option<u32>,
}

fn player_list(index: u8, b: &mint_sage_livingworld::Bonus) -> ModifierList {
    #[allow(clippy::cast_precision_loss)]
    let percent = |field: usize| b.get(field) as f32 / 100.0;
    let modifier = |kind, value| Modifier {
        kind,
        value,
        types: 0,
    };
    ModifierList {
        name: format!("AttributeMod_WOTR_Player_{index}"),
        category: None,
        modifiers: vec![
            modifier(AttributeKind::DamageMult, 1.0 + percent(bonus::ATTACK)),
            modifier(AttributeKind::Armor, percent(bonus::DEFENSE)),
            modifier(AttributeKind::Experience, 1.0 + percent(bonus::EXPERIENCE)),
        ],
        duration_ms: Some(0),
        ignore_if_anti_category_active: false,
    }
}

fn template_name(data: &GameData, name: &str) -> Option<String> {
    data.templates.get(name).map(|record| record.name.clone())
}

fn summon_order(
    data: &GameData,
    army: &mint_sage_livingworld::battle::SpecArmy,
) -> Vec<(String, u32, u32)> {
    let (heroes, rest): (Vec<_>, Vec<_>) = army.units.iter().partition(|u| u.hero);
    heroes
        .into_iter()
        .chain(rest)
        .filter_map(|u| Some((template_name(data, &u.template)?, u.count, u.level)))
        .collect()
}

fn seat_of(
    p: &Participant,
    index: usize,
    spec: &BattleSpec,
    data: &GameData,
    faction: &str,
) -> BattleSeat {
    let owner_team = spec
        .participants
        .iter()
        .find(|q| q.defender)
        .map(|q| q.team);
    let owner_side = p.defender || owner_team.is_some_and(|t| t != -1 && t == p.team);
    let tactical_units: Vec<String> = data
        .players
        .get(faction)
        .map(|t| t.starting_units_wotr.clone())
        .unwrap_or_default();
    let armies = p
        .armies
        .iter()
        .map(|army| {
            let mut units = summon_order(data, army);
            if army.primary {
                units.extend(
                    tactical_units
                        .iter()
                        .filter_map(|name| Some((template_name(data, name)?, 1, 1))),
                );
                units.extend(
                    p.bonus_units
                        .iter()
                        .filter_map(|(name, n)| Some((template_name(data, name)?, *n, 1))),
                );
            }
            BattleArmy {
                army: army.army.0,
                origin: army.origin.clone(),
                units,
                arrives_after: army.arrives_after,
            }
        })
        .collect();
    BattleSeat {
        player: PlayerId::new(u8::try_from(index).unwrap_or(u8::MAX)),
        owner: p.defender,
        owner_side,
        armies,
        structures: p
            .structures
            .iter()
            .filter_map(|s| Some((s.plot, template_name(data, &s.template)?, s.army.0)))
            .collect(),
        modifiers: player_list(p.player.0.saturating_add(1), &p.bonus),
        purchase_points: u32::try_from(p.legendary).unwrap_or(0),
        skill_points: p.skill_points,
        sciences: p.sciences.clone(),
        upgrades: p.upgrades.clone(),
        fallen: p
            .fallen
            .iter()
            .filter_map(|f| Some((template_name(data, &f.template)?, f.level, f.experience)))
            .collect(),
    }
}

fn difficulty_of(skill: Skill) -> Difficulty {
    match skill {
        Skill::Easy => Difficulty::Easy,
        Skill::Medium => Difficulty::Medium,
        Skill::Hard => Difficulty::Hard,
        Skill::Brutal => Difficulty::Brutal,
    }
}

impl MatchStart {
    pub(crate) fn for_wotr_battle(
        spec: &BattleSpec,
        setup: &crate::lobby::SkirmishSetup,
        data: &GameData,
    ) -> Result<Self, String> {
        let mut players = Vec::new();
        let mut seats = Vec::new();
        for (index, p) in spec.participants.iter().enumerate() {
            let faction = p.faction.clone().unwrap_or_default();
            if data.players.get(&faction).is_none() {
                return Err(format!("wotr: no PlayerTemplate named {faction:?}"));
            }
            let start = if p.defender {
                0
            } else {
                u8::try_from(u32::from(p.player.0).saturating_add(1)).unwrap_or(u8::MAX)
            };
            let seat = seat_of(p, index, spec, data, &faction);
            players.push(PlayerSeat {
                player: seat.player,
                template: faction,
                team: u8::try_from(p.team).ok(),
                colour: p.player.0,
                handicap: 0,
                at: StartAt::Waypoint(start),
                ai: (p.kind == PlayerKind::Computer).then(|| difficulty_of(p.skill)),
                hero: None,
                starting: StartingEntities::default(),
                cash: Some(i64::from(p.cash)),
                command_points: u32::try_from(p.command_points).ok(),
            });
            seats.push(seat);
        }
        let skill_scalars = data
            .experience_scalars
            .table(SKILL_SCALAR_TABLE)
            .map(<[f32]>::to_vec)
            .unwrap_or_default();
        Ok(Self {
            mode: crate::game_mode::GameMode::Skirmish,
            map_label: setup.map_label.clone(),
            players,
            rules: MatchRules::default(),
            difficulty: MatchDifficulty::Multiplayer,
            carried_in: None,
            deploy: DevDeploy {
                drive_templates: setup.drive_templates.clone(),
                ..DevDeploy::default()
            },
            wotr: Some(WotrBattle {
                region: spec.region_name.clone(),
                neighbours: spec.neighbours.clone(),
                seats,
                skill_scalars,
                seed: spec.seed,
                two_by_one: spec.sides.len() == 2
                    && spec.sides.iter().all(|s| s.players.len() == 1),
                revival_multipliers: spec.revival_multipliers,
            }),
        })
    }
}

fn point(objects: &[Object], name: &str) -> Option<[f32; 3]> {
    mint_sage_map::named_waypoint(objects, name).map(|at| [at.x, at.y, 0.0])
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}

fn spawn_and_gather(
    battle: &WotrBattle,
    seat: &BattleSeat,
    army: &BattleArmy,
    objects: &[Object],
    unowned_pair: Option<(&str, &str)>,
    side: usize,
) -> Option<([f32; 3], [f32; 3], bool)> {
    let reinforcement = army.arrives_after.is_some();
    let region_pair = |region: &str| {
        Some((
            point(objects, &format!("WOTRSpawnPoint_{region}_1"))?,
            point(objects, &format!("WOTRGatherPoint_{region}_1"))?,
            false,
        ))
    };
    let owner_start = || {
        if reinforcement {
            return region_pair("DefenderReinforcements");
        }
        let start = point(objects, "Player_1_Start")?;
        let rally =
            point(objects, "Player_1_Start_Rally").unwrap_or([start[0] + 10.0, start[1], 0.0]);
        Some((start, rally, true))
    };
    let against_owner = || {
        let rally =
            point(objects, "Player_1_Start_Rally").or_else(|| point(objects, "Player_1_Start"))?;
        let farthest = battle
            .neighbours
            .iter()
            .filter_map(|r| Some((r, point(objects, &format!("WOTRGatherPoint_{r}_1"))?)))
            .fold(None::<(&String, f32)>, |best, (r, at)| {
                let d = distance2(at, rally);
                match best {
                    Some((_, held)) if held >= d => best,
                    _ => Some((r, d)),
                }
            })?;
        region_pair(farthest.0)
    };
    if battle.two_by_one {
        if let Some((a, b)) = unowned_pair {
            return region_pair(if side == 0 { a } else { b });
        }
        return if seat.owner_side {
            owner_start()
        } else {
            against_owner()
        };
    }
    if army.origin.eq_ignore_ascii_case(&battle.region) {
        return owner_start();
    }
    if let Some(pair) = region_pair(&army.origin).filter(|_| !army.origin.is_empty()) {
        return Some(pair);
    }
    if seat.owner_side {
        region_pair("DefenderReinforcements")
    } else {
        against_owner()
    }
}

fn build_plot(objects: &[Object], plot: u8) -> Option<[f32; 3]> {
    if let Some(at) = point(objects, &format!("Player_1_BuildPlot_{plot}")) {
        return Some(at);
    }
    let start = point(objects, "Player_1_Start")?;
    let (dx, dy) = match plot {
        1 => (0.0, PLOT_SPACING),
        2 => (PLOT_SPACING, 0.0),
        3 => (0.0, -PLOT_SPACING),
        4 => (-PLOT_SPACING, 0.0),
        _ => return None,
    };
    Some([start[0] + dx, start[1] + dy, start[2]])
}

fn farthest_pair<'a>(battle: &'a WotrBattle, objects: &[Object]) -> Option<(&'a str, &'a str)> {
    let points: Vec<(&str, [f32; 3])> = battle
        .neighbours
        .iter()
        .filter_map(|r| {
            Some((
                r.as_str(),
                point(objects, &format!("WOTRGatherPoint_{r}_1"))?,
            ))
        })
        .collect();
    let mut best: Option<(&str, &str, f32)> = None;
    for (i, (a, pa)) in points.iter().enumerate() {
        for (b, pb) in points.iter().skip(i.saturating_add(1)) {
            let d = distance2(*pa, *pb);
            if best.is_none_or(|(_, _, held)| d > held) {
                best = Some((a, b, d));
            }
        }
    }
    best.map(|(a, b, _)| (a, b))
}

pub(crate) fn resolve(battle: &WotrBattle, objects: &[Object]) -> WotrStart {
    let unowned = !battle.seats.iter().any(|s| s.owner);
    let pair = if unowned && battle.two_by_one {
        farthest_pair(battle, objects)
    } else {
        None
    };
    let fallback = point(objects, "Player_2_Start").unwrap_or([0.0; 3]);
    let ticks_per_second = mint_sim::SIMULATION_HZ;
    let seats = battle
        .seats
        .iter()
        .enumerate()
        .map(|(side, seat)| {
            let armies = seat
                .armies
                .iter()
                .map(|army| {
                    let reinforcement = army.arrives_after.is_some();
                    let placed = spawn_and_gather(battle, seat, army, objects, pair, side);
                    let (stand, face, walk_to) = match placed {
                        Some((start, rally, true)) => (start, rally, None),
                        Some((spawn, gather, false)) if reinforcement => {
                            let ahead = [
                                spawn[0] + (gather[0] - spawn[0]),
                                spawn[1] + (gather[1] - spawn[1]),
                                0.0,
                            ];
                            (spawn, ahead, Some(Vec2::new(gather[0], gather[1])))
                        }
                        Some((spawn, gather, false)) => {
                            let (dx, dy) = (gather[0] - spawn[0], gather[1] - spawn[1]);
                            let length = (dx * dx + dy * dy).sqrt().max(1.0);
                            let face = [
                                gather[0] + 10.0 * dx / length,
                                gather[1] + 10.0 * dy / length,
                                0.0,
                            ];
                            (gather, face, None)
                        }
                        None => (fallback, [fallback[0] + 10.0, fallback[1], 0.0], None),
                    };
                    WotrArmy {
                        army: army.army,
                        units: army.units.clone(),
                        arrives: army
                            .arrives_after
                            .map_or(0, |s| s.saturating_mul(ticks_per_second)),
                        stand,
                        face,
                        walk_to,
                    }
                })
                .collect();
            let has_start = point(objects, "Player_1_Start").is_some();
            let structures = seat
                .structures
                .iter()
                .filter_map(|(plot, template, army)| {
                    let at = build_plot(objects, *plot).filter(|_| has_start);
                    if at.is_none() {
                        println!("wotr: no Player_1_BuildPlot_{plot} for {template} - not placed");
                    }
                    let at = at?;
                    Some(WotrStructure {
                        template: template.clone(),
                        army: *army,
                        at: Vec2::new(at[0], at[1]),
                    })
                })
                .collect();
            WotrSeat {
                player: seat.player,
                armies,
                structures,
                modifiers: Some(seat.modifiers.clone()),
                purchase_points: seat.purchase_points,
                skill_points: seat.skill_points,
                sciences: seat.sciences.clone(),
                upgrades: seat.upgrades.clone(),
                fallen: seat.fallen.clone(),
            }
        })
        .collect();
    WotrStart {
        seats,
        skill_scalars: battle.skill_scalars.clone(),
        revival_multipliers: battle.revival_multipliers,
    }
}

pub(crate) fn homes(start: &WotrStart) -> Vec<(PlayerId, Vec2)> {
    start
        .seats
        .iter()
        .filter_map(|seat| {
            let army = seat.armies.first()?;
            Some((seat.player, Vec2::new(army.stand[0], army.stand[1])))
        })
        .collect()
}

pub(crate) fn battle_result(
    rules: &RotwkRules,
    world: &World,
    spec: &BattleSpec,
    strategic: &Rules,
    winner: Option<u8>,
) -> BattleResult {
    let Some(run) = rules.wotr_run(world) else {
        return BattleResult::default();
    };
    let mut rosters: Vec<(ArmyId, Vec<Member>)> = Vec::new();
    let mut index = 0usize;
    for p in &spec.participants {
        for army in &p.armies {
            if run.arrived.get(index).copied().unwrap_or(false) {
                rosters.push((army.army, Vec::new()));
            }
            index = index.saturating_add(1);
        }
    }
    let mut next_id = 0u32;
    for (entity, army) in &run.members {
        let Some((template, level, experience, summary)) =
            rules.army_summary_object(world, *entity)
        else {
            continue;
        };
        if !summary {
            continue;
        }
        let Some(unit) = strategic.unit_id(&template) else {
            continue;
        };
        if let Some((_, roster)) = rosters.iter_mut().find(|(a, _)| a.0 == *army) {
            next_id = next_id.saturating_add(1);
            roster.push(Member {
                id: next_id,
                unit,
                level,
                count: 1,
                experience,
                battles: 0,
                kills: 0,
                kills_battle: run.kills_of(*entity),
                upgraded_at: None,
            });
        }
    }
    let mut fallen = Vec::new();
    for (index, p) in spec.participants.iter().enumerate() {
        let player = PlayerId::new(u8::try_from(index).unwrap_or(u8::MAX));
        for (name, level, experience) in rules.fallen_heroes(world, player) {
            if let Some(unit) = strategic.unit_id(&name) {
                fallen.push((
                    p.player,
                    FallenHero {
                        unit,
                        level,
                        experience,
                    },
                ));
            }
        }
    }
    let players = spec
        .participants
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            let player = PlayerId::new(u8::try_from(index).unwrap_or(u8::MAX));
            let carry = rules.wotr_player_carry(world, player)?;
            Some((
                p.player,
                PlayerCarry {
                    skill_points: carry.skill_points,
                    rank_points: carry.rank_points,
                    sciences: carry.sciences,
                    upgrades: carry.upgrades,
                },
            ))
        })
        .collect();
    BattleResult {
        winner,
        rosters,
        fallen_heroes: fallen,
        fallen_records: Vec::new(),
        players,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalEnd {
    ByStanding,
    Won,
    Lost,
}

pub(crate) fn winning_side(
    spec: &BattleSpec,
    end: LocalEnd,
    standing: &dyn Fn(mint_sage_livingworld::PlayerId) -> bool,
) -> Option<u8> {
    let local = spec.participants.first().map(|p| p.player);
    let local_side = spec
        .sides
        .iter()
        .position(|side| local.is_some_and(|p| side.players.contains(&p)));
    let stands = |side: &mint_sage_livingworld::SpecSide| side.players.iter().any(|p| standing(*p));
    let index = match end {
        LocalEnd::Won => local_side,
        LocalEnd::ByStanding => spec.sides.iter().position(stands),
        LocalEnd::Lost => spec
            .sides
            .iter()
            .enumerate()
            .position(|(i, side)| Some(i) != local_side && stands(side)),
    };
    index.and_then(|i| u8::try_from(i).ok())
}

fn map_label(name: &str) -> String {
    format!("maps/{name}/{name}.map")
}

pub(crate) fn battle_setup(spec: &BattleSpec, map: &str) -> crate::lobby::SkirmishSetup {
    let mut setup = crate::lobby::SkirmishSetup::two_player(
        String::new(),
        String::new(),
        map_label(map),
        [false, false],
        None,
    );
    setup.slots.clear();
    setup.resolved = Some(
        spec.participants
            .iter()
            .enumerate()
            .map(|(slot, p)| crate::lobby::ResolvedPlayer {
                slot,
                player: PlayerId::new(u8::try_from(slot).unwrap_or(u8::MAX)),
                faction: p.faction.clone().unwrap_or_default(),
                hero_pick: crate::lobby::HeroPick::None,
                hero: None,
                team: u8::try_from(p.team).ok(),
                colour: p.player.0,
                handicap: 0,
                start: 0,
                ai: (p.kind == PlayerKind::Computer).then(|| difficulty_of(p.skill)),
            })
            .collect(),
    );
    setup.wotr = Some(SpecHandle(std::sync::Arc::new(spec.clone())));
    setup
}

pub(crate) struct DevBattle {
    pub(crate) state: mint_sage_livingworld::LivingWorldState,
    pub(crate) spec: BattleSpec,
}

pub(crate) fn dev_battle(
    vfs: &mint_vfs::Vfs,
    data: &GameData,
    index: usize,
    region: &str,
) -> Result<DevBattle, String> {
    use mint_sage_livingworld::{
        apply, Catalog, Command, Event, LivingWorldState, PlayerId as Lw, PlayerSetup, PriceClass,
        Setup, UnitInfo,
    };
    let mut sources = mint_sage_ini::MemorySources::new();
    for path in vfs.paths_under("data/ini/") {
        if let Ok(bytes) = vfs.read_path(path) {
            sources.insert(path.as_str(), bytes);
        }
    }
    let load = mint_sage_ini::living_world::load_living_world(&sources);
    #[allow(clippy::cast_possible_truncation)]
    let units = data
        .templates
        .iter()
        .map(|record| {
            let hero = record.has_kind("HERO");
            let mut info = UnitInfo {
                command_points: record.command_points.unwrap_or(0.0) as i32,
                build_cost: record.build_cost.unwrap_or(0.0) as i32,
                price_class: if hero {
                    PriceClass::Hero
                } else {
                    PriceClass::Other
                },
                hero,
                ..UnitInfo::default()
            };
            if let Some(name) = &record.auto_resolve_unit_type {
                info.set_resolve_type(name);
            }
            (record.name.to_ascii_lowercase(), info)
        })
        .collect();
    let catalog = Catalog {
        units,
        resource_multiplier_limit: 1.0,
        ..Catalog::default()
    };
    let campaign = load
        .data
        .campaigns
        .iter()
        .filter(|c| !c.scenarios.is_empty())
        .nth(index)
        .ok_or_else(|| format!("no campaign {index} with a scenario"))?;
    let templates: Vec<String> = load
        .data
        .player_templates
        .iter()
        .filter(|t| t.faction.is_some() && t.starting_world_cp.is_some())
        .map(|t| t.name.clone())
        .collect();
    let seat = |i: usize, kind| PlayerSetup {
        template: templates
            .get(i.checked_rem(templates.len()).unwrap_or(0))
            .cloned()
            .unwrap_or_default(),
        kind,
        team: i32::try_from(i).unwrap_or(0),
        start_region: None,
        skill: mint_sage_livingworld::Skill::Medium,
    };
    let seats = campaign
        .scenarios
        .first()
        .and_then(|s| s.min_players)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(2)
        .max(2);
    let mut setup = Setup {
        campaign: campaign.name.clone(),
        scenario: 0,
        victory_type: 0,
        battle_type: mint_sage_livingworld::screen_queries::BattleType::default(),
        seed: 1,
        players: (0..seats)
            .map(|i| {
                seat(
                    i,
                    if i == 0 {
                        PlayerKind::Human
                    } else {
                        PlayerKind::Computer
                    },
                )
            })
            .collect(),
    };
    crate::wotr_setup::give_start_regions(&load.data, &catalog, &mut setup)
        .map_err(|e| e.to_string())?;
    for (i, seat) in setup.players.iter_mut().enumerate() {
        seat.team = i32::try_from(i).unwrap_or(0);
    }
    let mut state =
        LivingWorldState::start(&load.data, &catalog, &setup).map_err(|e| e.to_string())?;
    let (attacker, defender) = (Lw(0), Lw(1));
    let target = state.rules.region(region).map_err(|e| e.to_string())?;
    let from = state
        .rules
        .region_rules(target)
        .and_then(|r| r.neighbours.first().copied())
        .ok_or("the region has no neighbour")?;
    for (id, owner) in [(target, defender), (from, attacker)] {
        if let Some(r) = state.regions.get_mut(id.index()) {
            r.owner = Some(owner);
            r.enabled = true;
        }
    }
    let army = state
        .armies_of(attacker)
        .find(|a| a.is_hero_army())
        .map(|a| a.id)
        .ok_or("the first seat has no hero army")?;
    let held = state.armies_of(defender).next().map(|a| a.id);
    for (id, at) in [(Some(army), from), (held, target)] {
        if let Some(a) = id.and_then(|id| state.armies.get_mut(&id)) {
            a.region = at;
            a.prev_region = Some(at);
        }
    }
    let run = |state: &mut LivingWorldState, p, command: Command| {
        let named = format!("{command:?}");
        let phase = state.phase;
        apply(state, p, command).map_err(|e| format!("{e:?} for {named} in {phase:?}"))
    };
    run(
        &mut state,
        attacker,
        Command::MoveArmy { army, dest: target },
    )?;
    for p in 0..seats {
        if state.phase != mint_sage_livingworld::Phase::Planning {
            break;
        }
        let p = Lw(u8::try_from(p).unwrap_or(u8::MAX));
        if state.player(p).is_some_and(|x| !x.defeated) {
            run(&mut state, p, Command::EndPhase)?;
        }
    }
    let events = run(
        &mut state,
        attacker,
        Command::ResolveBattle {
            region: target,
            mode: mint_sage_livingworld::BattleMode::RealTime,
        },
    )?;
    let spec = events
        .into_iter()
        .find_map(|e| match e {
            Event::BattleSpec(spec) => Some(spec),
            _ => None,
        })
        .ok_or("no battle in the region")?;
    for p in &spec.participants {
        let armies: Vec<String> = p
            .armies
            .iter()
            .map(|a| {
                format!(
                    "army {} hero {:?} units {} arrives {:?}{}",
                    a.army.0,
                    a.hero,
                    a.units.iter().map(|u| u.count).sum::<u32>(),
                    a.arrives_after,
                    if a.primary { " primary" } else { "" }
                )
            })
            .collect();
        println!(
            "wotr: spec map {:?} region {} {} {:?} cash {} cp {} structures {} bonus units {:?}: {}",
            spec.map,
            spec.region_name,
            p.side_name,
            p.faction,
            p.cash,
            p.command_points,
            p.structures.len(),
            p.bonus_units,
            armies.join("; ")
        );
    }
    Ok(DevBattle { state, spec })
}

pub(crate) fn apply_dev_result(game: &crate::game::Game, dev: &DevBattle) {
    let result = game.wotr_battle_result(&dev.spec, &dev.state.rules);
    let survivors: Vec<String> = result
        .rosters
        .iter()
        .map(|(army, roster)| {
            format!(
                "army {} {}",
                army.0,
                roster.iter().map(|m| m.count).sum::<u32>()
            )
        })
        .collect();
    println!(
        "wotr: result winner {:?}; survivors {}; fallen heroes {}",
        result.winner,
        survivors.join(", "),
        result.fallen_heroes.len()
    );
    let mut state = dev.state.clone();
    let issuer = dev
        .spec
        .participants
        .first()
        .map_or(mint_sage_livingworld::PlayerId(0), |p| p.player);
    match mint_sage_livingworld::apply(
        &mut state,
        issuer,
        mint_sage_livingworld::Command::ApplyBattleResult {
            region: dev.spec.region,
            result,
        },
    ) {
        Ok(events) => {
            for event in &events {
                println!("wotr: ApplyBattleResult {event:?}");
            }
            println!(
                "wotr: strategic turn {} phase {:?} hash {:#018x}",
                state.turn,
                state.phase,
                state.state_hash()
            );
        }
        Err(error) => println!("wotr: ApplyBattleResult refused: {error:?}"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod winner_tests {
    use super::*;
    use mint_sage_livingworld::{PlayerId as Lw, RegionId, SpecSide};

    fn spec() -> BattleSpec {
        let side = |p| SpecSide {
            players: vec![Lw(p)],
            armies: Vec::new(),
        };
        let participant = |p| Participant {
            player: Lw(p),
            side_name: String::new(),
            faction: None,
            team: i32::from(p),
            kind: PlayerKind::Human,
            defender: false,
            cash: 0,
            command_points: 0,
            skill: Skill::Medium,
            skill_points: 0.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            armies: Vec::new(),
            structures: Vec::new(),
            bonus: mint_sage_livingworld::Bonus::default(),
            legendary: 0,
            bonus_units: Vec::new(),
            fallen: Vec::new(),
        };
        BattleSpec {
            battle: 1,
            region: RegionId(0),
            owner: None,
            sides: vec![side(1), side(0)],
            map: None,
            region_name: String::new(),
            neighbours: Vec::new(),
            participants: vec![participant(0), participant(1)],
            seed: 0,
            weapon_scale: Vec::new(),
            round_cap: None,
            revival_multipliers: (1.0, 1.0),
        }
    }

    #[test]
    fn a_seat_that_quits_or_is_beaten_loses_though_it_stands_first() {
        let spec = spec();
        let everyone = |_: Lw| true;
        let table = [
            (LocalEnd::ByStanding, Some(0)),
            (LocalEnd::Won, Some(1)),
            (LocalEnd::Lost, Some(0)),
        ];
        for (end, side) in table {
            assert_eq!(winning_side(&spec, end, &everyone), side, "{end:?}");
        }
        let mut local_first = spec;
        local_first.sides.reverse();
        assert_eq!(
            winning_side(&local_first, LocalEnd::Lost, &everyone),
            Some(1)
        );
    }
}
