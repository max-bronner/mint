use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityApproach\n  ReloadTime = 30000\nEnd\n\
ModifierList Rush\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 30000\nEnd\n\
Object Approacher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Rush\n    SpecialPowerTemplate = SpecialAbilityApproach\n    \
    StartAbilityRange = 20\n    HeroAttributeModifier = Rush\n    HeroEffectDuration = 30000\n  End\n  \
  CommandSet = RushSet\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\n  \
  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Object LongRangeApproacher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Rush\n    SpecialPowerTemplate = SpecialAbilityApproach\n    \
    StartAbilityRange = 150\n    HeroAttributeModifier = Rush\n    HeroEffectDuration = 30000\n  End\n  \
  CommandSet = RushSet\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\n  \
  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Object BigTarget\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 60\n  GeometryHeight = 30\nEnd\n\
Object SmallTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Approacher\n  2 = Build_LongRangeApproacher\nEnd\n\
CommandButton Build_Approacher\n  Command = UNIT_BUILD\n  Object = Approacher\nEnd\n\
CommandButton Build_LongRangeApproacher\n  Command = UNIT_BUILD\n  Object = LongRangeApproacher\nEnd\n\
CommandSet RushSet\n  1 = Fire_Rush\nEnd\n\
CommandButton Fire_Rush\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityApproach\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

fn simulation_with_approacher_at(
    approacher_kind: &str,
    target_kind: &str,
    offset: Vec2,
) -> (Simulation<RotwkRules>, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Camp",
        "Approacher",
        "LongRangeApproacher",
        "BigTarget",
        "SmallTarget",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let target_at = Vec2::new(480.0, 480.0);
    let target = kinds
        .spawn(
            &data.combat,
            kinds.kind(target_kind).unwrap(),
            target_at,
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![target])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == approacher_kind)
        .unwrap()
        .index;
    let spawn_at = target_at.plus(offset);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnUnit {
                buildable: recipe,
                x: spawn_at.x,
                y: spawn_at.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let entities = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .entities()
        .to_vec();
    let target = entities[0];
    let approacher = entities[1];
    (simulation, approacher, target)
}

fn damage_mult(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(1.0, |bonuses| bonuses.0.damage_mult)
}

fn fire_and_wait(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    approacher: Entity,
    target: Entity,
    budget: u32,
) -> bool {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![approacher],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(target),
            at: None,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    for _ in 0..budget {
        simulation.tick().unwrap();
        if (damage_mult(simulation, ids, approacher) - 1.5).abs() < 1e-6 {
            return true;
        }
    }
    false
}

#[test]
fn a_lone_objects_ability_approach_fires_from_every_side() {
    for (name, offset) in [
        ("north", Vec2::new(0.0, 200.0)),
        ("south", Vec2::new(0.0, -200.0)),
        ("east", Vec2::new(200.0, 0.0)),
        ("west", Vec2::new(-200.0, 0.0)),
    ] {
        let (mut simulation, approacher, target) =
            simulation_with_approacher_at("Approacher", "BigTarget", offset);
        let ids = simulation.rules().ids().unwrap();
        assert!(
            fire_and_wait(&mut simulation, ids, approacher, target, 400),
            "approach from the {name} never triggered"
        );
    }
}

#[test]
fn a_lone_objects_ability_approach_reaches_a_small_target_from_every_side() {
    for (name, offset) in [
        ("north", Vec2::new(0.0, 300.0)),
        ("south", Vec2::new(0.0, -300.0)),
        ("east", Vec2::new(300.0, 0.0)),
        ("west", Vec2::new(-300.0, 0.0)),
    ] {
        let (mut simulation, approacher, target) =
            simulation_with_approacher_at("LongRangeApproacher", "SmallTarget", offset);
        let ids = simulation.rules().ids().unwrap();
        assert!(
            fire_and_wait(&mut simulation, ids, approacher, target, 400),
            "approach from the {name} never triggered"
        );
    }
}

#[test]
fn a_move_order_cancels_an_approaching_ability() {
    let (mut simulation, approacher, target) =
        simulation_with_approacher_at("Approacher", "BigTarget", Vec2::new(0.0, -200.0));
    let ids = simulation.rules().ids().unwrap();
    assert!(!fire_and_wait(&mut simulation, ids, approacher, target, 10));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 480.0,
                y: 180.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..400 {
        simulation.tick().unwrap();
        assert!((damage_mult(&simulation, ids, approacher) - 1.0).abs() < 1e-6);
    }
    let at = simulation
        .state()
        .get(ids.body, approacher)
        .unwrap()
        .0
        .position;
    assert!(at.y < 200.0, "walked to the order, not the target: {at:?}");
}

#[test]
fn an_order_before_the_ability_does_not_cancel_it() {
    let (mut simulation, approacher, target) =
        simulation_with_approacher_at("Approacher", "BigTarget", Vec2::new(0.0, -200.0));
    let ids = simulation.rules().ids().unwrap();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![approacher],
        },
        RotwkCommand::MoveTo {
            x: 380.0,
            y: 280.0,
            queue: false,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    assert!(fire_and_wait(&mut simulation, ids, approacher, target, 400));
}

#[test]
fn an_armed_object_button_refuses_a_target_its_relation_does_not_name() {
    let (mut simulation, approacher, target) =
        simulation_with_approacher_at("Approacher", "SmallTarget", Vec2::new(40.0, 0.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![approacher],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let enemies = crate::templates::target_relation(&["NEED_TARGET_ENEMY_OBJECT".to_owned()]);
    let ask = |target| {
        simulation.rules().power_target_refusal(
            simulation.state(),
            PLAYER,
            crate::rules::PowerButton::Selection(0),
            &enemies,
            target,
            None,
        )
    };
    assert_eq!(ask(Some(target)), None, "an enemy is a target");
    assert!(ask(Some(approacher)).is_some(), "its own unit is no enemy");
    assert!(ask(None).is_some(), "an object button needs an object");
}
