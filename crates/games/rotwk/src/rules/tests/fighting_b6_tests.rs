use super::fighting_b4_tests::{attack, scene, TOGGLES};
use super::*;

fn weapon(name: &str, range: u32, extra: &str) -> String {
    format!(
        "Weapon {name}\n  AttackRange = {range}\n  DelayBetweenShots = 500\n  {extra}\n  \
         DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n"
    )
}

fn shooter(name: &str, set_extra: &str, slots: &str) -> String {
    format!(
        "Object {name}\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           WeaponSet\n    Conditions = None\n    {set_extra}\n    {slots}\n  End\n  \
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

const DUMMY: &str = "Object Dummy\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
    Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
    Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn current_slot(simulation: &Simulation<RotwkRules>, ids: Ids, unit: Entity) -> u8 {
    simulation
        .state()
        .get(ids.weapon_slots, unit)
        .map_or(0, |held| held.current)
}

fn run_attack(
    data: &str,
    shooters: &[&str],
    victim_at: f32,
    ticks: u32,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let mut spawns: Vec<(&str, Vec2, PlayerId)> = shooters
        .iter()
        .enumerate()
        .map(|(row, name)| (*name, Vec2::new(300.0, 300.0 + 60.0 * row as f32), PLAYER))
        .collect();
    spawns.push(("Dummy", Vec2::new(300.0 + victim_at, 300.0), OTHER));
    let (mut simulation, ids, units) = scene(data, &spawns);
    simulation.tick().unwrap();
    for shooter in 0..shooters.len() {
        let victim = units[shooters.len()];
        attack(&mut simulation, units[shooter], victim);
        simulation.tick().unwrap();
    }
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, units)
}

#[test]
fn the_sets_default_criteria_picks_the_slot() {
    let data = format!(
        "{TOGGLES}{}{}{DUMMY}{}{}",
        weapon("Short", 100, ""),
        weapon("Long", 300, ""),
        shooter(
            "Reach",
            "DefaultWeaponChoiceCritera = PREFER_LONGEST_RANGE",
            "Weapon = PRIMARY Short\n    Weapon = SECONDARY Long"
        ),
        shooter(
            "Plain",
            "",
            "Weapon = PRIMARY Short\n    Weapon = SECONDARY Long"
        ),
    );
    let (simulation, ids, units) = run_attack(&data, &["Reach", "Plain"], 900.0, 6);
    assert_eq!(current_slot(&simulation, ids, units[0]), 1);
    assert_eq!(current_slot(&simulation, ids, units[1]), 0);
}

#[test]
fn a_set_sharing_ready_status_holds_its_slot_while_a_weapon_reloads() {
    let data = format!(
        "{TOGGLES}{}{}{DUMMY}{}{}",
        weapon("Slow", 300, "ClipSize = 1\n  ClipReloadTime = 20000"),
        weapon("Hit", 300, ""),
        shooter(
            "Sharing",
            "ReadyStatusSharedWithinSet = Yes",
            "Weapon = PRIMARY Slow\n    Weapon = SECONDARY Hit\n    PreferredAgainst = SECONDARY MONSTER"
        ),
        shooter(
            "Free",
            "",
            "Weapon = PRIMARY Slow\n    Weapon = SECONDARY Hit\n    PreferredAgainst = SECONDARY MONSTER"
        ),
    );
    let (simulation, ids, units) = run_attack(&data, &["Sharing", "Free"], 900.0, 6);
    assert_eq!(current_slot(&simulation, ids, units[0]), 0);
    assert_eq!(current_slot(&simulation, ids, units[1]), 1);
}

#[test]
fn a_weapon_whose_nugget_accepts_the_victim_is_usable_without_damage() {
    let data = format!(
        "{TOGGLES}ModifierList Slow\n  Category = DEBUFF\n  Modifier = SPEED 50%\n  Duration = 2000\nEnd\n\
         Weapon Chill\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
           AttributeModifierNugget\n    AttributeModifier = Slow\n    Radius = 10\n  End\nEnd\n\
         {}{DUMMY}{}",
        weapon("Hit", 300, ""),
        shooter(
            "Caster",
            "",
            "Weapon = PRIMARY Chill\n    Weapon = SECONDARY Hit"
        ),
    );
    let (simulation, ids, units) = run_attack(&data, &["Caster"], 900.0, 6);
    assert_eq!(current_slot(&simulation, ids, units[0]), 0);
}

#[test]
fn a_created_weapon_starts_reloading_unless_it_loads_instantly() {
    let data = format!(
        "{TOGGLES}{}{}{DUMMY}{}{}",
        weapon("Bow", 300, "ClipSize = 3\n  ClipReloadTime = 4000"),
        weapon(
            "BowQuick",
            300,
            "ClipSize = 3\n  ClipReloadTime = 4000\n  InstantLoadClipOnActivate = Yes"
        ),
        shooter("Slow", "", "Weapon = PRIMARY Bow"),
        shooter("Quick", "", "Weapon = PRIMARY BowQuick"),
    );
    let (mut simulation, ids, units) = scene(
        &data,
        &[
            ("Slow", Vec2::new(300.0, 300.0), PLAYER),
            ("Quick", Vec2::new(300.0, 360.0), PLAYER),
        ],
    );
    simulation.tick().unwrap();
    let now = simulation.frame().get();
    let ready = |unit| {
        simulation
            .state()
            .get(ids.armed, unit)
            .unwrap()
            .state
            .when_we_can_fire_again()
            .get()
    };
    assert!(ready(units[0]) > now, "{} vs {now}", ready(units[0]));
    assert!(ready(units[1]) <= now, "{} vs {now}", ready(units[1]));
}

#[test]
fn an_attack_on_the_attackers_own_unit_ends_without_a_shot() {
    let data = format!(
        "{TOGGLES}{}{DUMMY}{}",
        weapon("Hit", 300, ""),
        shooter("Gunner", "", "Weapon = PRIMARY Hit"),
    );
    let (mut simulation, ids, units) = scene(
        &data,
        &[
            ("Gunner", Vec2::new(300.0, 300.0), PLAYER),
            ("Dummy", Vec2::new(500.0, 300.0), PLAYER),
        ],
    );
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ForceAttackObject { target: units[1] })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let health = simulation.state().get(ids.health, units[1]).unwrap().0;
    assert_eq!(health.current(), health.max());
    assert!(!simulation.state().contains(ids.attack_order, units[0]));
}

