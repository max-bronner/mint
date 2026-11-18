use std::sync::Arc;
use std::time::Instant;

use mint_app::WindowConfig;
use mint_render::Renderer;
use mint_sim::{Entity, PlayerId, Simulation};
use mint_vfs::Vfs;

use crate::game::{build_match, MatchConfig, PassabilitySources};
use crate::rules::{Ids, RotwkCommand, RotwkRules};
use crate::scene::load_match_scene;

const HORDE_SPACING: f32 = 20.0;
const MAX_SPAWN_HORDES: u32 = 2000;
const SPAWN_ROW: u32 = 10;
const ENGAGE_GAP: f32 = 120.0;
const CLASH_SECONDS: f64 = 30.0;

pub(crate) fn run_stress(
    vfs: &Arc<Vfs>,
    map_bytes: &[u8],
    map_label: String,
    total_units: u32,
    options: &super::Options,
) -> Result<(), String> {
    let mut renderer = Renderer::offscreen(
        WindowConfig::default().width,
        WindowConfig::default().height,
    )
    .map_err(|e| e.to_string())?;
    let (mut data, line) = crate::load_game_data_or_exit(vfs);
    println!("{line}");
    let sources = match options.passability.as_deref() {
        Some(raw) => PassabilitySources::parse(raw)?,
        None => PassabilitySources::default(),
    };
    let config = MatchConfig::Skirmish(crate::lobby::SkirmishSetup::two_player(
        options
            .faction
            .clone()
            .unwrap_or_else(|| String::from(crate::skirmish::DEFAULT_FACTION)),
        options
            .opponent
            .clone()
            .unwrap_or_else(|| String::from(crate::skirmish::DEFAULT_OPPONENT)),
        map_label.clone(),
        [false, false],
        None,
    ));
    crate::game_data::compose_for_match(&mut data, vfs, &map_label)?;
    let object_index = data.object_index.take();
    let (skirmish, allow_ring_heroes) = config.scene_filter();
    let scene = load_match_scene(
        &mut renderer,
        vfs,
        map_bytes,
        map_label,
        &data.templates,
        skirmish,
        allow_ring_heroes,
        object_index,
        None,
    )?;
    let setup = build_match(&scene, &data, vfs, &config, sources)?;
    let mut simulation = setup.simulation;
    let ids = simulation
        .rules()
        .ids()
        .ok_or("the ruleset registered no components")?;
    let players = simulation.rules().player_count();
    if players < 2 {
        return Err("--stress needs at least two player slots".to_string());
    }
    let (Some(&start0), Some(&start1)) = (setup.starts.first(), setup.starts.get(1)) else {
        return Err("the map resolved fewer than two player starts".to_string());
    };

    let per_side = total_units / 2;
    println!(
        "stress: target {total_units} units ({per_side} a side) near ({:.0},{:.0}) / ({:.0},{:.0})",
        start0.x, start0.y, start1.x, start1.y
    );

    spawn_side(
        &mut simulation,
        ids,
        PlayerId::new(0),
        (start0.x, start0.y),
        per_side,
    )?;
    let rows = hordes_owned_by(simulation.state(), ids, PlayerId::new(0))
        .len()
        .div_ceil(SPAWN_ROW as usize);
    #[allow(clippy::cast_precision_loss)]
    let second_row_y = start0.y + rows as f32 * HORDE_SPACING + ENGAGE_GAP;
    spawn_side(
        &mut simulation,
        ids,
        PlayerId::new(1),
        (start0.x, second_row_y),
        per_side,
    )?;

    let side0 = hordes_owned_by(simulation.state(), ids, PlayerId::new(0));
    let side1 = hordes_owned_by(simulation.state(), ids, PlayerId::new(1));
    let counts = super::count_buildings_and_army(simulation.state(), ids, players);
    let (army0, army1) = (
        counts.first().map_or(0, |c| c.1),
        counts.get(1).map_or(0, |c| c.1),
    );
    println!(
        "stress: spawned {} units (player0={army0} in {} hordes, player1={army1} in {} hordes)",
        army0 + army1,
        side0.len(),
        side1.len()
    );
    if side0.is_empty() || side1.is_empty() {
        return Err(
            "no hordes were spawned - this match's own `hordes` recipe table may be empty"
                .to_string(),
        );
    }

    let avoidance = crate::parse_avoidance(options.avoidance.as_deref())?;
    println!("stress: avoidance={avoidance:?}");
    simulation
        .submit(
            PlayerId::new(0),
            RotwkCommand::SetMovementModes(crate::unit::Modes {
                avoidance,
                ..crate::unit::Modes::default()
            }),
        )
        .map_err(|e| e.to_string())?;
    order_at_nearest(&mut simulation, ids, PlayerId::new(0), &side0, &side1)?;
    order_at_nearest(&mut simulation, ids, PlayerId::new(1), &side1, &side0)?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cap_frames = (CLASH_SECONDS * f64::from(mint_app::SIMULATION_HZ)) as u64;
    let mut max_step_ms = 0f64;
    let mut total_step_ms = 0f64;
    let mut fighting = FightingCount::default();
    for _ in 0..cap_frames {
        let tick_started = Instant::now();
        simulation.tick().map_err(|e| e.to_string())?;
        crate::perf::note_tick_returned();
        let tick_ms = tick_started.elapsed().as_secs_f64() * 1000.0;
        max_step_ms = max_step_ms.max(tick_ms);
        total_step_ms += tick_ms;
        fighting.note(members_holding_a_target(simulation.state(), ids));
    }
    #[allow(clippy::cast_precision_loss)]
    let mean_step_ms = total_step_ms / cap_frames as f64;
    let counts = super::count_buildings_and_army(simulation.state(), ids, players);
    println!(
        "stress: after {cap_frames} ticks - army p0={} p1={}",
        counts.first().map_or(0, |c| c.1),
        counts.get(1).map_or(0, |c| c.1)
    );
    println!(
        "stress: step cost over the run - mean {mean_step_ms:.3} ms, peak {max_step_ms:.3} ms, \
         {cap_frames} tick(s)"
    );
    println!("{}", fighting.summary(cap_frames));
    crate::perf::report();
    Ok(())
}

