use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZCamo\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Camo\n    \
    InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      DetectionRange = 100\n      \
      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    UpdatePeriod = 1000\n    StartsActive = Yes\n  End\n  \
  VisionRange = 100\n  ShroudClearingRange = 100\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZCloak\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Cloak\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      ForbiddenConditions = MOVING\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = Yes\n  End\n  \
  VisionRange = 100\n  ShroudClearingRange = 100\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZWatcher\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 400\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = StealthDetectorUpdate Tag_Detect\n    DetectionRate = 500\n  End\n  \
  VisionRange = 300\n  ShroudClearingRange = 300\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Crowd {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    hiders: Vec<Entity>,
    watchers: Vec<Entity>,
}

fn crowd(columns: u32, rows: u32, watchers: u32) -> Crowd {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZCamo", "ZCloak", "ZWatcher"];
    let (kinds, missing) = KindTable::closure(
        &game,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let mut spawns = Vec::new();
    let mut place = |name: &str, at: Vec2, owner: PlayerId| {
        spawns.push(
            kinds
                .spawn(
                    &game.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap(),
        );
    };
    for row in 0..rows {
        for column in 0..columns {
            let at = Vec2::new(60.0 + 40.0 * column as f32, 60.0 + 40.0 * row as f32);
            let name = if (row + column) % 3 == 0 {
                "ZCloak"
            } else {
                "ZCamo"
            };
            place(name, at, PLAYER);
        }
    }
    for n in 0..watchers {
        place("ZWatcher", Vec2::new(70.0 + 130.0 * n as f32, 30.0), OTHER);
    }
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits));
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let split = entities.len() - watchers as usize;
    Crowd {
        simulation,
        ids,
        hiders: entities.get(..split).unwrap().to_vec(),
        watchers: entities.get(split..).unwrap().to_vec(),
    }
}

fn send_watchers(crowd: &mut Crowd, to_y: f32) {
    for (n, watcher) in crowd.watchers.clone().into_iter().enumerate() {
        crowd
            .simulation
            .submit(
                OTHER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![watcher],
                },
            )
            .unwrap();
        crowd
            .simulation
            .submit(
                OTHER,
                RotwkCommand::MoveTo {
                    x: 70.0 + 130.0 * n as f32,
                    y: to_y,
                    queue: false,
                },
            )
            .unwrap();
    }
}

fn send_hiders(crowd: &mut Crowd, every: usize, dy: f32) {
    for at in (3..crowd.hiders.len()).step_by(every) {
        let Some(hider) = crowd.hiders.get(at).copied() else {
            continue;
        };
        let Some(here) = crowd
            .simulation
            .state()
            .get(crowd.ids.body, hider)
            .map(|body| body.0.position)
        else {
            continue;
        };
        crowd
            .simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![hider],
                },
            )
            .unwrap();
        crowd
            .simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: here.x,
                    y: here.y + dy,
                    queue: false,
                },
            )
            .unwrap();
    }
}

fn fold(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

#[test]
fn the_pass_hides_and_reveals_a_crowd_on_the_same_frames_as_ever() {
    let mut crowd = crowd(10, 6, 4);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let (mut hides, mut reveals) = (0u32, 0u32);
    let mut last: Vec<(bool, u16)> = vec![(false, 0); crowd.hiders.len()];
    for frame in 0..900u32 {
        if frame % 240 == 0 {
            send_watchers(&mut crowd, if frame % 480 == 0 { 400.0 } else { 30.0 });
        }
        if frame == 140 || frame == 520 {
            send_hiders(&mut crowd, 7, if frame == 140 { 120.0 } else { -120.0 });
        }
        crowd.simulation.tick().unwrap();
        for (at, hider) in crowd.hiders.iter().enumerate() {
            let (world, rules) = (crowd.simulation.state(), crowd.simulation.rules());
            let now = (
                world
                    .get(crowd.ids.stealth, *hider)
                    .is_some_and(|s| s.hidden),
                u16::from(rules.stealth_look_hides(world, crowd.ids, OTHER, *hider)),
            );
            if last.get(at).copied() != Some(now) {
                if let Some(slot) = last.get_mut(at) {
                    if !slot.0 && now.0 {
                        hides += 1;
                    }
                    if slot.1 == 0 && now.1 != 0 {
                        reveals += 1;
                    }
                    *slot = now;
                }
                fold(&mut hash, u64::from(frame));
                fold(&mut hash, at as u64);
                fold(&mut hash, u64::from(now.0));
                fold(&mut hash, u64::from(now.1));
            }
        }
    }
    assert!(hides >= 60 && reveals >= 10, "{hides} {reveals}");
    assert_eq!(
        hash, 0xe898_02fa_14d2_a5fd,
        "{hides} hides, {reveals} reveals"
    );
}

#[test]
#[ignore = "measurement"]
fn the_pass_cost() {
    let mut crowd = crowd(20, 15, 8);
    for _ in 0..90 {
        crowd.simulation.tick().unwrap();
    }
    let (mut pass, mut other) = (Vec::new(), Vec::new());
    for frame in 0..900u32 {
        if frame % 240 == 0 {
            send_watchers(&mut crowd, if frame % 480 == 0 { 700.0 } else { 30.0 });
        }
        let started = std::time::Instant::now();
        crowd.simulation.tick().unwrap();
        let micros = started.elapsed().as_secs_f64() * 1e6;
        if crowd.simulation.frame().get() % 30 == 1 {
            pass.push(micros);
        } else {
            other.push(micros);
        }
    }
    let report = |label: &str, mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        println!(
            "{label}: n {} p50 {:.0} p99 {:.0} mean {:.0} us",
            v.len(),
            v[v.len() / 2],
            v[v.len() * 99 / 100],
            mean
        );
    };
    let mut all = pass.clone();
    all.extend(other.iter().copied());
    report("all", all);
    report("pass frames", pass);
    report("other frames", other);
}
