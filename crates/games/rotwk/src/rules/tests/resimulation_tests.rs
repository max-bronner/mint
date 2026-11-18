use super::guard_tests::DATA;
use super::*;

const HORDES: u32 = 6;
const TICKS: usize = 300;

fn fight() -> Vec<u64> {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Archer", "Quarry", "PikeHorde", "PorcupineHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let mut spawns = Vec::new();
    for index in 0..12u8 {
        let (template, x) = if index % 3 == 0 {
            ("Spearman", 520.0)
        } else {
            ("Archer", 640.0)
        };
        let at = Vec2::new(x, 150.0 + f32::from(index) * 45.0);
        spawns.push(
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(template).unwrap(),
                    at,
                    Facing::EAST,
                    OTHER,
                )
                .unwrap(),
        );
    }
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "PikeHorde")
        .unwrap()
        .index;
    for (player, x) in [(PLAYER, 300.0), (OTHER, 760.0)] {
        for row in 0..HORDES {
            let y = 160.0 + row as f32 * 90.0;
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
                .unwrap();
        }
    }
    simulation.tick().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .to_vec();
    let enemies: Vec<Entity> = simulation
        .state()
        .store(ids.armed)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .filter(|e| {
            simulation.state().get(ids.owner, *e).map(|o| o.0) == Some(OTHER)
                && !simulation.state().contains(ids.member, *e)
        })
        .collect();
    let stances = [
        mint_sage_ini::Stance::HoldGround,
        mint_sage_ini::Stance::Battle,
        mint_sage_ini::Stance::Aggressive,
    ];
    for (index, horde) in hordes.iter().enumerate() {
        let player = simulation.state().get(ids.owner, *horde).map(|o| o.0);
        let player = player.unwrap_or(PLAYER);
        simulation
            .submit(
                player,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![*horde],
                },
            )
            .unwrap();
        simulation
            .submit(
                player,
                RotwkCommand::SetStance {
                    stance: stances[index % stances.len()].index(),
                },
            )
            .unwrap();
        if index % 2 == 1 && player == PLAYER {
            simulation
                .submit(
                    player,
                    RotwkCommand::AttackObject {
                        target: enemies[index % enemies.len()],
                    },
                )
                .unwrap();
        }
    }
    (0..TICKS)
        .map(|_| {
            simulation.tick().unwrap();
            simulation.state_hash()
        })
        .collect()
}

#[test]
fn the_fight_hashes_the_same_every_tick_at_one_thread_and_at_several() {
    let serial = mint_pool::with_threads(1, fight).unwrap();
    let changed = serial.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert!(
        changed > TICKS / 2,
        "the fight barely moves: {changed} ticks changed"
    );
    for threads in [2, 3, 8] {
        let parallel = mint_pool::with_threads(threads, fight).unwrap();
        let first = serial.iter().zip(&parallel).position(|(a, b)| a != b);
        assert_eq!(first, None, "{threads} threads: first differing tick");
    }
}
