use super::*;

const DATA: &str = "\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
SpecialPower SpecialAbilityBlast\n  ReloadTime = 0\nEnd\n\
Weapon BlastWeapon\n  FireFX = FX_Blast\n  \
  DamageNugget\n    Damage = 50\n    Radius = 50\n    DamageType = FLAME\n  End\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CasterSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = DevastateSpecialPower Tag_Dev\n    SpecialPowerTemplate = SpecialAbilityBlast\n    \
    Radius = 100\n    FireWeapon = BlastWeapon\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 30\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Blast\nEnd\n\
CommandButton Fire_Blast\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityBlast\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object Target\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster", "Target"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 0)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn placed(simulation: &Simulation<RotwkRules>, ids: Ids, m: &Match, name: &str) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

#[test]
fn a_ground_targeted_powers_weapon_fx_plays_at_the_shooter_with_the_place_as_its_target() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Caster", 200.0, 200.0, PLAYER),
            ("Target", 260.0, 260.0, OTHER),
        ],
    );
    let caster = placed(&simulation, ids, &m, "Caster")[0];
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
                at: Some((260.0, 260.0)),
            },
        )
        .unwrap();
    let report = simulation.tick().unwrap();
    let events = simulation
        .rules()
        .fx_events(simulation.state(), report.frame);
    let fired: Vec<_> = events
        .iter()
        .filter(|event| simulation.rules().fx_name(event.list) == Some("FX_Blast"))
        .collect();
    assert_eq!(fired.len(), 1, "the weapon's FireFX fired once");
    let event = fired[0];
    assert_eq!(
        (event.x, event.y),
        (200.0, 200.0),
        "at the shooter, not the place"
    );
    assert_eq!(
        event.target,
        Some((260.0, 260.0)),
        "the place is the secondary point"
    );
}
