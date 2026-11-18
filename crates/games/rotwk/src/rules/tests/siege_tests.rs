use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
SpecialPower ZDeploy\n  ReloadTime = 0\nEnd\n\
SpecialPower ZToggleDeploy\n  ReloadTime = 0\nEnd\n\
Weapon ZRam\n  AttackRange = 60\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SIEGE\n  End\nEnd\n\
CommandSet ZEngineSet\n  1 = ZFireDeploy\nEnd\n\
CommandSet ZRamSet\n  1 = ZFireToggleDeploy\nEnd\n\
CommandButton ZFireDeploy\n  Command = SPECIAL_POWER\n  SpecialPower = ZDeploy\n  \
  Options = NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT\nEnd\n\
CommandButton ZFireToggleDeploy\n  Command = SPECIAL_POWER\n  SpecialPower = ZToggleDeploy\nEnd\n\
Object ZWall\n  Side = Neutral\n  KindOf = STRUCTURE IMMOBILE WALK_ON_TOP_OF_WALL SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 5000\n  End\n  \
  Behavior = SiegeDockingBehavior Tag_Dock\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 50\n  GeometryMinorRadius = 10\n  GeometryHeight = 40\nEnd\n\
Object ZTower\n  Side = Men\n  KindOf = SELECTABLE SIEGE_TOWER\n  CommandSet = ZEngineSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = SiegeAIUpdate Tag_AI\n  End\n  \
  Behavior = SiegeEngineContain Tag_Crew\n    CrewMax = 1\n    \
    CrewFilter = NONE +ZPassenger\n    InitialCrew = ZPassenger 1\n  End\n  \
  Behavior = SiegeDeploySpecialPower Tag_Deploy\n    SpecialPowerTemplate = ZDeploy\n    \
    LowerDelay = 1000\n    RaiseDelay = 1000\n    SkipAdjustPosition = Yes\n    AwayFromWallWaitDist = 0\n    WallSearchDistance = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 30\nEnd\n\
Object ZKeep\n  Side = Neutral\n  KindOf = STRUCTURE IMMOBILE SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 5000\n  End\n  \
  Behavior = SiegeDockingBehavior Tag_Dock\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object ZPassenger\n  Side = Men\n  KindOf = SELECTABLE INFANTRY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 20\nEnd\n\
Object ZRam\n  Side = Men\n  KindOf = SELECTABLE\n  CommandSet = ZRamSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZRam\n  End\n  \
  Behavior = DeployStyleAIUpdate Tag_AI\n    MustDeployToAttack = Yes\n    UnpackTime = 2000\n    \
    PackTime = 2000\n  End\n  \
  Behavior = ToggleDeploySpecialAbilityUpdate Tag_Toggle\n    SpecialPowerTemplate = ZToggleDeploy\n    \
    SoundDeploy = ZSoundDeploy\n    SoundUndeploy = ZSoundUndeploy\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 30\nEnd\n\
Object ZTarget\n  Side = Mordor\n  KindOf = SELECTABLE STRUCTURE IMMOBILE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n";

const WALL_AT: Vec2 = Vec2::new(300.0, 300.0);

fn simulation() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene(None)
}

fn scene(keep: Option<(u16, u16)>) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_with_slots(
        keep,
        vec![crate::rules::siege::DockBone {
            ladder: false,
            at: Vec2::new(0.0, -40.0),
            height: 0.0,
            axis: Facing::from_vector(Vec2::new(0.0, -1.0)).unwrap(),
        }],
    )
}

fn scene_with_slots(
    keep: Option<(u16, u16)>,
    wall_slots: Vec<crate::rules::siege::DockBone>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_with(keep, wall_slots, true)
}

