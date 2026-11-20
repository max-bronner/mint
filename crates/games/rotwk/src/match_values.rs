use std::sync::Arc;

use mint_sage_campaign::{CapturedObject, Carryover, MissionEnd, PlayerCapture};
use mint_sage_ini::{PlayerTemplateDef, StartingUnit};
use mint_sage_map::{Object, Side};
use mint_sim::PlayerId;
use mint_vfs::Vfs;

use crate::game::LOCAL_PLAYER;
use crate::game_data::GameData;
use crate::lobby::{CarriedHero, Difficulty, MatchRules, SkirmishSetup};
use crate::rules::difficulty::{DifficultyPlayer, DifficultySetup};
use crate::rules::map_scripts::MapScriptEngine;

#[derive(Debug, Clone)]
pub(crate) struct MatchStart {
    pub(crate) mode: crate::game_mode::GameMode,
    pub(crate) map_label: String,
    pub(crate) players: Vec<PlayerSeat>,
    pub(crate) rules: MatchRules,
    pub(crate) difficulty: MatchDifficulty,
    pub(crate) carried_in: Option<CarriedIn>,
    pub(crate) deploy: DevDeploy,
    pub(crate) wotr: Option<crate::wotr_battle::WotrBattle>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlayerSeat {
    pub(crate) player: PlayerId,
    pub(crate) template: String,
    pub(crate) team: Option<u8>,
    pub(crate) colour: u8,
    pub(crate) handicap: u8,
    pub(crate) at: StartAt,
    pub(crate) ai: Option<Difficulty>,
    pub(crate) hero: Option<CarriedHero>,
    pub(crate) starting: StartingEntities,
    pub(crate) cash: Option<i64>,
    pub(crate) command_points: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartAt {
    Waypoint(u8),
    MapSide(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StartingEntities {
    pub(crate) building: Option<String>,
    pub(crate) units: Vec<StartingUnit>,
}

impl StartingEntities {
    pub(crate) fn of_template(template: &PlayerTemplateDef) -> Self {
        Self {
            building: template.starting_building.clone(),
            units: template.starting_units.clone(),
        }
    }

    pub(crate) fn applied_to(&self, template: &PlayerTemplateDef) -> PlayerTemplateDef {
        PlayerTemplateDef {
            starting_building: self.building.clone(),
            starting_units: self.units.clone(),
            ..template.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchDifficulty {
    Multiplayer,
    SinglePlayer(Difficulty),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CarriedIn {
    pub(crate) roster: Carryover,
    pub(crate) delayed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DevDeploy {
    pub(crate) army: Option<u32>,
    pub(crate) spawn: Option<String>,
    pub(crate) spawn_buildings: Option<String>,
    pub(crate) dummies: Option<String>,
    pub(crate) dummies_attack: bool,
    pub(crate) drive_templates: Vec<String>,
}

impl MatchStart {
    pub(crate) fn from_lobby(
        setup: &SkirmishSetup,
        objects: &[Object],
        data: &GameData,
        vfs: &Vfs,
    ) -> Result<Self, String> {
        let waypoints = crate::skirmish::start_waypoints(objects);
        let catalogue = crate::lobby::Catalogue {
            factions: crate::lobby::playable_factions(data),
            random_pool: crate::lobby::random_pool(data),
            colours: u8::try_from(data.multiplayer_colors.len()).unwrap_or(u8::MAX),
            starts: u8::try_from(waypoints.len()).unwrap_or(u8::MAX),
            heroes: crate::setup_heroes::load_entries(vfs, &data.hero_subclasses),
            sides: crate::lobby::template_sides(data),
        };
        let mut players = setup
            .resolved_players(&catalogue, &waypoints)
            .map_err(|error| format!("skirmish setup: {error}"))?;
        if let (Some(id), Some(first)) = (&setup.cah_hero, players.first_mut()) {
            if first.hero.is_none() {
                first.hero = catalogue
                    .heroes
                    .iter()
                    .find(|entry| {
                        entry
                            .record
                            .id_str()
                            .is_some_and(|name| name.eq_ignore_ascii_case(id))
                    })
                    .map(|entry| CarriedHero::new(Arc::clone(&entry.record)));
                if first.hero.is_none() {
                    let ids: Vec<String> = catalogue
                        .heroes
                        .iter()
                        .filter_map(|entry| entry.record.id_str())
                        .collect();
                    println!("cah: --cah-hero {id} is not a stored hero; ids: {ids:?}");
                }
            }
        }
        let own_dir = crate::setup_heroes::own_dir();
        for seat in &mut players {
            seat.hero = seat.hero.take().map(|hero| hero.with_stats_from(&own_dir));
        }
        Ok(Self {
            mode: crate::game_mode::GameMode::Skirmish,
            map_label: setup.map_label.clone(),
            players: players
                .into_iter()
                .map(|p| PlayerSeat {
                    player: p.player,
                    starting: data
                        .players
                        .get(&p.faction)
                        .map(StartingEntities::of_template)
                        .unwrap_or_default(),
                    template: p.faction,
                    team: p.team,
                    colour: p.colour,
                    handicap: p.handicap,
                    at: StartAt::Waypoint(p.start),
                    ai: p.ai,
                    hero: p.hero,
                    cash: None,
                    command_points: None,
                })
                .collect(),
            rules: setup.rules.clone(),
            difficulty: MatchDifficulty::Multiplayer,
            carried_in: None,
            deploy: DevDeploy {
                army: setup.army,
                spawn: setup.spawn.clone(),
                spawn_buildings: setup.spawn_buildings.clone(),
                dummies: setup.dummies.clone(),
                dummies_attack: setup.dummies_attack,
                drive_templates: setup.drive_templates.clone(),
            },
            wotr: None,
        })
    }

    pub(crate) fn for_mission(
        mission: &crate::campaign::CampaignMatch,
        sides: &[Side],
        data: &GameData,
    ) -> Result<Self, String> {
        let side = crate::campaign::local_side(sides)
            .and_then(|at| sides.get(at))
            .ok_or("campaign: the map declares no player side")?;
        let template = side.text("playerFaction").unwrap_or_default().to_owned();
        if data.players.get(&template).is_none() {
            return Err(format!("campaign: no PlayerTemplate named {template}"));
        }
        Ok(Self {
            mode: mission.mode,
            map_label: mission.map.clone(),
            players: vec![PlayerSeat {
                player: LOCAL_PLAYER,
                template,
                team: None,
                colour: 0,
                handicap: 0,
                at: StartAt::MapSide(side.name().unwrap_or_default().to_owned()),
                ai: None,
                hero: None,
                starting: StartingEntities::default(),
                cash: None,
                command_points: None,
            }],
            rules: MatchRules::default(),
            difficulty: MatchDifficulty::SinglePlayer(mission.difficulty),
            carried_in: Some(CarriedIn {
                roster: mission.carryover.clone(),
                delayed: mission.delayed.clone(),
            }),
            deploy: DevDeploy {
                drive_templates: mission.drive_templates.clone(),
                ..DevDeploy::default()
            },
            wotr: None,
        })
    }

    pub(crate) fn scenery() -> Self {
        Self {
            mode: crate::game_mode::GameMode::Skirmish,
            map_label: String::new(),
            players: Vec::new(),
            rules: MatchRules::default(),
            difficulty: MatchDifficulty::Multiplayer,
            carried_in: None,
            deploy: DevDeploy::default(),
            wotr: None,
        }
    }
}

pub(crate) struct MapFacts<'a> {
    pub(crate) objects: &'a [Object],
    pub(crate) sides: &'a [Side],
    pub(crate) scripts: Option<MapScriptEngine>,
    pub(crate) script_lines: Vec<String>,
    pub(crate) referenced_unit_names: &'a std::collections::BTreeSet<String>,
    pub(crate) snowy: bool,
}

pub(crate) fn inputs(
    start: &MatchStart,
    data: &GameData,
    map: MapFacts<'_>,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<crate::skirmish::Inputs, String> {
    let scripts = map.scripts.map(|engine| match &start.carried_in {
        Some(carried) => engine.with_carryover(carryover_start(carried, map.objects)),
        None => engine,
    });
    let wotr = start
        .wotr
        .as_ref()
        .map(|battle| crate::wotr_battle::resolve(battle, map.objects));
    let homes = wotr
        .as_ref()
        .map(crate::wotr_battle::homes)
        .unwrap_or_default();
    let scripts = scripts.map(|engine| match &wotr {
        Some(start) => engine.with_wotr(start.clone()),
        None => engine,
    });
    let mut inputs = crate::skirmish::resolve(
        data,
        map.objects,
        read,
        &start.players,
        &start.map_label,
        scripts,
        map.script_lines,
        start.rules.allow_ring_heroes,
        &homes,
    )?;
    inputs.snowy = map.snowy;
    inputs.teams = start.players.iter().map(|p| p.team).collect();
    let named: Vec<(String, PlayerId)> = start
        .players
        .iter()
        .filter_map(|p| match &p.at {
            StartAt::MapSide(name) => Some((name.clone(), p.player)),
            StartAt::Waypoint(_) => None,
        })
        .collect();
    inputs.map_cast = crate::map_players::cast(
        &data.templates,
        map.sides,
        map.objects,
        &inputs.starts,
        &named,
        map.referenced_unit_names,
    );
    if named.is_empty() {
        let occupied: Vec<PlayerId> = start.players.iter().map(|p| p.player).collect();
        inputs.map_cast.rebuild_creep_relations(&occupied);
    }
    for seat in &start.players {
        if let StartAt::MapSide(_) = seat.at {
            let start = map_side_start(data, seat, &inputs.map_cast, map.objects)?;
            inputs.players.push(start);
        }
    }
    inputs.heroes = start
        .players
        .iter()
        .filter_map(|p| Some((p.player, p.hero.clone()?)))
        .collect();
    inputs.handicaps = start
        .players
        .iter()
        .map(|p| data.handicaps.get(p.handicap))
        .collect();
    inputs.mode = start.mode;
    inputs.starting_money = Some(start.rules.starting_money);
    inputs.command_point_factor = Some(start.rules.command_point_factor);
    inputs.cash = start.players.iter().map(|p| p.cash).collect();
    inputs.command_points = start.players.iter().map(|p| p.command_points).collect();
    inputs.defeat_mode = crate::defeat::DefeatMode::of_match(start.wotr.is_some());
    inputs.ai_difficulty = start
        .players
        .iter()
        .filter_map(|p| p.ai.map(|difficulty| (p.player, difficulty)))
        .collect();
    inputs.forfeit = start
        .wotr
        .as_ref()
        .and(data.wotr_retreat)
        .filter(|tuning| !tuning.disable_tactical_ai);
    inputs.difficulty = difficulty_setup(start, data, &inputs.map_cast);
    let deploy = &start.deploy;
    inputs.test_seeds = [&deploy.spawn, &deploy.dummies]
        .into_iter()
        .flatten()
        .flat_map(|spec| {
            crate::spawn::named_templates(spec, |name| data.templates.get(name).is_some())
        })
        .chain(
            deploy
                .drive_templates
                .iter()
                .filter(|name| data.templates.get(name).is_some())
                .cloned(),
        )
        .collect();
    Ok(inputs)
}

fn carryover_start(
    carried: &CarriedIn,
    objects: &[Object],
) -> crate::rules::carryover::CarryoverStart {
    let start_name = mint_sage_map::start_waypoint_name(0);
    let point =
        |name: &str| mint_sage_map::named_waypoint(objects, name).map(|at| [at.x, at.y, 0.0]);
    crate::rules::carryover::CarryoverStart {
        carry: carried.roster.clone(),
        delayed: carried.delayed.clone(),
        start: point(&start_name),
        rally: point(&format!("{start_name}_Rally")),
    }
}

fn map_side_start(
    data: &GameData,
    seat: &PlayerSeat,
    cast: &crate::map_players::MapCast,
    objects: &[Object],
) -> Result<crate::skirmish::PlayerStart, String> {
    let template = data
        .players
        .get(&seat.template)
        .ok_or_else(|| format!("campaign: no PlayerTemplate named {}", seat.template))?;
    let at = cast
        .entities
        .iter()
        .find(|e| matches!(e.owner, crate::map_players::Relation::Contestant(_)))
        .map(|e| e.at)
        .or_else(|| objects.first().map(|o| mint_movement::Vec2::new(o.x, o.y)))
        .unwrap_or_default();
    Ok(crate::skirmish::PlayerStart {
        player: seat.player,
        faction: seat.starting.applied_to(template),
        at,
        castle: None,
        ai_base_candidates: Vec::new(),
    })
}

fn difficulty_setup(
    start: &MatchStart,
    data: &GameData,
    cast: &crate::map_players::MapCast,
) -> DifficultySetup {
    let playable = |template: &str| {
        data.players
            .iter()
            .any(|t| t.name == template && t.playable == Some(true))
    };
    let mut players: Vec<DifficultyPlayer> = start
        .players
        .iter()
        .map(|p| DifficultyPlayer {
            player: p.player,
            human: p.ai.is_none(),
            playable: playable(&p.template),
            ai: p.ai,
        })
        .collect();
    let (multiplayer, game) = match start.difficulty {
        MatchDifficulty::Multiplayer => (true, Difficulty::Medium),
        MatchDifficulty::SinglePlayer(game) => {
            for (offset, map_player) in cast.players.iter().enumerate() {
                let Some(id) = offset
                    .checked_add(start.players.len())
                    .and_then(|id| u8::try_from(id).ok())
                else {
                    break;
                };
                players.push(DifficultyPlayer {
                    player: PlayerId::new(id),
                    human: false,
                    playable: playable(&map_player.faction),
                    ai: None,
                });
            }
            (false, game)
        }
    };
    DifficultySetup {
        multiplayer,
        game,
        players,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MatchResult {
    pub(crate) end: Option<MissionEnd>,
    pub(crate) roster: Option<Carryover>,
    pub(crate) survivors: Vec<CapturedObject>,
    pub(crate) players: Vec<PlayerResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayerResult {
    pub(crate) player: PlayerId,
    pub(crate) fallen_heroes: Vec<CapturedObject>,
    pub(crate) state: Option<PlayerCapture>,
}

impl MatchResult {
    pub(crate) fn carryover(&self) -> Option<Carryover> {
        let mut roster = self.roster.clone()?;
        let local = self.players.iter().find(|p| p.player == LOCAL_PLAYER);
        roster.capture(
            &self.survivors,
            local.map_or(&[], |p| p.fallen_heroes.as_slice()),
            local.and_then(|p| p.state.as_ref()),
        );
        Some(roster)
    }
}
