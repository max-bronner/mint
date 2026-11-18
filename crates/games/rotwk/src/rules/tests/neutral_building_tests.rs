use super::*;

const DATA: &str = "Upgrade Upgrade_TestFaction
  Type = PLAYER
End
CommandSet RepairOnlySet
  1 = Command_TestRepair
End
CommandButton Command_TestRepair
  Command = START_SELF_REPAIR
End
CommandSet HostelSet
  1 = Build_Walker
End
CommandButton Build_Walker
  Command = UNIT_BUILD
  Object = Walker
End
Object Hostel
  Side = Neutral
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = RepairOnlySet
    Body = StructureBody Tag_Body
    MaxHealth = 1000
  End
    Behavior = ProductionUpdate Tag_Production
  End
    Behavior = QueueProductionExitUpdate Tag_Exit
    UnitCreatePoint = X:10 Y:0 Z:0
  End
    Behavior = CommandSetUpgrade Tag_Faction
    TriggeredBy = Upgrade_TestFaction
        CommandSet = HostelSet
  End
    Geometry = BOX
  GeometryMajorRadius = 20
  GeometryMinorRadius = 20
  GeometryHeight = 30
End
\
SpecialPower SpecialAbilityTestQuick\n  Flags = RESPECT_RECHARGE_TIME_DISCOUNT\n  ReloadTime = 30000\nEnd\n\
SpecialPower SpecialAbilityTestPlain\n  ReloadTime = 30000\nEnd\n\
CommandSet CasterSet\n  1 = Fire_TestQuick\n  2 = Fire_TestPlain\nEnd\n\
CommandButton Fire_TestQuick\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityTestQuick\nEnd\n\
CommandButton Fire_TestPlain\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityTestPlain\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpecialPowerModule Tag_Quick\n    SpecialPowerTemplate = SpecialAbilityTestQuick\n  End\n  \
  Behavior = SpecialPowerModule Tag_Plain\n    SpecialPowerTemplate = SpecialAbilityTestPlain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Beacon\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SpellRechargeModifierUpgrade Tag_Quicken\n    StartsActive = Yes\n    \
    Percentage = -20%\n    Percentage = -40%\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Chest\n  Side = Civilian\n  KindOf = SELECTABLE IMMOBILE NOT_AUTOACQUIRABLE UNATTACKABLE CRATE\n  \
  Body = HighlanderBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = SalvageCrateCollide Tag_Take\n    ForbiddenKindOf = PROJECTILE GHOST\n    \
    LevelUpChance = 0%\n    ResourceChance = 100%\n    MinResource = 70\n    MaxResource = 70\n    \
    AllowAIPickup = Yes\n  End\n  \
  Behavior = DeletionUpdate Tag_Keep\n    MinLifetime = -1\n    MaxLifetime = -1\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 6\n  GeometryMinorRadius = 6\n  GeometryHeight = 6\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 120\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Wraith\n  Side = Men\n  KindOf = INFANTRY SELECTABLE GHOST\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn simulation(placed: &[(&str, f32, f32, PlayerId)]) -> Simulation<RotwkRules> {
    simulation_and_kinds(placed).0
}

fn simulation_and_kinds(
    placed: &[(&str, f32, f32, PlayerId)],
) -> (Simulation<RotwkRules>, Arc<KindTable>) {
    simulation_from(DATA, placed)
}

fn simulation_from(
    text: &str,
    placed: &[(&str, f32, f32, PlayerId)],
) -> (Simulation<RotwkRules>, Arc<KindTable>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster", "Beacon", "Chest", "Walker", "Wraith", "Hostel"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let mut faction = mint_sim::UpgradeMask::EMPTY;
    faction.set(data.upgrade_bits.bit("Upgrade_TestFaction").unwrap());
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = kinds.kind(name).unwrap();
            kinds
                .spawn(&data.combat, kind, Vec2::new(*x, *y), Facing::EAST, *owner)
                .unwrap()
        })
        .collect();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_initial_upgrades(vec![faction.clone(), faction])
        .with_multiplay_multipliers(data.multiplay);
    (Simulation::new(5, rules).unwrap(), kinds)
}

fn first_of(simulation: &Simulation<RotwkRules>, name: &str) -> Option<Entity> {
    let ids = simulation.rules().ids().unwrap();
    let kind = simulation_and_kinds(&[]).1.kind(name)?;
    let world = simulation.state();
    world
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| **k == kind && world.is_live(*entity))
        .map(|(entity, _)| entity)
}

