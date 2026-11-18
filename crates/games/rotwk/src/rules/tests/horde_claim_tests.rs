use super::*;
use crate::rules::debug::{DebugCommand, DebugTarget, SpawnRole};

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Villager\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Ring\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 50\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:40 Y:40 Position:X:40 Y:-40 \
Position:X:-40 Y:40 Position:X:-40 Y:-40\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 60\n  GeometryHeight = 20\nEnd\n\
Object Cohort\n  Side = Men\n  KindOf = SELECTABLE HORDE LARGE_RECTANGLE_PATHFIND\n  \
  CommandSet = CampSet\n  BuildCost = 100\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 50\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 10\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-20 Position:X:10 Y:-10 \
Position:X:10 Y:0 Position:X:10 Y:10 Position:X:10 Y:20\n    \
    RankInfo = RankNumber:2 UnitType:Soldier Position:X:-10 Y:-20 Position:X:-10 Y:-10 \
Position:X:-10 Y:0 Position:X:-10 Y:10 Position:X:-10 Y:20\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 20\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    ExitDelay = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Cohort\nEnd\n\
CommandButton Build_Cohort\n  Command = UNIT_BUILD\n  Object = Cohort\nEnd\n";

struct Rig {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    kinds: Arc<KindTable>,
}

fn rig(placed: &[(&str, Vec2)]) -> Rig {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Camp", "Ring", "Cohort", "Soldier", "Villager"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let footprints = placed
        .iter()
        .enumerate()
        .map(|(at, (name, _))| {
            let record = &kinds.get(kinds.kind(name).unwrap()).unwrap().record;
            (at, crate::construction::footprint_of(record))
        })
        .collect();
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, at)| {
            let kind = kinds.kind(name).unwrap();
            let mut spawn = kinds
                .spawn(&data.combat, kind, *at, Facing::EAST, PLAYER)
                .unwrap();
            spawn.producer = kinds.producer(&production, kind, *at, Facing::EAST);
            spawn
        })
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_structure_footprints(footprints)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_players(2);
    let simulation = Simulation::new(37, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    Rig {
        simulation,
        ids,
        kinds,
    }
}

impl Rig {
    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.simulation.tick().unwrap();
        }
    }

    fn submit(&mut self, command: RotwkCommand) {
        self.simulation.submit(PLAYER, command).unwrap();
    }

    fn spawn(&mut self, name: &str, at: Vec2) {
        let kind = self.kinds.kind(name).unwrap().0;
        self.submit(RotwkCommand::Debug(DebugCommand::Spawn {
            kind,
            x: at.x,
            y: at.y,
            facing: 0.0,
            role: SpawnRole::Normal,
        }));
        self.run(2);
    }

    fn live_of(&self, name: &str) -> Vec<Entity> {
        let kind = self.kinds.kind(name).unwrap();
        self.simulation
            .state()
            .store(self.ids.kind)
            .unwrap()
            .iter()
            .filter(|(entity, k)| **k == kind && self.simulation.state().is_live(*entity))
            .map(|(entity, _)| entity)
            .collect()
    }

    fn members(&self, horde: Entity) -> Vec<Entity> {
        self.simulation
            .state()
            .get(self.ids.horde, horde)
            .unwrap()
            .members
            .clone()
    }

    fn position(&self, who: Entity) -> Vec2 {
        position(&self.simulation, self.ids, who)
    }
}

#[test]
fn hordes_ordered_to_one_point_claim_member_footprints_and_no_horde_body() {
    let mut rig = rig(&[]);
    rig.spawn("Ring", Vec2::new(400.0, 705.0));
    rig.spawn("Ring", Vec2::new(150.0, 705.0));
    rig.spawn("Villager", Vec2::new(300.0, 1000.0));
    let rings = {
        let mut rings = rig.live_of("Ring");
        rings.sort_by_key(|horde| horde.index());
        rings
    };
    let soldier = rig.live_of("Villager")[0];
    let ring_members: Vec<Vec<Entity>> = rings.iter().map(|ring| rig.members(*ring)).collect();
    assert!(ring_members.iter().all(|members| members.len() == 4));
    let click = Vec2::new(900.0, 705.0);
    let units: Vec<Entity> = ring_members
        .iter()
        .flatten()
        .copied()
        .chain([soldier])
        .collect();
    rig.submit(RotwkCommand::SetSelection {
        replace: true,
        units,
    });
    rig.submit(RotwkCommand::MoveTo {
        x: click.x,
        y: click.y,
        queue: false,
    });
    rig.run(1200);
    let apart = rig
        .position(rings[0])
        .minus(rig.position(rings[1]))
        .length();
    assert!(
        apart >= 80.0,
        "the second horde stands {apart} from the first"
    );
    let at = rig.position(soldier);
    assert!(
        at.minus(click).length() < 25.0,
        "the soldier stands {} from the click, wanted inside the first ring",
        at.minus(click).length()
    );
}

#[test]
fn a_produced_hordes_rectangle_counts_members_still_in_the_door() {
    let camp = Vec2::new(300.0, 300.0);
    let click = Vec2::new(700.0, 300.0);
    let mut rig = rig(&[("Camp", camp), ("Villager", Vec2::new(670.0, 300.0))]);
    let camp_entity = rig.live_of("Camp")[0];
    rig.submit(RotwkCommand::SetSelection {
        replace: true,
        units: vec![camp_entity],
    });
    rig.submit(RotwkCommand::QueueUnit { button: 0 });
    let mut horde = None;
    for _ in 0..90 {
        rig.run(1);
        horde = rig
            .simulation
            .state()
            .store(rig.ids.horde)
            .and_then(|store| store.entities().first().copied());
        if horde.is_some() {
            break;
        }
    }
    let horde = horde.expect("the horde was produced");
    let cohort = rig.kinds.kind("Cohort").unwrap().0;
    rig.submit(RotwkCommand::Debug(DebugCommand::AdjustDestination {
        target: DebugTarget::Kind {
            kind: cohort,
            side: None,
        },
        x: click.x,
        y: click.y,
    }));
    rig.run(1);
    let stream = rig
        .simulation
        .state()
        .get(rig.ids.production_stream, horde)
        .expect("members are still in the door");
    assert!(stream.remaining >= 5, "{} left", stream.remaining);
    assert!(
        rig.position(horde).minus(click).length() > 15.0,
        "answered at the click {:?}, inside nothing",
        rig.position(horde)
    );
}
