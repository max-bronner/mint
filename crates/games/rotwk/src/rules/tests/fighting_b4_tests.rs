use super::*;

pub(super) fn scene(
    data: &str,
    spawns: &[(&str, Vec2, PlayerId)],
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names: Vec<String> = spawns.iter().map(|(name, ..)| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let power_rules = power_names.cast_rules(&data.special_powers);
    let spawned = spawns
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawned)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(power_rules)
        .with_players(2);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn press(simulation: &mut Simulation<RotwkRules>, unit: Entity, button: &str) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::ToggleWeaponSet {
                button: button.to_owned(),
                lead: None,
            },
        )
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
}

pub(super) const TOGGLES: &str = "\
CommandSet TogglerSet\n  1 = Command_Toggle1\n  2 = Command_Toggle2\nEnd\n\
CommandButton Command_Toggle1\n  Command = TOGGLE_WEAPONSET\n  \
  FlagsUsedForToggle = WEAPONSET_TOGGLE_1\nEnd\n\
CommandButton Command_Toggle2\n  Command = TOGGLE_WEAPONSET\n  \
  FlagsUsedForToggle = WEAPONSET_TOGGLE_2\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n";

pub(super) const SHELL: &str = "\
  KindOf = MONSTER SELECTABLE\n  CommandSet = TogglerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\n";

fn bow(name: &str, extra: &str) -> String {
    format!(
        "Weapon {name}\n  AttackRange = 300\n  DelayBetweenShots = 500\n  ClipSize = 3\n  \
         ClipReloadTime = 2000\n  {extra}\n  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n"
    )
}

fn when_ready(simulation: &Simulation<RotwkRules>, ids: Ids, unit: Entity) -> u64 {
    simulation
        .state()
        .get(ids.armed, unit)
        .unwrap()
        .state
        .when_we_can_fire_again()
        .get()
}

fn f124_data() -> String {
    format!(
        "{TOGGLES}{}{}Weapon Fist\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
         DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
         Object Slow\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  \
         WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Bow\n  End\nEnd\n\
         Object Quick\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  \
         WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY BowQuick\n  End\nEnd\n\
         Object Sharer\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    ShareWeaponReloadTime = Yes\n    \
           Weapon = PRIMARY Fist\n    Weapon = SECONDARY Bow\n  End\n  \
         WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    ShareWeaponReloadTime = Yes\n    \
           Weapon = PRIMARY Bow\n  End\nEnd\n",
        bow("Bow", ""),
        bow("BowQuick", "InstantLoadClipOnActivate = Yes"),
    )
}

#[test]
fn a_new_weapon_after_a_set_change_reloads_unless_it_loads_instantly() {
    let (mut simulation, ids, units) = scene(
        &f124_data(),
        &[
            ("Slow", Vec2::new(300.0, 300.0), PLAYER),
            ("Quick", Vec2::new(300.0, 360.0), PLAYER),
        ],
    );
    simulation.tick().unwrap();
    press(&mut simulation, units[0], "Command_Toggle1");
    press(&mut simulation, units[1], "Command_Toggle1");
    let now = simulation.frame().get();
    let slow = when_ready(&simulation, ids, units[0]);
    assert!(
        slow > now + 40,
        "a 2 s reload left the bow ready at {slow}, now {now}"
    );
    assert!(
        when_ready(&simulation, ids, units[1]) <= now,
        "an instant-load bow was made to reload"
    );
}

#[test]
fn sets_sharing_reload_time_keep_a_carried_weapons_clock() {
    let (mut simulation, ids, units) =
        scene(&f124_data(), &[("Sharer", Vec2::new(300.0, 300.0), PLAYER)]);
    simulation.tick().unwrap();
    press(&mut simulation, units[0], "Command_Toggle1");
    let now = simulation.frame().get();
    assert!(
        when_ready(&simulation, ids, units[0]) <= now,
        "a bow carried between sets that share reload time was reloaded"
    );
}