fn ready_frames_with_beacons(beacons: usize) -> (u64, u64) {
    let mut placed = vec![("Caster", 100.0, 100.0, PLAYER)];
    let spots = [(200.0, 200.0), (300.0, 200.0), (400.0, 200.0)];
    placed.extend(
        spots
            .iter()
            .take(beacons)
            .map(|&(x, y)| ("Beacon", x, y, PLAYER)),
    );
    let mut simulation = simulation(&placed);
    let ids = simulation.rules().ids().unwrap();
    let caster = first_of(&simulation, "Caster").unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
    for button in [0, 1] {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::FireSpecialPower {
                    button,
                    target: None,
                    at: None,
                },
            )
            .unwrap();
    }
    simulation.tick().unwrap();
    let book = simulation.state().get(ids.power_book, caster).unwrap();
    let on = |id: u16| {
        book.0
            .iter()
            .find(|instance| instance.power.get() == id)
            .map_or(0, |instance| instance.available_on.get())
    };
    (on(1), on(0))
}

#[test]
fn beacons_shorten_only_the_discounted_power_by_their_count() {
    let (quick, plain) = ready_frames_with_beacons(0);
    assert_eq!(quick, plain, "no beacon, no discount");
    let one = ready_frames_with_beacons(1);
    assert_eq!(
        one,
        (plain - 180, plain),
        "one beacon: -20% of the discounted power only"
    );
    assert_eq!(
        ready_frames_with_beacons(2).0,
        plain - 360,
        "two beacons: -40%"
    );
    assert_eq!(
        ready_frames_with_beacons(3).0,
        plain - 360,
        "the last entry holds"
    );
}

#[test]
fn the_modifier_is_the_last_one_chosen_even_at_zero() {
    let list = [-0.2f32, -0.4];
    let none = crate::spell_state::SpellRecharge::default();
    let one = none.moved(1, &list);
    let two = one.moved(1, &list);
    let back = two.moved(-1, &list).moved(-1, &list);
    assert_eq!((one.count, one.modifier), (1, -0.2));
    assert_eq!((two.count, two.modifier), (2, -0.4));
    assert_eq!((back.count, back.modifier), (0, -0.2));
}

#[test]
fn a_unit_on_a_chest_takes_its_money_and_a_forbidden_one_does_not() {
    let purse = |simulation: &Simulation<RotwkRules>| {
        let ids = simulation.rules().ids().unwrap();
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
    };
    let mut taken = simulation(&[
        ("Chest", 200.0, 200.0, OTHER),
        ("Walker", 203.0, 200.0, PLAYER),
    ]);
    let before = purse(&taken);
    for _ in 0..5 {
        taken.tick().unwrap();
    }
    assert!(first_of(&taken, "Chest").is_none(), "the chest is taken");
    assert_eq!(purse(&taken) - before, 70);

    let mut refused = simulation(&[
        ("Chest", 200.0, 200.0, OTHER),
        ("Wraith", 203.0, 200.0, PLAYER),
    ]);
    let before = purse(&refused);
    for _ in 0..60 {
        refused.tick().unwrap();
    }
    assert!(first_of(&refused, "Chest").is_some(), "the chest stays");
    assert_eq!(purse(&refused), before);
}

#[test]
fn a_chest_whose_money_scales_to_nothing_still_shows_its_text() {
    let text = format!(
        "{DATA}GameData
  MultiPlayMoneyMult = MP2:0.0
End
"
    );
    let (mut simulation, _) = simulation_from(
        &text,
        &[
            ("Chest", 200.0, 200.0, OTHER),
            ("Walker", 203.0, 200.0, PLAYER),
        ],
    );
    let mut shown = Vec::new();
    for _ in 0..5 {
        let frame = simulation.frame();
        simulation.tick().unwrap();
        shown.extend(
            simulation
                .rules()
                .crate_texts(simulation.state(), frame)
                .iter()
                .map(|text| (text.player, text.kind)),
        );
    }
    assert!(
        first_of(&simulation, "Chest").is_none(),
        "the chest is taken"
    );
    assert_eq!(shown, vec![(PLAYER, crate::rules::CrateTextKind::Cash(0))]);
}

#[test]
fn a_set_given_by_an_upgrade_makes_a_producer_of_a_plain_structure() {
    let mut simulation = simulation(&[("Hostel", 300.0, 300.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let hostel = first_of(&simulation, "Hostel").unwrap();
    simulation.tick().unwrap();
    let purse = |simulation: &Simulation<RotwkRules>| {
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
    };
    let before = purse(&simulation);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![hostel],
        },
        RotwkCommand::QueueUnit { button: 0 },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    let queued = simulation
        .state()
        .get(ids.producer, hostel)
        .map_or(0, |producer| producer.queue.entries().len());
    assert_eq!(queued, 1, "the walker is queued");
    assert_eq!(before - purse(&simulation), 120);
}
