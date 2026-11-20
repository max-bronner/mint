use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::{Dictionary, HeightMapData, ScriptNode, ScriptTree, TeamDef, Value};
use mint_sage_passability::Passability;
use mint_sim::{PlayerId, Simulation};

use mint_sage_ini::fire_logic_system::{FireLogicSystem, FireMaterial};

use super::team::TeamProtos;
use super::w3a_tests::{act, counting, number, once, run, script, state_of, text};
use super::MapScriptEngine;
use crate::rules::terrain_fire::FireGrid;
use crate::rules::RotwkRules;
use crate::templates::KindTable;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Armor ZPlain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon ZClaw\n  AttackRange = 60\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
SpecialPower ZPower\n  ReloadTime = 4000\nEnd\n\
Object ZKiller\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  ShroudClearingRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZClaw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZVictim\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 25\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZPlain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZCaster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SpecialPowerModule Tag_Power\n    SpecialPowerTemplate = ZPower\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor ZWalk\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\n  \
  WanderAboutPointRadius = 24\nEnd\n\
Object ZWalker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_WANDER\n    Speed = 30\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_PANIC\n    Speed = 120\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTownie\n  Side = Men\n  KindOf = INFANTRY SELECTABLE CAN_BE_REPULSED\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_WANDER\n    Speed = 30\n  End\n  \
  LocomotorSet\n    Locomotor = ZWalk\n    Condition = SET_PANIC\n    Speed = 120\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTower\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ROOTS: &[&str] = &[
    "ZKiller", "ZVictim", "ZCaster", "ZSoldier", "ZTower", "ZWalker", "ZTownie",
];

pub(super) const P0: PlayerId = PlayerId::new(0);
pub(super) const P1: PlayerId = PlayerId::new(1);
pub(super) const SIDE: PlayerId = PlayerId::new(2);

pub(super) struct Rig {
    pub(super) placed: Vec<(&'static str, f32, f32, PlayerId)>,
    pub(super) teams: Vec<TeamDef>,
    pub(super) scripts: Vec<mint_sage_map::Script>,
    pub(super) waypoints: Vec<(&'static str, f32, f32)>,
    pub(super) computer: bool,
    pub(super) fire: bool,
    pub(super) paths: Vec<super::PathPoint>,
    pub(super) fudge: f32,
    pub(super) ai_seat: bool,
}

pub(super) fn rig(scripts: Vec<mint_sage_map::Script>) -> Rig {
    Rig {
        placed: Vec::new(),
        teams: Vec::new(),
        scripts,
        waypoints: vec![("P1", 10.0, 10.0), ("P2", 60.0, 60.0)],
        computer: false,
        fire: false,
        paths: Vec::new(),
        fudge: 0.0,
        ai_seat: false,
    }
}

impl Rig {
    pub(super) fn start(self) -> Simulation<RotwkRules> {
        let data = crate::game::tests::data_from(DATA);
        let names = ROOTS
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        let (kinds, missing) = KindTable::closure(&data, &names);
        assert!(missing.is_empty(), "{missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
        let power_reload = power_names.reload_table(&data.special_powers);
        let spawns: Vec<_> = self
            .placed
            .iter()
            .map(|(template, x, y, owner)| {
                let kind = kinds.kind(template).unwrap();
                kinds
                    .spawn(&data.combat, kind, Vec2::new(*x, *y), Facing::EAST, *owner)
                    .unwrap()
            })
            .collect();
        let waypoints: BTreeMap<String, (Vec2, f32)> = self
            .waypoints
            .iter()
            .map(|(name, x, y)| ((*name).to_owned(), (Vec2::new(*x, *y), 0.0)))
            .collect();
        let mut teams = self.teams;
        if !teams.iter().any(|def| def.name == "teamWilds") {
            teams.push(team("Wilds", "teamWilds", true, &[]));
        }
        let tree = ScriptTree {
            player_slots: vec![self.scripts.into_iter().map(ScriptNode::Script).collect()],
        };
        let engine = MapScriptEngine::build(
            &[tree],
            &[vec![String::from("Wilds")]],
            &waypoints,
            &BTreeMap::new(),
            false,
        )
        .with_team_prototypes(TeamProtos::from_defs(&teams).unwrap())
        .with_path_points(self.paths);
        let map = HeightMapData {
            width: 96,
            height: 96,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![0; 96 * 96],
        };
        let mut rules = RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), spawns)
            .with_players(2)
            .with_teams(vec![None, None])
            .with_map_players(
                2,
                vec![String::from("Wilds")],
                vec![crate::alliance::MapRelations::default()],
            )
            .with_map_scripts(
                Some(engine),
                vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
            )
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_economy(production.buildables.clone(), 1_000)
            .with_powers(power_names, power_reload)
            .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
        if self.computer {
            rules = rules.with_computer_sides(vec![crate::economy::ComputerSide {
                player: SIDE,
                evil: true,
                cash: 3_000,
            }]);
        }
        if self.fudge > 0.0 {
            rules = rules.with_group_formation(crate::rules::GroupFormation {
                skirmish_group_fudge: self.fudge,
                ..crate::rules::GroupFormation::default()
            });
        }
        if self.ai_seat {
            rules = rules.with_skirmish_ai(
                vec![crate::skirmish_ai::AiSeed {
                    player: SIDE,
                    candidates: Vec::new(),
                }],
                crate::combat::AiTiming::default(),
                Vec::new(),
            );
        }
        if self.fire {
            let material = FireMaterial {
                fuel: 100,
                max_burn_rate: 20,
                decay: 0,
                resistance: 0,
            };
            let system = FireLogicSystem {
                cell_types: [
                    FireMaterial::default(),
                    material,
                    FireMaterial::default(),
                    FireMaterial::default(),
                ],
                ..FireLogicSystem::default()
            };
            rules = rules.with_fire_grid(FireGrid::build(10, 10, |_, _| Some(1), &system));
        }
        Simulation::new(5, rules).unwrap()
    }
}

pub(super) fn team(owner: &str, name: &str, singleton: bool, extra: &[(&str, Value)]) -> TeamDef {
    TeamDef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        singleton,
        properties: Dictionary::from_pairs(
            extra
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        ),
    }
}