const BUILDABLE_PROBE_LIMIT: u16 = 400;

fn find_horde_recipe(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    player: PlayerId,
    origin: (f32, f32),
) -> Result<u16, String> {
    let mut first_any = None;
    for candidate in 0..BUILDABLE_PROBE_LIMIT {
        let before = hordes_owned_by(simulation.state(), ids, player);
        simulation
            .submit(
                player,
                RotwkCommand::DebugSpawnHorde {
                    recipe: candidate,
                    x: origin.0,
                    y: origin.1,
                    facing: 0.0,
                },
            )
            .map_err(|e| e.to_string())?;
        simulation.tick().map_err(|e| e.to_string())?;
        let after = hordes_owned_by(simulation.state(), ids, player);
        let Some(spawned) = after.iter().find(|horde| !before.contains(horde)) else {
            continue;
        };
        if simulation
            .rules()
            .is_melee_horde(simulation.state(), ids, *spawned)
        {
            return Ok(candidate);
        }
        first_any.get_or_insert(candidate);
    }
    first_any.ok_or_else(|| {
        format!(
            "player {} - no buildable among the first {BUILDABLE_PROBE_LIMIT} produces a horde",
            player.get()
        )
    })
}

fn army_of(simulation: &Simulation<RotwkRules>, ids: Ids, players: u8, player: PlayerId) -> u32 {
    super::count_buildings_and_army(simulation.state(), ids, players)
        .get(usize::from(player.get()))
        .map_or(0, |c| c.1)
}

fn spawn_side(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    player: PlayerId,
    origin: (f32, f32),
    target: u32,
) -> Result<(), String> {
    let recipe = find_horde_recipe(simulation, ids, player, origin)?;
    println!(
        "stress: player {} spawns from buildable {recipe}",
        player.get()
    );
    let players = simulation.rules().player_count();
    let mut spawned = 0u32;
    loop {
        let current = army_of(simulation, ids, players, player);
        if current >= target {
            break;
        }
        if spawned >= MAX_SPAWN_HORDES {
            println!(
                "stress: player {} only reached {current}/{target} units after {spawned} hordes \
                 from buildable {recipe}",
                player.get()
            );
            break;
        }
        let row = spawned / SPAWN_ROW;
        let col = spawned % SPAWN_ROW;
        #[allow(clippy::cast_precision_loss)]
        let x = origin.0 + (col as f32) * HORDE_SPACING;
        #[allow(clippy::cast_precision_loss)]
        let y = origin.1 + (row as f32) * HORDE_SPACING;
        simulation
            .submit(
                player,
                RotwkCommand::DebugSpawnHorde {
                    recipe,
                    x,
                    y,
                    facing: 0.0,
                },
            )
            .map_err(|e| e.to_string())?;
        simulation.tick().map_err(|e| e.to_string())?;
        spawned = spawned.saturating_add(1);
    }
    Ok(())
}

fn hordes_owned_by(world: &mint_sim::World, ids: Ids, player: PlayerId) -> Vec<Entity> {
    world
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&entity| {
            world
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player)
        })
        .collect()
}

fn members_holding_a_target(world: &mint_sim::World, ids: Ids) -> u32 {
    let held = world.store(ids.armed).map_or(0, |store| {
        store
            .iter()
            .filter(|(entity, armed)| {
                armed.target.is_some() && world.get(ids.member, *entity).is_some()
            })
            .count()
    });
    u32::try_from(held).unwrap_or(u32::MAX)
}

#[derive(Default)]
struct FightingCount {
    ticks_with_any: u64,
    sum: u64,
    peak: u32,
}

impl FightingCount {
    fn note(&mut self, holding: u32) {
        self.ticks_with_any += u64::from(holding > 0);
        self.sum += u64::from(holding);
        self.peak = self.peak.max(holding);
    }

    fn summary(&self, ticks: u64) -> String {
        #[allow(clippy::cast_precision_loss)]
        let mean = self.sum as f64 / ticks.max(1) as f64;
        format!(
            "stress: members holding a target - mean {mean:.1}, peak {}, {} of {ticks} ticks with any",
            self.peak, self.ticks_with_any
        )
    }
}

fn order_at_nearest(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    player: PlayerId,
    own: &[Entity],
    enemies: &[Entity],
) -> Result<(), String> {
    let position = |world: &mint_sim::World, entity: Entity| {
        world.get(ids.body, entity).map(|body| body.0.position)
    };
    for horde in own {
        let Some(at) = position(simulation.state(), *horde) else {
            continue;
        };
        let nearest = enemies
            .iter()
            .filter_map(|enemy| {
                let there = position(simulation.state(), *enemy)?;
                Some((there.minus(at).length_squared(), *enemy))
            })
            .fold(None, |best: Option<(f32, Entity)>, candidate| match best {
                Some(held) if held.0 <= candidate.0 => Some(held),
                _ => Some(candidate),
            });
        let Some((_, target)) = nearest else {
            continue;
        };
        simulation
            .submit(
                player,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![*horde],
                },
            )
            .map_err(|e| e.to_string())?;
        simulation
            .submit(player, RotwkCommand::AttackObject { target })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
