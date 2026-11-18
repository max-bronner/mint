use super::*;
use crate::unit::Kind;

const DATA: &str = "\
SpecialPower SpecialAbilityCall\n  ReloadTime = 1000\nEnd\n\
Upgrade Upgrade_Call\n  Type = OBJECT\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Caller\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CallerSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_Call\n  End\n  \
  Behavior = DoCommandUpgrade Tag_Press\n    TriggeredBy = Upgrade_Call\n    \
    GetUpgradeCommandButtonName = Press_Call\n  End\n  \
  Behavior = SpecialPowerModule Tag_CallStarter\n    \
    SpecialPowerTemplate = SpecialAbilityCall\n    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = SummonReplacementSpecialAbilityUpdate Tag_Call\n    \
    SpecialPowerTemplate = SpecialAbilityCall\n    MountedTemplate = Called\n    \
    UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\nEnd\n\
Object Called\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\nEnd\n\
CommandSet CallerSet\nEnd\n\
CommandButton Press_Call\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityCall\nEnd\n";

#[test]
fn a_pressed_power_button_outside_every_command_set_summons_the_replacement() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec!["Caller".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let caller = kinds.kind("Caller").unwrap();
    let called = kinds
        .kind("Called")
        .expect("MountedTemplate is in the closure");
    let spawn = kinds
        .spawn(
            &data.combat,
            caller,
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(7, rules).unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let state = simulation.state();
    let kinds_now: Vec<Kind> = state
        .store(ids.kind)
        .unwrap()
        .entities()
        .iter()
        .filter_map(|entity| state.get(ids.kind, *entity).copied())
        .collect();
    assert!(
        !kinds_now.contains(&caller),
        "the caller is replaced: {kinds_now:?}"
    );
    assert!(
        kinds_now.contains(&called),
        "the summoned template stands in its place: {kinds_now:?}"
    );
}
