use super::*;
use crate::rules::debug::{DebugCommand, DebugTarget, SpawnRole};

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 30\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Spear\n  AttackRange = 10\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE CAN_BE_REPULSED\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pikeman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE CAN_BE_REPULSED\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Villager\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Ghost\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ArcherHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Archer Position:X:50 Y:0 Position:X:50 Y:20 Position:X:50 Y:-20\n  End\nEnd\n\
Object CreepingHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 3\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Archer Position:X:50 Y:0 Position:X:50 Y:20 Position:X:50 Y:-20\n  End\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Pikeman Position:X:50 Y:0 Position:X:50 Y:20 Position:X:50 Y:-20\n  End\nEnd\n\
Object CrowdHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE CAN_BE_REPULSED\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Villager Position:X:50 Y:0 Position:X:50 Y:20 Position:X:50 Y:-20\n  End\nEnd\n";

const HORDE_AT: Vec2 = Vec2::new(400.0, 400.0);
const GHOST_AT: Vec2 = Vec2::new(440.0, 400.0);

struct Rig {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    kinds: Arc<KindTable>,
    ghost: Entity,
}

fn rig() -> Rig {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [
        "ArcherHorde",
        "CreepingHorde",
        "PikeHorde",
        "CrowdHorde",
        "Ghost",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let ghost = kinds
        .spawn(
            &data.combat,
            kinds.kind("Ghost").unwrap(),
            GHOST_AT,
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![ghost])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(Vec::new(), 0)
        .with_menus(production.menus.clone())
        .with_map_scripts(
            Some(crate::rules::map_scripts::MapScriptEngine::build(
                &[mint_sage_map::ScriptTree {
                    player_slots: vec![Vec::new()],
                }],
                &[Vec::new()],
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                false,
            )),
            vec![(0, PLAYER)],
        );
    let simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let ghost = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    Rig {
        simulation,
        ids,
        kinds,
        ghost,
    }
}

impl Rig {
    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.simulation.tick().unwrap();
        }
    }

    fn spawn_horde(&mut self, name: &str) -> Entity {
        let kind = self.kinds.kind(name).unwrap().0;
        self.simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::Spawn {
                    kind,
                    x: HORDE_AT.x,
                    y: HORDE_AT.y,
                    facing: 0.0,
                    role: SpawnRole::Normal,
                }),
            )
            .unwrap();
        self.run(40);
        self.simulation
            .state()
            .store(self.ids.horde)
            .unwrap()
            .entities()
            .last()
            .copied()
            .unwrap()
    }

    fn members(&self, horde: Entity) -> Vec<Entity> {
        self.simulation
            .state()
            .get(self.ids.horde, horde)
            .unwrap()
            .members
            .clone()
    }

    fn dirty(&self, horde: Entity) -> bool {
        self.simulation
            .state()
            .get(self.ids.horde, horde)
            .unwrap()
            .dirty
    }

    fn position(&self, who: Entity) -> Vec2 {
        self.simulation
            .state()
            .get(self.ids.body, who)
            .unwrap()
            .0
            .position
    }

    fn make_ghost_a_repulsor(&mut self) {
        let ghost = self.ghost;
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ghost],
            },
            RotwkCommand::Debug(DebugCommand::SetStatus {
                status: "REPULSOR".to_owned(),
            }),
        ] {
            self.simulation.submit(PLAYER, command).unwrap();
        }
    }

    fn repulsed(&self, who: Entity) -> bool {
        self.simulation.state().contains(self.ids.repulsed, who)
    }

    fn any_repulsed(&self, horde: Entity) -> bool {
        self.members(horde).into_iter().any(|m| self.repulsed(m))
    }
}

#[test]
fn a_can_be_repulsed_horde_object_is_idled_into_the_flee() {
    let mut rig = rig();
    let horde = rig.spawn_horde("CrowdHorde");
    assert!(!rig.repulsed(horde), "nothing repulses it yet");
    rig.make_ghost_a_repulsor();
    rig.run(40);
    assert!(
        rig.simulation
            .state()
            .get(rig.ids.repulsed, horde)
            .is_some_and(|state| state.is_fleeing()),
        "the horde object enters the flee"
    );
    assert!(!rig.any_repulsed(horde), "its members lack the KindOf");
}

#[test]
fn a_horde_member_at_rest_with_a_ranged_weapon_takes_the_repulsor_check() {
    let mut rig = rig();
    let horde = rig.spawn_horde("ArcherHorde");
    assert!(!rig.dirty(horde), "the horde has settled on its slots");
    rig.make_ghost_a_repulsor();
    rig.run(40);
    assert!(rig.any_repulsed(horde));
}

#[test]
fn a_horde_member_holding_a_melee_weapon_skips_the_repulsor_check() {
    let mut rig = rig();
    let horde = rig.spawn_horde("PikeHorde");
    assert!(!rig.dirty(horde), "the horde has settled on its slots");
    rig.make_ghost_a_repulsor();
    rig.run(40);
    assert!(!rig.any_repulsed(horde));
}

#[test]
fn a_horde_member_walking_to_its_slot_skips_the_repulsor_check_while_the_others_ask() {
    let mut rig = rig();
    let horde = rig.spawn_horde("ArcherHorde");
    let straggler = rig.members(horde)[0];
    let from = rig.position(straggler);
    let archer = rig.kinds.kind("Archer").unwrap().0;
    rig.simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Teleport {
                target: DebugTarget::Near {
                    kind: archer,
                    x: from.x,
                    y: from.y,
                },
                x: 400.0,
                y: 480.0,
            }),
        )
        .unwrap();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![straggler],
        },
        RotwkCommand::Debug(DebugCommand::MarkHordeDirty),
    ] {
        rig.simulation.submit(PLAYER, command).unwrap();
    }
    rig.run(3);
    rig.make_ghost_a_repulsor();
    rig.run(40);
    assert!(
        !rig.repulsed(straggler),
        "the straggler is still walking to its slot"
    );
    assert!(
        rig.members(horde)
            .into_iter()
            .any(|member| member != straggler && rig.repulsed(member)),
        "the settled members ask"
    );
}

#[test]
fn a_member_of_a_hunting_horde_skips_the_repulsor_check() {
    let mut rig = rig();
    let horde = rig.spawn_horde("ArcherHorde");
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![horde],
        },
        RotwkCommand::Debug(DebugCommand::HuntWhenIdle),
    ] {
        rig.simulation.submit(PLAYER, command).unwrap();
    }
    rig.make_ghost_a_repulsor();
    rig.run(40);
    assert!(!rig.any_repulsed(horde));
}

#[test]
fn a_member_aligned_in_its_slot_of_a_marching_horde_takes_the_repulsor_check() {
    let mut rig = rig();
    let horde = rig.spawn_horde("CreepingHorde");
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![horde],
        },
        RotwkCommand::MoveTo {
            x: 400.0,
            y: 100.0,
            queue: false,
        },
    ] {
        rig.simulation.submit(PLAYER, command).unwrap();
    }
    rig.run(20);
    assert!(rig.simulation.state().contains(rig.ids.path, horde));
    rig.make_ghost_a_repulsor();
    rig.run(30);
    assert!(rig.any_repulsed(horde));
}