fn horde_keeps_its_order(victim_armour: Option<&str>) -> bool {
    let (mut simulation, ids, hordes) = super::group_tests::archers_and_victims_armoured(
        &[Vec2::new(420.0, 300.0)],
        1,
        true,
        false,
        victim_armour,
    );
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    simulation.state().contains(ids.attack_order, hordes[0])
}

#[test]
fn a_horde_whose_member_weapon_is_refused_drops_the_attack() {
    assert!(horde_keeps_its_order(None));
    assert!(!horde_keeps_its_order(Some("TestImmune")));
}

fn walked_after_a_runner(extra: &str) -> f32 {
    let data = format!(
        "{TOGGLES}{}\
         SpecialPower ChargePower\n  Enum = SPECIAL_CHARGE_ATTACK\n  ReloadTime = 5000\nEnd\n\
         Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Arrow\n  End\n  \
           Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
             MoodAttackCheckRate = 500\n  End\n  {extra}\
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object Runner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
        weapon("Arrow", 150, ""),
    );
    let (mut simulation, ids, units) = scene(
        &data,
        &[
            ("Archer", Vec2::new(300.0, 300.0), PLAYER),
            ("Runner", Vec2::new(400.0, 300.0), OTHER),
        ],
    );
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let start = position(&simulation, ids, units[0]);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[1]],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..(5 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    position(&simulation, ids, units[0]).minus(start).length()
}

#[test]
fn a_unit_owning_a_charge_ability_chases_with_a_ranged_weapon() {
    assert!(walked_after_a_runner("") < 5.0);
    let walked = walked_after_a_runner(
        "Behavior = SpecialPowerModule Tag_Charge\n    SpecialPowerTemplate = ChargePower\n  End\n  ",
    );
    assert!(walked > 50.0, "walked {walked}");
}
