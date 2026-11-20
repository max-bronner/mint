use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_math::rng::Pcg32;
use mint_sage_big::BigArchive;
use mint_sage_ini::living_world::load_living_world;
use mint_sage_ini::MemorySources;
use mint_sage_livingworld::battle::{AutoResolve, BattleSpec};
use mint_sage_livingworld::harness::{play_with_ai, AiRun};
use mint_sage_livingworld::model::BattleResult;
use mint_sage_livingworld::stand_in::{catalog_from_object_ini, HeadCount};
use mint_sage_livingworld::{
    assign_start_regions, LivingWorldState, PlayerKind, PlayerSetup, Setup, StrategicAi,
};

struct Args {
    install: PathBuf,
    also: Option<PathBuf>,
    scenario: usize,
    players: usize,
    seed: u64,
    turns: u32,
    victory: usize,
    openings: bool,
    human: Option<usize>,
    swap: bool,
    factions: Vec<String>,
    resolver: Pick,
}

#[derive(Clone, Copy)]
enum Pick {
    First,
    Last,
    Coin,
}

impl AutoResolve for Pick {
    fn resolve(&mut self, spec: &BattleSpec, rng: &mut Pcg32) -> BattleResult {
        let sides = u8::try_from(spec.sides.len()).unwrap_or(u8::MAX).max(1);
        let winner = match self {
            Pick::First => 0,
            Pick::Last => sides.saturating_sub(1),
            Pick::Coin => u8::try_from(rng.below(u32::from(sides))).unwrap_or(0),
        };
        BattleResult {
            winner: Some(winner),
            ..BattleResult::default()
        }
    }
}