fn scene_with(
    keep: Option<(u16, u16)>,
    wall_slots: Vec<crate::rules::siege::DockBone>,
    walled: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZWall", "ZTower", "ZRam", "ZTarget", "ZKeep"];
    let seeds: Vec<String> = roots.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawns: Vec<Spawn> = [
        ("ZWall", WALL_AT, OTHER),
        ("ZTower", Vec2::new(300.0, 150.0), PLAYER),
        ("ZRam", Vec2::new(500.0, 500.0), PLAYER),
        ("ZTarget", Vec2::new(540.0, 500.0), OTHER),
    ]
    .into_iter()
    .chain(keep.map(|(x, y)| {
        let at = Vec2::new(f32::from(x) * 10.0, f32::from(y) * 10.0);
        ("ZKeep", at, OTHER)
    }))
    .map(|(name, at, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    })
    .collect();
    let mut docks = vec![Vec::new(); kinds.len()];
    docks[usize::from(kinds.kind("ZWall").unwrap().0)] = wall_slots;
    let mut above = vec![None; kinds.len()];
    above[usize::from(kinds.kind("ZTower").unwrap().0)] = Some(
        crate::rules::siege::AboveWall::Bone(Vec2::new(35.0, 0.0), 35.0),
    );
    let mut ladder_exit = vec![None; kinds.len()];
    ladder_exit[usize::from(kinds.kind("ZTower").unwrap().0)] = Some(crate::perches::Perch {
        position: [0.0, 4.0, 20.0],
        facing: Facing::EAST,
    });
    let mut wall_bounds = vec![None; kinds.len()];
    if walled {
        wall_bounds[usize::from(kinds.kind("ZWall").unwrap().0)] =
            Some(crate::wall_meshes::WallBounds {
                perimeter: vec![(-50.0, -10.0), (50.0, -10.0), (50.0, 10.0), (-50.0, 10.0)],
                height: 40.0,
            });
    }
    let mut map = flat_map(96, 96);
    if let Some((cx, cy)) = keep {
        let (cx, cy) = (u32::from(cx), u32::from(cy));
        for y in cy - 14..=cy + 14 {
            for x in (cx - 14..=cx + 14).step_by(2) {
                map.samples[(y * map.width + x) as usize] = 400;
            }
        }
    }
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_structure_footprints(vec![(0, Vec::new())])
        .with_wall_bounds(wall_bounds)
        .with_siege_bones(docks, above, ladder_exit);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let mut entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    entities.sort_unstable_by_key(|entity| entity.index());
    simulation.tick().unwrap();
    (simulation, ids, entities)
}

fn holds(simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str) -> bool {
    simulation
        .rules()
        .modules
        .as_ref()
        .and_then(|wiring| simulation.state().get(wiring.state, entity))
        .is_some_and(|state| {
            state
                .model_condition_names()
                .iter()
                .any(|held| held.eq_ignore_ascii_case(flag))
        })
}

fn select(simulation: &mut Simulation<RotwkRules>, unit: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
}

#[test]
fn a_siege_engine_docks_at_a_wall_slot_and_then_refuses_a_move() {
    let (mut simulation, ids, entities) = simulation();
    let (wall, tower) = (entities[0], entities[1]);
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    let mut deployed = false;
    for _ in 0..900 {
        simulation.tick().unwrap();
        if holds(&simulation, tower, "DEPLOYED") {
            deployed = true;
            break;
        }
    }
    assert!(deployed, "the tower deploys at the wall");
    let at = simulation.state().get(ids.body, tower).unwrap().0.position;
    assert!(
        (at.x - WALL_AT.x).abs() < 6.0 && (at.y - 260.0).abs() < 6.0,
        "it stands at the slot's approach point: {at:?}"
    );
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 100.0,
                y: 100.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().get(ids.path, tower).is_none(),
        "a deployed engine takes no move order"
    );
}

#[test]
fn an_engine_that_must_deploy_unpacks_before_it_fires() {
    let (mut simulation, ids, entities) = simulation();
    let (ram, target) = (entities[2], entities[3]);
    select(&mut simulation, ram);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let health = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, target)
            .unwrap()
            .0
            .fraction()
    };
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(holds(&simulation, ram, "UNPACKING"), "it unpacks first");
    assert!(
        (health(&simulation) - 1.0).abs() < f32::EPSILON,
        "and fires nothing meanwhile"
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(holds(&simulation, ram, "DEPLOYED"));
    assert!(health(&simulation) < 1.0, "deployed, it fires");
}

#[test]
fn a_deployed_siege_tower_passenger_exits_at_the_ladder_bone_on_layer_17() {
    let (mut simulation, ids, entities) = simulation();
    let (wall, tower) = (entities[0], entities[1]);
    let passenger = simulation.state().get(ids.passengers, tower).unwrap().0[0];
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    let mut deployed = false;
    for _ in 0..900 {
        simulation.tick().unwrap();
        if holds(&simulation, tower, "DEPLOYED") {
            deployed = true;
            break;
        }
    }
    assert!(deployed, "the tower deploys at the wall");
    for _ in 0..5 {
        if !simulation.state().contains(ids.contained, passenger) {
            break;
        }
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.contained, passenger),
        "EvacuatePassengersOnDeploy evacuates it"
    );
    let layer = simulation.state().get(ids.layer, passenger).copied();
    assert!(
        layer.is_some_and(crate::wall_layer::Layer::is_wall_top),
        "layer 17, not the ordinary exit's ground layer: {layer:?}"
    );
    let tower_body = simulation.state().get(ids.body, tower).unwrap().0;
    let (sin, cos) = mint_math::f32::sin_cos(tower_body.facing.radians());
    let expected = Vec2::new(
        tower_body.position.x - 4.0 * sin,
        tower_body.position.y + 4.0 * cos,
    );
    let at = simulation
        .state()
        .get(ids.body, passenger)
        .unwrap()
        .0
        .position;
    assert!(
        (at.x - expected.x).abs() < 1.0 && (at.y - expected.y).abs() < 1.0,
        "stands at the Ladder04 bone {expected:?}, not the ordinary exit's point: {at:?}"
    );
}

