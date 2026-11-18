use super::*;

const DATA: &str = "\
SpecialPower Strike\n  ReloadTime = 0\nEnd\n\
Weapon Dagger\n  AttackRange = 999\n  DelayBetweenShots = 1\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object CahAssassin\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Camo\n    \
    InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      \
      DetectionRange = 50\n      ForbiddenConditions = MOVING\n    End\n    \
    UpdatePeriod = 1000\n    StartsActive = Yes\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Strike\n    SpecialPowerTemplate = Strike\n    \
    SpecialWeapon = Dagger\n    StartAbilityRange = 999\n    UnpackTime = 1000\n    \
    PreparationTime = 1\n    PackTime = 1000\n    LoseStealthOnTrigger = Yes\n    \
    PreTriggerUnstealthTime = 2000\n    IgnoreFacingCheck = Yes\n  End\n  \
  CommandSet = AssassinSet\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet AssassinSet\n  1 = Fire_Strike\nEnd\n\
CommandButton Fire_Strike\n  Command = SPECIAL_POWER\n  SpecialPower = Strike\n  \
  Options = NEED_TARGET_OBJECT\nEnd\n\
Object CahTarget\n  Side = Other\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["CahAssassin".to_owned(), "CahTarget".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let assassin = kinds
        .spawn(
            &data.combat,
            kinds.kind("CahAssassin").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let target = kinds
        .spawn(
            &data.combat,
            kinds.kind("CahTarget").unwrap(),
            Vec2::new(500.0, 300.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![assassin, target])
        .with_players(2)
        .with_teams(vec![Some(0), Some(1)])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let assassin = entities
        .iter()
        .copied()
        .find(|e| simulation.state().get(ids.kind, *e) == Some(&kinds.kind("CahAssassin").unwrap()))
        .unwrap();
    let target = entities
        .iter()
        .copied()
        .find(|e| simulation.state().get(ids.kind, *e) == Some(&kinds.kind("CahTarget").unwrap()))
        .unwrap();
    (simulation, ids, assassin, target)
}

#[test]
fn an_invisibility_hidden_caster_is_revealed_through_unpack_and_pack_only() {
    let (mut simulation, ids, assassin, target) = scene();
    let mut hidden_at_rest = false;
    for _ in 0..60 {
        simulation.tick().unwrap();
        if simulation
            .rules()
            .stealthed_from(simulation.state(), ids, OTHER, assassin)
        {
            hidden_at_rest = true;
            break;
        }
    }
    assert!(hidden_at_rest, "the fixture's premise: hidden at rest");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![assassin],
            },
        )
        .unwrap();
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

    for _ in 0..16 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation
            .rules()
            .stealthed_from(simulation.state(), ids, OTHER, assassin),
        "revealed mid-unpack"
    );

    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation
            .rules()
            .stealthed_from(simulation.state(), ids, OTHER, assassin),
        "still revealed mid-pack"
    );

    for _ in 0..20 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .rules()
            .stealthed_from(simulation.state(), ids, OTHER, assassin),
        "hidden again once the cast and its reveal end"
    );
}