pub(super) fn attack(simulation: &mut Simulation<RotwkRules>, unit: Entity, target: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
}

fn f121_data() -> String {
    let lob = |name: &str, flag: &str| {
        format!(
            "Weapon {name}\n  AttackRange = 300\n  PreAttackDelay = 1000\n  DelayBetweenShots = 3000\n  \
             {flag}\n  DamageNugget\n    Damage = 50\n    Radius = 20\n    DamageType = CRUSH\n  End\nEnd\n"
        )
    };
    format!(
        "{TOGGLES}{}{}\
         Weapon Snipe\n  AttackRange = 300\n  PreAttackDelay = 300\n  DelayBetweenShots = 3000\n  \
         DamageNugget\n    Damage = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
         Object Finisher\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Lob\n  End\nEnd\n\
         Object Giver\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY LobPlain\n  End\nEnd\n\
         Object Sniper\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Snipe\n  End\nEnd\n\
         Object Victim\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object Bystander\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
        lob("Lob", "FinishAttackOnceStarted = Yes"),
        lob("LobPlain", ""),
    )
}

fn bystander_after_interrupted_wind_up(shooter: &str) -> f32 {
    let (mut simulation, ids, units) = scene(
        &f121_data(),
        &[
            (shooter, Vec2::new(300.0, 300.0), PLAYER),
            ("Sniper", Vec2::new(300.0, 340.0), PLAYER),
            ("Victim", Vec2::new(450.0, 300.0), OTHER),
            ("Bystander", Vec2::new(450.0, 310.0), OTHER),
        ],
    );
    simulation.tick().unwrap();
    attack(&mut simulation, units[0], units[2]);
    attack(&mut simulation, units[1], units[2]);
    for _ in 0..(3 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert!(
        health_of(&simulation, ids, units[2]).is_none_or(|health| health <= 0.0),
        "the sniper never killed the victim"
    );
    health_of(&simulation, ids, units[3]).unwrap()
}

fn f122_data() -> String {
    let arrow = |name: &str, flag: &str| {
        format!(
            "Weapon {name}\n  AttackRange = 150\n  DelayBetweenShots = 1000\n  {flag}\n  \
             DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n"
        )
    };
    let walker = |name: &str, weapon: &str| {
        format!(
            "Object {name}\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
             Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
             WeaponSet\n    Conditions = None\n    Weapon = PRIMARY {weapon}\n  End\n  \
             Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
               MoodAttackCheckRate = 500\n  End\n  \
             LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
             Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
        )
    };
    format!(
        "{TOGGLES}{}{}{}{}\
         Object Runner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
        arrow("ArrowShort", ""),
        arrow("ArrowChase", "ChaseWeapon = Yes"),
        walker("Archer", "ArrowShort"),
        walker("Chaser", "ArrowChase"),
    )
}

fn distance_walked_after_a_runner(shooter: &str) -> f32 {
    let (mut simulation, ids, units) = scene(
        &f122_data(),
        &[
            (shooter, Vec2::new(300.0, 300.0), PLAYER),
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
fn a_human_players_ranged_unit_chases_an_acquired_target_only_with_a_chase_weapon() {
    assert!(distance_walked_after_a_runner("Archer") < 5.0);
    let walked = distance_walked_after_a_runner("Chaser");
    assert!(walked > 50.0, "walked {walked}");
}

fn f123_data() -> String {
    let slow = |name: &str, extra: &str| {
        format!(
            "Weapon {name}\n  AttackRange = 300\n  MinimumAttackRange = 100\n  PreAttackDelay = 2000\n  \
             DelayBetweenShots = 3000\n  {extra}\n  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n"
        )
    };
    let holder = |name: &str, weapon: &str| {
        format!(
            "Object {name}\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
             Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
             WeaponSet\n    Conditions = None\n    Weapon = PRIMARY {weapon}\n    Weapon = SECONDARY Fist\n  End\n  \
             LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
             Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
        )
    };
    format!(
        "{TOGGLES}{}{}{}{}{}{}\
         Weapon Fist\n  AttackRange = 300\n  DelayBetweenShots = 1000\n  \
         DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
         Object Runner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
        slow("SlowLocking", "LockWhenUsing = Yes"),
        slow("SlowPlain", ""),
        slow("SlowIdling", "LockWhenUsing = Yes\n  IdleAfterFiringDelay = 5000"),
        holder("Locker", "SlowLocking"),
        holder("Plain", "SlowPlain"),
        holder("Idler", "SlowIdling"),
    )
}

fn walk_in(holder: &str, ticks: u32, dest_x: f32) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let (mut simulation, ids, units) = scene(
        &f123_data(),
        &[
            (holder, Vec2::new(300.0, 300.0), PLAYER),
            ("Runner", Vec2::new(480.0, 300.0), OTHER),
        ],
    );
    simulation.tick().unwrap();
    attack(&mut simulation, units[0], units[1]);
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
                x: dest_x,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    (simulation, ids, units)
}

fn lock_of(simulation: &Simulation<RotwkRules>, unit: Entity) -> Option<u8> {
    simulation
        .rules()
        .upgrade_state_id()
        .and_then(|id| simulation.state().get(id, unit))
        .and_then(|state| state.weapon_lock)
}

#[test]
fn a_lock_when_using_weapon_locks_its_slot_for_the_wind_up() {
    let (simulation, _, units) = walk_in("Locker", 20, 480.0);
    assert_eq!(lock_of(&simulation, units[0]), Some(0));
    let (simulation, _, units) = walk_in("Plain", 20, 480.0);
    assert_eq!(lock_of(&simulation, units[0]), None);
}

#[test]
fn a_temporary_lock_ends_with_the_shot_or_a_new_order() {
    let (mut simulation, ids, units) = walk_in("Locker", 50, 480.0);
    let mut released = None;
    for tick in 0..120 {
        simulation.tick().unwrap();
        if lock_of(&simulation, units[0]).is_none() {
            released = Some(tick);
            break;
        }
    }
    let state = simulation.state().get(ids.armed, units[0]).unwrap().state;
    assert!(
        released.is_some(),
        "kept past its shot: shots {:?} ready {:?}",
        state.last_shot(),
        state.when_we_can_fire_again()
    );
    let (mut simulation, _, units) = walk_in("Idler", 170, 480.0);
    assert!(
        lock_of(&simulation, units[0]).is_some(),
        "released despite IdleAfterFiringDelay"
    );
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
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 100.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(lock_of(&simulation, units[0]), None, "kept past an order");
}

fn f126_data() -> String {
    let weapon = |name: &str, range: u32, extra: &str| {
        format!(
            "Weapon {name}\n  AttackRange = {range}\n  DelayBetweenShots = 1000\n  {extra}\n  \
             DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n"
        )
    };
    format!(
        "{TOGGLES}{}{}\
         Object Mortarman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Lobber\n    Weapon = SECONDARY Bow\n  End\n  \
           LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object Runner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
        weapon("Lobber", 300, "BombardType = Yes"),
        weapon("Bow", 200, ""),
    )
}

#[test]
fn a_bombard_weapon_is_not_chosen_against_an_object() {
    let (mut simulation, ids, units) = scene(
        &f126_data(),
        &[
            ("Mortarman", Vec2::new(300.0, 300.0), PLAYER),
            ("Runner", Vec2::new(450.0, 300.0), OTHER),
        ],
    );
    simulation.tick().unwrap();
    attack(&mut simulation, units[0], units[1]);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let held = simulation.state().get(ids.armed, units[0]).unwrap().weapon;
    assert_eq!(simulation.rules().combat.weapons.id("Bow"), Some(held));
}

#[test]
fn a_flagged_wind_up_survives_its_victims_death() {
    assert!(bystander_after_interrupted_wind_up("Finisher") < 1000.0);
    assert!((bystander_after_interrupted_wind_up("Giver") - 1000.0).abs() < 0.001);
}