#[test]
fn a_wall_attacker_aims_at_a_deployed_tower_along_its_above_wall_bearing() {
    let (mut simulation, ids, entities) = simulation();
    let (wall, tower, ram) = (entities[0], entities[1], entities[2]);
    let passenger = simulation.state().get(ids.passengers, tower).unwrap().0[0];
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    for _ in 0..900 {
        simulation.tick().unwrap();
        if holds(&simulation, tower, "DEPLOYED") {
            break;
        }
    }
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let rules = simulation.rules();
    let world = simulation.state();
    let goal = rules
        .siege_tower_wall_goal(world, ids, passenger, tower)
        .expect("a wall attacker gets a goal");
    let body = world.get(ids.body, tower).unwrap().0;
    let along = goal.minus(body.position);
    let facing = body.facing.to_vector();
    assert!(
        along.length() > 8.0 && along.normalised().unwrap().minus(facing).length() < 0.01,
        "goal {along:?} off the tower's bearing {facing:?}"
    );
    let step = 8.0 * std::f32::consts::FRAC_1_SQRT_2;
    let steps = along.length() / step;
    assert!(
        (steps - steps.round()).abs() < 1e-3,
        "{} is not a whole number of {step}-long steps",
        along.length()
    );
    assert!(rules
        .siege_tower_wall_goal(world, ids, ram, tower)
        .is_none());
}

#[test]
fn a_passenger_of_an_undeployed_engine_uses_the_ordinary_exit() {
    let (mut simulation, ids, entities) = simulation();
    let tower = entities[1];
    let passenger = simulation.state().get(ids.passengers, tower).unwrap().0[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Exit {
                passenger: Some(passenger),
            },
        )
        .unwrap();
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.contained, passenger),
        "left the tower"
    );
    let layer = simulation.state().get(ids.layer, passenger).copied();
    assert!(
        !layer.is_some_and(crate::wall_layer::Layer::is_wall_top),
        "the ordinary exit gives no wall-top layer: {layer:?}"
    );
}

#[test]
fn a_siege_engine_sent_at_a_full_wall_walks_as_close_as_it_can() {
    let (mut simulation, ids, entities) = scene(Some((70, 70)));
    let tower = entities[1];
    let keep = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(_, kind)| simulation.rules().kinds.get(**kind).unwrap().record.name == "ZKeep")
        .map(|(entity, _)| entity)
        .unwrap();
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(keep),
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().get(ids.path, tower).is_some(),
        "the tower sets off"
    );
    for _ in 0..1500 {
        simulation.tick().unwrap();
    }
    let at = simulation.state().get(ids.body, tower).unwrap().0.position;
    assert!(
        simulation.state().get(ids.path, tower).is_none(),
        "the walk ends at the cliffs' edge rather than holding blocked: at {at:?}"
    );
    assert!(
        at.minus(Vec2::new(700.0, 700.0)).length() < 260.0,
        "it ends at the cliffs, not short of them: {at:?}"
    );
}

#[test]
fn a_siege_deploy_at_a_structure_that_docks_nothing_does_nothing() {
    let (mut simulation, ids, entities) = simulation();
    let (tower, target) = (entities[1], entities[3]);
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 100.0,
                y: 150.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(target),
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let state = simulation.state();
    assert!(
        state.get(ids.siege_deploy, tower).is_none(),
        "no deploy state"
    );
    assert!(
        state.contains(ids.player_order, tower),
        "the move order stands"
    );
    let goal = state.get(ids.path, tower).map(|path| path.goal);
    assert!(
        goal.is_some_and(|goal| goal.x < 200.0),
        "still walking the ordered move: {goal:?}"
    );
}

