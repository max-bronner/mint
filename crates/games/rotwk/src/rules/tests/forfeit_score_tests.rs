use super::*;
use crate::rules::difficulty::{DifficultyPlayer, DifficultySetup};

const ME: PlayerId = PlayerId::new(0);
const ALLY: PlayerId = PlayerId::new(1);
const FOE: PlayerId = PlayerId::new(2);
const NEUTRAL: PlayerId = PlayerId::new(3);

const DATA: &str = "\
Upgrade Upgrade_Blades\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_RingHero\n  Type = PLAYER\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Object Barracks\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Horde\n  2 = Build_Overloaded\nEnd\n\
CommandButton Build_Overloaded\n  Command = UNIT_BUILD\n  Object = OverloadedHorde\nEnd\n\
CommandButton Build_Horde\n  Command = UNIT_BUILD\n  Object = Horde\nEnd\n\
Object Forge\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = ForgeSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
CommandSet ForgeSet\n  1 = Buy_Ring\nEnd\n\
CommandButton Buy_Ring\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_RingHero\nEnd\n\
Object Soldier\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Horde\n  KindOf = SELECTABLE HORDE CAN_ATTACK\n  CommandSet = HordeSet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = StatusBitsUpgrade ModuleTag_Legality\n    TriggeredBy = Upgrade_Blades\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10\n  \
  End\nEnd\n\
Object OverloadedHorde\n  KindOf = SELECTABLE HORDE CAN_ATTACK\n  CommandSet = HordeSet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = StatusBitsUpgrade ModuleTag_Legality\n    TriggeredBy = Upgrade_Blades\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10\n    \
    LivingWorldOverloadTemplate = Soldier\n  \
  End\nEnd\n\
CommandSet HordeSet\n  1 = Buy_Blades\nEnd\n\
CommandButton Buy_Blades\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Blades\nEnd\n\
Object Farm\n  KindOf = STRUCTURE ECONOMY_STRUCTURE SELECTABLE IMMOBILE\n  BuildCost = 50\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Flag\n  KindOf = CAPTUREFLAG SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Stronghold\n  KindOf = STRUCTURE DOZER_FACTORY SELECTABLE IMMOBILE\n  BuildCost = 1000\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Outpost\n  KindOf = STRUCTURE DOZER_FACTORY SUMMONED SELECTABLE IMMOBILE\n  BuildCost = 400\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n";

struct World {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
}

fn world(placed: &[(&str, PlayerId)]) -> World {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Barracks", "Forge", "Farm", "Flag", "Stronghold", "Outpost"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let upgrade_menus = kinds.upgrades(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .enumerate()
        .map(|(index, (name, owner))| {
            let kind = kinds.kind(name).unwrap();
            let at = Vec2::new(20.0 + 12.0 * index as f32, 20.0);
            let mut spawn = kinds
                .spawn(&data.combat, kind, at, Facing::EAST, *owner)
                .unwrap();
            spawn.producer = kinds.producer(&production, kind, at, Facing::EAST);
            spawn
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let difficulty = DifficultySetup {
        multiplayer: true,
        game: crate::lobby::Difficulty::Medium,
        players: [ME, ALLY, FOE, NEUTRAL]
            .into_iter()
            .map(|player| DifficultyPlayer {
                player,
                human: false,
                playable: player != NEUTRAL,
                ai: None,
            })
            .collect(),
    };
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_horde_overloaded(data.formations.overloaded().to_vec())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_players(4)
        .with_teams(vec![Some(0), Some(0), Some(1), Some(1)])
        .with_difficulty(difficulty);
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    World { simulation, ids }
}

impl World {
    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.simulation.tick().unwrap();
        }
    }

    fn sides(&self, player: PlayerId) -> crate::rules::ai_forfeit::ForfeitSides {
        self.simulation
            .rules()
            .forfeit_sides(self.simulation.state(), self.ids, player)
    }

    fn first_of(&self, template: &str, owner: PlayerId) -> Entity {
        let kind = self.simulation.rules().kinds.kind(template).unwrap();
        let state = self.simulation.state();
        state
            .store(self.ids.kind)
            .unwrap()
            .iter()
            .find(|(entity, k)| {
                **k == kind && state.get(self.ids.owner, *entity).map(|o| o.0) == Some(owner)
            })
            .map(|(entity, _)| entity)
            .unwrap()
    }

    fn press(&mut self, owner: PlayerId, unit: Entity, command: RotwkCommand) {
        self.simulation
            .submit(
                owner,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![unit],
                },
            )
            .unwrap();
        self.simulation.submit(owner, command).unwrap();
        self.run(35);
    }
}

#[test]
fn a_horde_counts_once_at_its_own_cost_plus_the_upgrades_it_holds() {
    let mut w = world(&[("Barracks", ME)]);
    let recipe = w
        .simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Horde")
        .unwrap()
        .index;
    w.simulation
        .submit(
            ME,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    w.run(2);
    assert_eq!(w.sides(ME).own.quality, 300.0);
    let horde = w.first_of("Horde", ME);
    w.press(ME, horde, RotwkCommand::ObjectUpgrade { button: 0 });
    let sides = w.sides(ME);
    assert_eq!((sides.own.quality, sides.enemy.quality), (400.0, 0.0));
}

#[test]
fn a_horde_naming_a_living_world_overload_template_takes_no_upgrade_term() {
    let mut w = world(&[("Barracks", ME)]);
    let recipe = w
        .simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "OverloadedHorde")
        .unwrap()
        .index;
    w.simulation
        .submit(
            ME,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    w.run(2);
    let horde = w.first_of("OverloadedHorde", ME);
    w.press(ME, horde, RotwkCommand::ObjectUpgrade { button: 0 });
    assert_eq!(w.sides(ME).own.quality, 300.0);
}

#[test]
fn map_control_counts_economy_structures_and_allied_flags_of_playable_players_only() {
    let mut placed = vec![("Flag", ALLY), ("Flag", FOE)];
    placed.extend([("Farm", ME); 10]);
    placed.push(("Farm", FOE));
    placed.extend([("Farm", NEUTRAL); 5]);
    let w = world(&placed);
    let sides = w.sides(ME);
    assert_eq!((sides.own.map_control, sides.enemy.map_control), (12, 2));
}

#[test]
fn the_ring_term_and_the_dozer_factory_constant_follow_alliance_and_the_structure_lists() {
    let mut w = world(&[("Forge", ALLY), ("Stronghold", ALLY), ("Outpost", ALLY)]);
    assert!(!w.sides(ME).ring);
    let forge = w.first_of("Forge", ALLY);
    w.press(ALLY, forge, RotwkCommand::PlayerUpgrade { button: 0 });
    assert!(w.sides(ME).ring, "an ally holds the ring upgrade");
    assert!(w.sides(ALLY).ring, "and so does the holder");
    assert!(!w.sides(FOE).ring, "an enemy of the holder does not see it");
    let sides = w.sides(ME);
    assert_eq!(
        sides.constant, 2.0,
        "the summoned outpost is past the five-kind skip, so it sets nothing"
    );
    assert_eq!((sides.own.base, sides.enemy.base), (1000.0, 0.0));
}