fn parse_args() -> Option<Args> {
    let mut args = Args {
        install: PathBuf::new(),
        also: None,
        scenario: 0,
        players: 2,
        seed: 1,
        turns: 10,
        victory: 0,
        openings: false,
        human: None,
        swap: false,
        factions: Vec::new(),
        resolver: Pick::Last,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--scenario" => args.scenario = it.next()?.parse().ok()?,
            "--players" => args.players = it.next()?.parse().ok()?,
            "--seed" => args.seed = it.next()?.parse().ok()?,
            "--turns" => args.turns = it.next()?.parse().ok()?,
            "--victory" => args.victory = it.next()?.parse().ok()?,
            "--openings" => args.openings = true,
            "--human" => args.human = Some(it.next()?.parse().ok()?),
            "--swap" => args.swap = true,
            "--also" => args.also = Some(PathBuf::from(it.next()?)),
            "--resolver" => {
                args.resolver = match it.next()?.as_str() {
                    "first" => Pick::First,
                    "last" => Pick::Last,
                    "coin" => Pick::Coin,
                    _ => return None,
                }
            }
            "--factions" => args.factions = it.next()?.split(',').map(str::to_owned).collect(),
            other => args.install = PathBuf::from(other),
        }
    }
    (!args.install.as_os_str().is_empty()).then_some(args)
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        eprintln!("usage: wotr_autoplay <install dir> [--scenario N] [--players N] [--seed N] [--turns N] [--victory N] [--openings]");
        return ExitCode::FAILURE;
    };
    let mut sources = MemorySources::new();
    let mut object_texts: Vec<String> = Vec::new();
    let mut roots: Vec<&Path> = args.also.iter().map(PathBuf::as_path).collect();
    roots.push(&args.install);
    for path in roots.into_iter().flat_map(big_archives) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        for entry in archive.entries() {
            let name = entry.path.as_str();
            if !name.starts_with("data/ini/") {
                continue;
            }
            if let Ok(bytes) = archive.read_entry(entry) {
                if name.contains("/object/") || name.contains("/gamedata") {
                    object_texts.push(String::from_utf8_lossy(&bytes).into_owned());
                }
                sources.insert(name, bytes);
            }
        }
    }
    let load = load_living_world(&sources);
    println!("wotr: load errors {}", load.errors.len());
    let catalog = catalog_from_object_ini(object_texts.iter().map(String::as_str), &load.data);
    println!(
        "wotr: catalog {} templates, {} read build cost 0, {} read command points 0",
        catalog.units.len(),
        catalog.units.values().filter(|u| u.build_cost == 0).count(),
        catalog
            .units
            .values()
            .filter(|u| u.command_points == 0)
            .count()
    );
    let campaigns: Vec<_> = load
        .data
        .campaigns
        .iter()
        .filter(|c| !c.scenarios.is_empty())
        .collect();
    println!("wotr: {} campaigns hold a scenario", campaigns.len());
    let templates: Vec<String> = load
        .data
        .player_templates
        .iter()
        .filter(|t| t.faction.is_some() && t.starting_world_cp.is_some())
        .map(|t| t.name.clone())
        .collect();
    println!("wotr: templates {templates:?}");
    let seats = |count: usize| -> Vec<PlayerSetup> {
        (0..count)
            .map(|i| PlayerSetup {
                template: args
                    .factions
                    .get(i)
                    .cloned()
                    .or_else(|| {
                        templates
                            .get(if args.swap {
                                count.saturating_sub(1).saturating_sub(i) % templates.len().max(1)
                            } else {
                                i % templates.len().max(1)
                            })
                            .cloned()
                    })
                    .unwrap_or_default(),
                kind: if args.human == Some(i) {
                    PlayerKind::Human
                } else {
                    PlayerKind::Computer
                },
                team: i32::try_from(i).unwrap_or(0),
                start_region: None,
                skill: mint_sage_livingworld::Skill::Medium,
            })
            .collect()
    };
    let setup_for = |index: usize, players: usize| {
        let mut setup = Setup {
            campaign: campaigns
                .get(index)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            scenario: 0,
            victory_type: args.victory,
            battle_type: mint_sage_livingworld::screen_queries::BattleType::default(),
            seed: args.seed,
            players: seats(players),
        };
        let _ = assign_start_regions(&load.data, &catalog, &mut setup);
        setup
    };

    if args.openings {
        let mut refused = 0usize;
        for (index, campaign) in campaigns.iter().enumerate() {
            let max = campaign.scenarios.first().and_then(|s| s.max_players);
            let players = usize::try_from(max.unwrap_or(2)).unwrap_or(2);
            match LivingWorldState::start(&load.data, &catalog, &setup_for(index, players)) {
                Ok(state) => println!(
                    "wotr: opening scenario {index} players {players} hash 0x{:016x} regions {:?}",
                    state.state_hash(),
                    owned(&state)
                ),
                Err(error) => {
                    refused += 1;
                    println!("wotr: opening scenario {index} players {players} refused {error}");
                }
            }
        }
        println!("wotr: openings refused {refused} of {}", campaigns.len());
        return ExitCode::SUCCESS;
    }

    let mut state = match LivingWorldState::start(
        &load.data,
        &catalog,
        &setup_for(args.scenario, args.players),
    ) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("setup refused: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "wotr: campaign {} scenario {} players {} seed {} opening 0x{:016x} regions {:?} armies {:?}",
        campaigns.get(args.scenario).map_or("", |c| c.name.as_str()),
        args.scenario,
        args.players,
        args.seed,
        state.state_hash(),
        owned(&state),
        state
            .players
            .iter()
            .map(|p| state.armies_of(p.id).count())
            .collect::<Vec<_>>()
    );
    let mut log = Vec::new();
    let mut resolver = args.resolver;
    let mut ai = StrategicAi::for_slots(&state, false);
    let mut predictor = HeadCount;
    let mut rejected = 0usize;
    for _ in 0..args.turns {
        if state.ended.is_some() {
            break;
        }
        let run = AiRun {
            ai: &mut ai,
            predictor: &mut predictor,
        };
        let outcome = play_with_ai(&mut state, &mut resolver, run, 1, &mut log);
        rejected += outcome.rejected;
        for (player, command, reason, phase) in &outcome.refused {
            println!(
                "wotr: refused turn {} {} player {} {command:?}: {reason:?}",
                state.turn,
                phase.name(),
                player.0
            );
        }
        for b in outcome.boundaries {
            println!(
                "wotr: turn {} phase {} hash 0x{:016x}",
                b.turn,
                b.phase.name(),
                b.hash
            );
        }
        println!(
            "wotr: turn {} end regions {:?} resources {:?}",
            state.turn,
            owned(&state),
            state
                .players
                .iter()
                .map(|p| p.resources)
                .collect::<Vec<_>>()
        );
    }
    println!(
        "wotr: teams {:?} templates {:?} regions {:?}",
        state.players.iter().map(|p| p.team).collect::<Vec<_>>(),
        state.players.iter().map(|p| p.template).collect::<Vec<_>>(),
        owned(&state)
    );
    println!(
        "wotr: summary turn {} commands {} rejected {} ended {:?} final 0x{:016x}",
        state.turn,
        log.len(),
        rejected,
        state.ended,
        state.state_hash()
    );
    println!("wotr: ai skipped {:?}", ai.skipped);
    println!("wotr: duplicate heroes {}", print_heroes(&state));
    ExitCode::SUCCESS
}

fn print_heroes(state: &LivingWorldState) -> usize {
    let mut repeats = 0usize;
    for player in &state.players {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for army in state.armies_of(player.id) {
            let Some(hero) = army.hero else { continue };
            let name = state.rules.unit_name(hero);
            match counts.iter_mut().find(|(n, _)| *n == name) {
                Some((_, count)) => *count += 1,
                None => counts.push((name, 1)),
            }
        }
        repeats += counts
            .iter()
            .map(|(_, c)| c.saturating_sub(1))
            .sum::<usize>();
        println!("wotr: heroes player {} {counts:?}", player.id.0);
    }
    repeats
}

fn owned(state: &LivingWorldState) -> Vec<i32> {
    state
        .players
        .iter()
        .map(|p| state.regions_owned(p.id))
        .collect()
}
