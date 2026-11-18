use super::*;

const DATA: &str = "\
Weapon ZThiefBlade\n  AttackRange = 400\n  DelayBetweenShots = 3000\n  AcceptableAimDelta = 360\n  \
  StealMoneyNugget\n    AmountStolenPerAttack = 40\n  End\nEnd\n\
Weapon ZStunClub\n  AttackRange = 400\n  DelayBetweenShots = 3000\n  AcceptableAimDelta = 360\n  \
  SpecialModelConditionNugget\n    ModelConditionNames = USER_2\n    \
    ModelConditionDuration = 1000\n  End\nEnd\n\
Object ZVictim\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 20000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZThief\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 700\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZThiefBlade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZClubber ZThief\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZStunClub\n  End\nEnd\n";

const VICTIM_AT: Vec2 = Vec2::new(300.0, 300.0);

fn scene(template: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let names = [String::from("ZVictim"), template.to_owned()];
    let places = [VICTIM_AT, Vec2::new(VICTIM_AT.x + 200.0, VICTIM_AT.y)];
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = names
        .iter()
        .zip(places)
        .enumerate()
        .map(|(at, (name, place))| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    place,
                    Facing::EAST,
                    if at == 0 { OTHER } else { PLAYER },
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let victim = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    (simulation, ids, victim)
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

#[test]
fn a_steal_money_hit_moves_the_amount_between_the_two_purses() {
    let (mut simulation, ids, _) = scene("ZThief");
    let before = (
        purse(&simulation, ids, PLAYER),
        purse(&simulation, ids, OTHER),
    );
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    let after = (
        purse(&simulation, ids, PLAYER),
        purse(&simulation, ids, OTHER),
    );
    assert!(
        after.0 > before.0,
        "the thief's player gained: {before:?} {after:?}"
    );
    assert_eq!(
        after.0 - before.0,
        before.1 - after.1,
        "what one lost is what the other gained: {before:?} {after:?}"
    );
}

#[test]
fn a_special_model_condition_comes_off_when_its_duration_runs_out() {
    let (mut simulation, ids, victim) = scene("ZClubber");
    let mut worn_at = None;
    for frame in 0..120u32 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.special_conditions, victim) {
            worn_at = Some(frame);
            break;
        }
    }
    let worn_at = worn_at.expect("the club's nugget put its condition on");
    for _ in 0..31 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.special_conditions, victim),
        "1,000 ms is 30 ticks, so it is off by now (worn on tick {worn_at})"
    );
}
