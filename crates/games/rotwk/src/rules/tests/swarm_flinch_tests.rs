use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
ModifierList ZSwarmBonus\n  Category = BUFF\n  Modifier = ARMOR 50%\n  Duration = 0\nEnd\n\
Weapon ZBow\n  AttackRange = 400\n  DelayBetweenShots = 300\n  AcceptableAimDelta = 360\n  \
  DamageNugget\n    Damage = 30\n    DamageType = PIERCE\n  End\nEnd\n\
Object ZSwarmer\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZFadingSwarmer ZSwarmer\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 3000\n    MaxLifetime = 3000\n  End\nEnd\n\
Object ZSwarmLeader\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = LargeGroupBonusUpdate Tag_Swarm\n    UpdateRate = 500\n    \
    HordeMemberFilter = NONE +ZSwarmer +ZFadingSwarmer\n    Count = 2\n    Radius = 60\n    \
    RubOffRadius = 200\n    AlliesOnly = Yes\n    AttributeModifier = ZSwarmBonus\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFlincher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 20000\n  End\n  \
  Behavior = HitReactionBehavior Tag_Hit\n    HitReactionThreshold1 = 0.0\n    \
    HitReactionThreshold2 = 25.0\n    HitReactionLifeTimer1 = 2000\n    \
    HitReactionLifeTimer2 = 2000\n    FastHitsResetReaction = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZStoicFlincher ZFlincher\n  \
  Behavior = HitReactionBehavior Tag_Hit\n    HitReactionThreshold1 = 0.0\n    \
    HitReactionThreshold2 = 25.0\n    HitReactionLifeTimer1 = 2000\n    \
    HitReactionLifeTimer2 = 2000\n    FastHitsResetReaction = No\n  End\nEnd\n\
Object ZMobileFlincher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 20000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = HitReactionBehavior Tag_Hit\n    HitReactionThreshold1 = 0.0\n    \
    HitReactionThreshold2 = 25.0\n    HitReactionLifeTimer1 = 2000\n    \
    HitReactionLifeTimer2 = 2000\n    FastHitsResetReaction = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZArcher\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 700\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const LEADER_AT: Vec2 = Vec2::new(300.0, 300.0);

fn swarm(members: &[(f32, &str)]) -> (Simulation<RotwkRules>, Ids, Entity) {
    let mut names: Vec<String> = vec![String::from("ZSwarmLeader")];
    let mut places: Vec<Vec2> = vec![LEADER_AT];
    for (offset, template) in members {
        names.push((*template).to_owned());
        places.push(Vec2::new(LEADER_AT.x + offset, LEADER_AT.y));
    }
    let (simulation, ids) = scene(&names, &places, &[]);
    let leader = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    (simulation, ids, leader)
}

fn scene(names: &[String], places: &[Vec2], enemy: &[usize]) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, names);
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
                    *place,
                    Facing::EAST,
                    if enemy.contains(&at) { OTHER } else { PLAYER },
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
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn buffed(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .is_some_and(|bonuses| bonuses.0.armor != 0.0)
}

#[test]
fn a_swarm_holds_its_list_only_while_enough_members_are_in_reach() {
    let (mut simulation, ids, leader) = swarm(&[(40.0, "ZSwarmer"), (50.0, "ZFadingSwarmer")]);
    run(&mut simulation, 45);
    assert!(buffed(&simulation, ids, leader), "two within 60");
    run(&mut simulation, 90);
    assert!(
        !buffed(&simulation, ids, leader),
        "one left once the fading swarmer's lifetime ran out"
    );
}

#[test]
fn a_member_between_the_two_radii_keeps_a_bonus_it_could_not_have_started() {
    let (mut simulation, ids, leader) = swarm(&[
        (40.0, "ZSwarmer"),
        (50.0, "ZFadingSwarmer"),
        (150.0, "ZSwarmer"),
    ]);
    run(&mut simulation, 45);
    assert!(buffed(&simulation, ids, leader), "two within 60");
    run(&mut simulation, 90);
    assert!(
        buffed(&simulation, ids, leader),
        "the distant member is inside RubOffRadius, so the bonus is kept"
    );
}

#[test]
fn only_fast_hits_reset_reaction_carries_a_flinch_past_its_first_hit() {
    let ends_of = |template: &str| -> Vec<u64> {
        let (mut simulation, ids) = scene(
            &[template.to_owned(), String::from("ZArcher")],
            &[LEADER_AT, Vec2::new(LEADER_AT.x + 200.0, LEADER_AT.y)],
            &[1],
        );
        let victim = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .first()
            .copied()
            .unwrap();
        let mut seen: Vec<u64> = Vec::new();
        for _ in 0..120 {
            simulation.tick().unwrap();
            if let Some(running) = simulation
                .state()
                .get(ids.hit_reaction, victim)
                .filter(|running| running.level > 0)
            {
                if seen.last() != Some(&running.ends.get()) {
                    seen.push(running.ends.get());
                }
            }
        }
        seen
    };
    let resetting = ends_of("ZFlincher");
    let stoic = ends_of("ZStoicFlincher");
    assert!(
        !resetting.is_empty() && !stoic.is_empty(),
        "both were hit: {resetting:?} {stoic:?}"
    );
    let gaps = |ends: &[u64]| -> Vec<u64> {
        ends.windows(2)
            .map(|pair| pair[1].saturating_sub(pair[0]))
            .collect()
    };
    assert!(
        gaps(&resetting).iter().all(|gap| *gap < 60),
        "every shot pushes the end out by its own arrival, not by a whole \
         expiry-and-restart: {resetting:?}"
    );
    assert!(
        gaps(&stoic).iter().all(|gap| *gap >= 60),
        "a stoic flincher only takes a new reaction once the last one ended: {stoic:?}"
    );
}

#[test]
fn a_move_order_clears_a_running_hit_reaction_early() {
    let (mut simulation, ids) = scene(
        &[String::from("ZMobileFlincher"), String::from("ZArcher")],
        &[LEADER_AT, Vec2::new(LEADER_AT.x + 200.0, LEADER_AT.y)],
        &[1],
    );
    let victim = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    let mut running_since = None;
    for _ in 0..120 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.hit_reaction, victim)
            .is_some_and(|running| running.level > 0)
        {
            running_since = Some(simulation.frame().get());
            break;
        }
    }
    running_since.expect("the flincher never took a hit reaction");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![victim],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: LEADER_AT.x - 300.0,
                y: LEADER_AT.y,
                queue: false,
            },
        )
        .unwrap();
    let mut cleared = false;
    for _ in 0..12 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.hit_reaction, victim)
            .is_none_or(|running| running.level == 0)
        {
            cleared = true;
            break;
        }
    }
    assert!(
        cleared,
        "the reaction should clear within a retail frame or two of the order, \
         long before its 2000 ms timer"
    );
}
