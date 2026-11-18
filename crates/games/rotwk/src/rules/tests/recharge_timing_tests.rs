use super::*;

fn data(starts_attack: &str, ability_fields: &str) -> String {
    format!(
        "SpecialPower SpecialAbilityTestPower\n  ReloadTime = 30000\nEnd\n\
         CommandSet CasterSet\n  1 = Fire_TestPower\nEnd\n\
         CommandButton Fire_TestPower\n  Command = SPECIAL_POWER\n  \
           SpecialPower = SpecialAbilityTestPower\nEnd\n\
         Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
           Behavior = SpecialPowerModule Tag_Power\n    \
             SpecialPowerTemplate = SpecialAbilityTestPower\n    \
             UpdateModuleStartsAttack = {starts_attack}\n  End\n  \
           Behavior = SpecialAbilityUpdate Tag_Ability\n    \
             SpecialPowerTemplate = SpecialAbilityTestPower\n    {ability_fields}\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load(ini: &str) -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", ini, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let kind = m.kinds.kind("Caster").unwrap();
    let spawn = m
        .kinds
        .spawn(
            &m.data.combat,
            kind,
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn only_caster(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap()
}

fn recharging(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.power_book, entity)
        .is_some_and(|book| book.0.iter().any(|p| p.available_on > Frame::ZERO))
}

fn available_on(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Frame {
    simulation
        .state()
        .get(ids.power_book, entity)
        .and_then(|book| book.0.first())
        .map_or(Frame::ZERO, |p| p.available_on)
}

fn press_fire(simulation: &mut Simulation<RotwkRules>, caster: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
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
}

#[test]
fn instant_no_recharges_at_the_press() {
    let m = load(&data("No", "Instant = Yes"));
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_caster(&simulation, ids);
    press_fire(&mut simulation, caster);
    simulation.tick().unwrap();
    assert!(
        recharging(&simulation, ids, caster),
        "UpdateModuleStartsAttack = No must recharge on the press's own tick"
    );
}

#[test]
fn instant_yes_recharges_from_the_trigger_despite_the_press_deferring_it() {
    let m = load(&data("Yes", "Instant = Yes"));
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_caster(&simulation, ids);
    press_fire(&mut simulation, caster);
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert!(
        recharging(&simulation, ids, caster),
        "the trigger must start the recharge even though the press deferred it"
    );
}

#[test]
fn chained_button_suppresses_the_trigger_recharge() {
    let m = load(&data(
        "Yes",
        "Instant = Yes\n    ChainedButton = Nonexistent_Button",
    ));
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_caster(&simulation, ids);
    press_fire(&mut simulation, caster);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        !recharging(&simulation, ids, caster),
        "ChainedButton must suppress this module's own recharge entirely"
    );
}

#[test]
fn a_normal_cast_aborted_before_its_trigger_leaves_the_power_ready() {
    let m = load(&data(
        "Yes",
        "StartAbilityRange = 5\n    PreparationTime = 2000",
    ));
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_caster(&simulation, ids);
    press_fire(&mut simulation, caster);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        !recharging(&simulation, ids, caster),
        "fixture premise: preparation (2000 ms) must still be running at tick 10"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(
        !recharging(&simulation, ids, caster),
        "an abort before the trigger must leave the power ready"
    );
}

#[test]
fn a_no_power_with_a_wind_up_is_ready_after_the_trigger_not_the_press() {
    const RELOAD: u32 = 900;
    let m = load(&data(
        "No",
        "StartAbilityRange = 5\n    UnpackTime = 2000\n    PreparationTime = 1000",
    ));
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_caster(&simulation, ids);
    let press_processed = simulation.frame();
    press_fire(&mut simulation, caster);
    simulation.tick().unwrap();
    let after_press = available_on(&simulation, ids, caster);
    assert!(
        after_press > Frame::ZERO,
        "fixture premise: the press (No) must start a recharge immediately"
    );

    let mut last_change = (Frame::ZERO, after_press);
    for _ in 0..150 {
        let processed = simulation.frame();
        simulation.tick().unwrap();
        let now = available_on(&simulation, ids, caster);
        if now != last_change.1 {
            last_change = (processed, now);
        }
    }
    let (last_restart, final_available) = last_change;
    assert!(
        last_restart > Frame::ZERO,
        "the trigger must restart the recharge at all"
    );
    assert_eq!(
        final_available,
        last_restart
            .checked_add(LogicFrames::from_frames(RELOAD))
            .unwrap(),
        "begin_recharge must restart the countdown from the frame it ran on"
    );
    assert!(
        last_restart.get() >= press_processed.get() + 70,
        "ready after the trigger (~90 ticks past the press), not after \
         startPreparation's own earlier restart (~60 ticks past it): last \
         restart was {} ticks past the press",
        last_restart.get() - press_processed.get()
    );
}
