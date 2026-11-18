use super::*;

const DEFERRED: &str = "\
SpecialPower SpecialAbilityGlow\n  ReloadTime = 30000\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Glow\nEnd\n\
CommandButton Fire_Glow\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityGlow\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialPowerModule Tag_Power\n    SpecialPowerTemplate = SpecialAbilityGlow\n    \
    UpdateModuleStartsAttack = Yes\n    SetModelCondition = ModelConditionState:USER_2\n    \
    SetModelConditionTime = 2.1\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_Ability\n    SpecialPowerTemplate = SpecialAbilityGlow\n    \
    UnpackTime = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const AT_PRESS: &str = "\
SpecialPower SpecialAbilityGlow\n  ReloadTime = 30000\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Glow\nEnd\n\
CommandButton Fire_Glow\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityGlow\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialPowerModule Tag_Power\n    SpecialPowerTemplate = SpecialAbilityGlow\n    \
    SetModelCondition = ModelConditionState:USER_2\n    SetModelConditionTime = 2.1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const INITIATE_FX: &str = "\
FXList FX_InitiateTest\n  Sound\n    Name = ZInitiateSound\n  End\nEnd\n\
SpecialPower SpecialAbilityGlow\n  ReloadTime = 30000\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Glow\nEnd\n\
CommandButton Fire_Glow\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityGlow\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialPowerModule Tag_Power\n    SpecialPowerTemplate = SpecialAbilityGlow\n    \
    InitiateFX = FX_InitiateTest\n    SetModelCondition = ModelConditionState:USER_2\n    \
    SetModelConditionTime = 2.1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const HERO_MODE: &str = "\
ModifierList HeroBoost\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 6000\nEnd\n\
SpecialPower SpecialAbilityHero\n  ReloadTime = 30000\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Hero\nEnd\n\
CommandButton Fire_Hero\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityHero\nEnd\n\
Object HeroCaster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Hero\n    SpecialPowerTemplate = SpecialAbilityHero\n    \
    UnpackTime = 100\n    HeroEffectDuration = 3000\n    HeroAttributeModifier = HeroBoost\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn simulation_from(data: &str) -> (Simulation<RotwkRules>, Entity) {
    simulation_with(data, "Caster")
}

fn simulation_with(data: &str, object: &str) -> (Simulation<RotwkRules>, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [object].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let caster = kinds
        .spawn(
            &data.combat,
            kinds.kind(object).unwrap(),
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![caster])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = simulation
        .state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .find(|(_, owner)| owner.0 == PLAYER)
        .map(|(entity, _)| entity)
        .unwrap();
    (simulation, caster)
}

fn has_user_2(simulation: &Simulation<RotwkRules>, entity: Entity) -> bool {
    let Some(id) = simulation.rules().upgrade_state_id() else {
        return false;
    };
    simulation
        .state()
        .get(id, entity)
        .is_some_and(|state| state.has_model_condition("USER_2"))
}

fn has_weapon_set_flag(simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str) -> bool {
    let Some(id) = simulation.rules().upgrade_state_id() else {
        return false;
    };
    simulation
        .state()
        .get(id, entity)
        .is_some_and(|state| state.has_weapon_set_flag(flag))
}

fn has_hero(simulation: &Simulation<RotwkRules>, entity: Entity) -> bool {
    let Some(id) = simulation.rules().upgrade_state_id() else {
        return false;
    };
    simulation
        .state()
        .get(id, entity)
        .is_some_and(|state| state.has_model_condition("HERO"))
}

fn fire(simulation: &mut Simulation<RotwkRules>, caster: Entity) {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![caster],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: None,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
}

#[test]
fn a_deferred_power_raises_its_condition_at_preparation_start_and_it_expires() {
    let (mut simulation, caster) = simulation_from(DEFERRED);
    fire(&mut simulation, caster);
    simulation.tick().unwrap();
    assert!(
        !has_user_2(&simulation, caster),
        "USER_2 must not rise at the press for a deferred power"
    );
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        has_user_2(&simulation, caster),
        "USER_2 must be held once preparation starts"
    );
    for _ in 0..80 {
        simulation.tick().unwrap();
    }
    assert!(
        !has_user_2(&simulation, caster),
        "USER_2 must clear once its own hold expires"
    );
}

#[test]
fn an_undeferred_power_with_no_ability_update_raises_its_condition_at_the_press() {
    let (mut simulation, caster) = simulation_from(AT_PRESS);
    fire(&mut simulation, caster);
    simulation.tick().unwrap();
    assert!(
        has_user_2(&simulation, caster),
        "USER_2 must rise on the press frame when nothing defers it"
    );
}

#[test]
fn initiate_fx_plays_as_an_object_play_on_the_caster() {
    let (mut simulation, caster) = simulation_from(INITIATE_FX);
    fire(&mut simulation, caster);
    let frame = simulation.tick().unwrap().frame;
    let events = simulation
        .rules()
        .fx_events(simulation.state(), frame)
        .to_vec();
    assert!(
        events.iter().any(|event| {
            simulation.rules().fx_name(event.list) == Some("FX_InitiateTest")
                && event.on_object
                && event.caller == caster
        }),
        "{events:?}"
    );
}

#[test]
fn hero_mode_outlives_the_cast_and_the_modifier_times_by_the_lists_own_duration() {
    let (mut simulation, caster) = simulation_with(HERO_MODE, "HeroCaster");
    let ids = simulation.rules().ids().unwrap();
    fire(&mut simulation, caster);
    for _ in 0..15 {
        simulation.tick().unwrap();
    }
    assert!(
        has_weapon_set_flag(&simulation, caster, "WEAPONSET_HERO_MODE"),
        "WEAPONSET_HERO_MODE must rise at unpack start"
    );
    let damage_mult = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.bonuses, caster)
            .map_or(1.0, |bonuses| bonuses.0.damage_mult)
    };
    assert!(
        (damage_mult(&simulation) - 1.5).abs() < 1e-6,
        "HeroBoost must have been given once the (near-instant) cast triggers"
    );
    assert!(
        has_hero(&simulation, caster),
        "HERO must rise with the trigger"
    );
    for _ in 0..70 {
        simulation.tick().unwrap();
    }
    assert!(
        has_weapon_set_flag(&simulation, caster, "WEAPONSET_HERO_MODE"),
        "WEAPONSET_HERO_MODE must outlive the (already finished) cast"
    );
    assert!(
        has_hero(&simulation, caster),
        "HERO must still be held before its own 3000 ms expiry"
    );
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !has_weapon_set_flag(&simulation, caster, "WEAPONSET_HERO_MODE"),
        "WEAPONSET_HERO_MODE must clear once HeroEffectDuration elapses"
    );
    assert!(
        !has_hero(&simulation, caster),
        "HERO must clear once HeroEffectDuration elapses"
    );
    assert!(
        (damage_mult(&simulation) - 1.5).abs() < 1e-6,
        "HeroBoost must still be held by its own, longer Duration"
    );
}