fn counted(sim: &Simulation<RotwkRules>, name: &str) -> i32 {
    let state = state_of(sim);
    state.counter_value("Wilds", name) + state.counter_value("", name)
}

fn allowed(sim: &Simulation<RotwkRules>, player: PlayerId, template: &str) -> bool {
    let ids = sim.rules().ids().unwrap();
    let kind = sim.rules().kinds.kind(template).unwrap();
    sim.rules()
        .allowed_to_build(sim.state(), ids, player, kind, false)
}

fn set_buildable(template: &str, status: &str) -> mint_sage_map::Script {
    once(
        "Set",
        vec![act(
            "TECHTREE_MODIFY_BUILDABILITY_OBJECT",
            vec![text(template), text(status)],
        )],
    )
}

#[test]
fn a_template_set_to_no_is_refused_to_every_player() {
    let mut sim = rig(vec![set_buildable("ZSoldier", "No")]).start();
    assert!(allowed(&sim, P0, "ZSoldier") && allowed(&sim, P1, "ZSoldier"));
    run(&mut sim, 10);
    assert!(!allowed(&sim, P0, "ZSoldier"));
    assert!(!allowed(&sim, P1, "ZSoldier"));
    assert!(allowed(&sim, P0, "ZTower"));
}

#[test]
fn only_by_ai_refuses_a_human_and_not_a_computer_player() {
    let mut sim = rig(vec![set_buildable("ZSoldier", "Only_By_AI")]).start();
    run(&mut sim, 10);
    assert!(!allowed(&sim, P0, "ZSoldier"));
    assert!(allowed(&sim, SIDE, "ZSoldier"));
}

fn kills_seen(killer: PlayerId, victim: PlayerId) -> i32 {
    let mut setup = rig(vec![counting(
        "seen",
        (
            "PLAYER_HAS_KILLED_TYPE_UNITS",
            vec![text("<All Players>"), number(1), text("ZVictim")],
        ),
    )]);
    setup.placed = vec![
        ("ZKiller", 200.0, 200.0, killer),
        ("ZVictim", 230.0, 200.0, victim),
    ];
    let mut sim = setup.start();
    run(&mut sim, 400);
    counted(&sim, "seen")
}

#[test]
fn killed_type_units_reads_only_the_first_player_of_the_mask() {
    assert_eq!(kills_seen(P0, P1), 1);
    assert_eq!(kills_seen(P1, P0), 0);
}

#[test]
fn a_not_ready_power_answer_is_cached_for_ten_seconds() {
    let mut setup = rig(vec![
        counting(
            "seen",
            (
                "SKIRMISH_SPECIAL_POWER_READY",
                vec![text("<All Players>"), text("ZPower")],
            ),
        ),
        script(
            "Late",
            Some(("TIMER_EXPIRED", vec![text("late")])),
            vec![act(
                "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
                vec![text("U"), text("ZCaster"), text("teamPlayer_1"), text("P1")],
            )],
            true,
        ),
        once("Arm", vec![act("SET_TIMER", vec![text("late"), number(3)])]),
    ]);
    setup.teams = vec![team("", "teamPlayer_1", true, &[])];
    let mut sim = setup.start();
    run(&mut sim, 150);
    assert_eq!(counted(&sim, "seen"), 0, "the cache still stands");
    run(&mut sim, 300);
    assert_eq!(counted(&sim, "seen"), 1, "the cache has lapsed");
}
