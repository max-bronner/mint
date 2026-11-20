use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Pike\n  AttackRange = 25\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
  Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 10\n    TriggerWidthY = 10\n  End\n  \
  Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
    ResetTimeInMilliseconds = 1000\n  End\n  \
  Geometry = BOX\n  GeometryName = Closed\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 20\n  \
  AdditionalGeometry = BOX\n  GeometryName = OpenLeft\n  GeometryActive = No\n  \
  GeometryMajorRadius = 10\n  GeometryMinorRadius = 3\n  GeometryHeight = 20\n  \
  GeometryOffset = X:0 Y:30 Z:0\nEnd\n\
Object Raider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 20\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Pike\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Orcs\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 20\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Rig {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    raider: Entity,
    quarry: Entity,
}

fn rig() -> Rig {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Gate", "Raider", "Quarry"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: Vec2, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn("Gate", Vec2::new(100.0, 100.0), PLAYER),
        spawn("Raider", Vec2::new(40.0, 100.0), PLAYER),
        spawn("Quarry", Vec2::new(220.0, 100.0), OTHER),
    ];
    let records = crate::construction::shown_footprint(
        &kinds.get(kinds.kind("Gate").unwrap()).unwrap().record,
        &[],
    );
    let passability = Passability::build(&flat_map(60, 60), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![Some(0), Some(1)])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_structure_footprints(vec![(0, records)])
        .with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    let mut simulation = Simulation::new(71, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    Rig {
        simulation,
        ids,
        raider: entities[1],
        quarry: entities[2],
    }
}

impl Rig {
    fn position(&self) -> Vec2 {
        position(&self.simulation, self.ids, self.raider)
    }

    fn order_attack(&mut self) {
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![self.raider],
            },
            RotwkCommand::AttackObject {
                target: self.quarry,
            },
        ] {
            self.simulation.submit(PLAYER, command).unwrap();
        }
    }
}

#[test]
fn an_attacker_reaching_a_type_seven_node_stops_attacking() {
    let mut rig = rig();
    rig.order_attack();
    let mut halted_at = None;
    for _ in 0..240 {
        rig.simulation.tick().unwrap();
        if !rig
            .simulation
            .state()
            .contains(rig.ids.attack_order, rig.raider)
        {
            halted_at = Some(rig.position());
            break;
        }
    }
    let at = halted_at.expect("the attack order outlived the halt");
    assert!(
        at.minus(Vec2::new(80.0, 100.0)).length() < 8.0,
        "stopped at {at:?}, not at the gate's near node"
    );
    for _ in 0..60 {
        rig.simulation.tick().unwrap();
    }
    assert!(!rig
        .simulation
        .state()
        .contains(rig.ids.attack_order, rig.raider));
    assert!(rig.position().x < 100.0, "crossed the gate");
}