#[test]
fn toggling_a_deployed_engine_plays_sound_undeploy_not_sound_deploy() {
    let (mut simulation, _ids, entities) = simulation();
    let ram = entities[2];
    let press = |simulation: &mut Simulation<RotwkRules>| {
        select(simulation, ram);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::FireSpecialPower {
                    button: 0,
                    target: None,
                    at: None,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    };
    press(&mut simulation);
    let mut deployed = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if holds(&simulation, ram, "DEPLOYED") {
            deployed = true;
            break;
        }
    }
    assert!(deployed, "the ram finishes deploying");
    press(&mut simulation);
    let frame = Frame::new(simulation.frame().get() - 1);
    let events = simulation
        .rules()
        .voice_events(simulation.state(), frame)
        .to_vec();
    assert!(
        events.iter().any(|event| {
            event.kind == crate::voice::VoiceKind::ObjectSound
                && event.entity == ram
                && event.sound.as_deref() == Some("ZSoundUndeploy")
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| event.sound.as_deref() == Some("ZSoundDeploy")),
        "only the undeploy branch ran: {events:?}"
    );
}

#[test]
fn toggling_mid_unpack_requests_and_plays_nothing() {
    let (mut simulation, _ids, entities) = simulation();
    let ram = entities[2];
    select(&mut simulation, ram);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        holds(&simulation, ram, "UNPACKING"),
        "the first press starts the unpack"
    );
    select(&mut simulation, ram);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        holds(&simulation, ram, "UNPACKING"),
        "a second press mid-unpack changes nothing"
    );
    let frame = Frame::new(simulation.frame().get() - 1);
    let events = simulation
        .rules()
        .voice_events(simulation.state(), frame)
        .to_vec();
    assert!(
        !events
            .iter()
            .any(|event| event.kind == crate::voice::VoiceKind::ObjectSound),
        "{events:?}"
    );
}

#[test]
fn a_reservation_sets_the_deploying_status() {
    let (mut simulation, ids, entities) = simulation();
    let (wall, tower) = (entities[0], entities[1]);
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert!(simulation
        .rules()
        .carries_object_status(simulation.state(), tower, "DEPLOYING"));
    assert_eq!(
        simulation
            .state()
            .get(ids.dock_occupants, wall)
            .map(|held| held.held_by(tower)),
        Some(1)
    );
}

#[test]
fn a_high_slot_bone_shortens_the_horizontal_push() {
    let n = crate::rules::siege::push_direction(Vec2::new(10.0, 0.0), 30.0).unwrap();
    assert!((n.x * 5.0 - 5.0 / 10.0f32.sqrt()).abs() < 1e-4, "{n:?}");
}

#[test]
fn a_footprint_without_a_wall_layer_stamp_does_not_pass_the_portal_test() {
    let (mut simulation, _, entities) = scene_with(
        None,
        vec![crate::rules::siege::DockBone {
            ladder: false,
            at: Vec2::new(0.0, -40.0),
            height: 0.0,
            axis: Facing::from_vector(Vec2::new(0.0, -1.0)).unwrap(),
        }],
        false,
    );
    let (wall, tower) = (entities[0], entities[1]);
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
        assert!(
            !holds(&simulation, tower, "DEPLOYED"),
            "docked on a footprint"
        );
    }
}

#[test]
fn an_unwritten_above_wall_passes_the_dock_test() {
    let text = format!(
        "{DATA}Object ZPortalTower\n  KindOf = SELECTABLE SIEGE_TOWER\n  \
         Behavior = SiegeDeploySpecialPower Tag_Deploy\n    SpecialPowerTemplate = ZDeploy\n  End\n  \
         Behavior = DynamicPortalBehaviour Tag_Portal\n    BonePrefix = ZBone\n    NumberOfBones = 1\n    \
         WayPoint = Index:0 Type:Walk\n  End\nEnd\n"
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec!["ZPortalTower".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let above = crate::skirmish::siege_above_wall(&data, &kinds, &|_, _| Vec::new());
    let tower = usize::from(kinds.kind("ZPortalTower").unwrap().0);
    assert!(matches!(
        above.get(tower),
        Some(Some(crate::rules::siege::AboveWall::Unlisted))
    ));
}

#[test]
fn the_nearest_slot_is_measured_in_three_dimensions() {
    let bone = |x: f32, height: f32| crate::rules::siege::DockBone {
        ladder: false,
        at: Vec2::new(x, -40.0),
        height,
        axis: Facing::from_vector(Vec2::new(0.0, -1.0)).unwrap(),
    };
    let (mut simulation, ids, entities) =
        scene_with_slots(None, vec![bone(0.0, 100.0), bone(60.0, 0.0)]);
    let (wall, tower) = (entities[0], entities[1]);
    select(&mut simulation, tower);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(wall),
                at: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.dock_occupants, wall)
            .and_then(|held| held.slot_of(tower)),
        Some(1)
    );
}
