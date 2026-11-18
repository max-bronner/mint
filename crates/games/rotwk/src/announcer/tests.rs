use super::*;
use crate::rules::RotwkRules;
use crate::templates::KindTable;
use mint_movement::{Facing, Vec2};
use mint_sage_ini::eva::EvaEventDef;
use mint_sage_map::HeightMapData;
use mint_sage_passability::Passability;
use mint_sim::Simulation;
use std::sync::Arc;

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

const DATA: &str = "\
Weapon ZDummy\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object ZOwned\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZEnemy\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn flat_map(width: u32, height: u32) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; (width * height) as usize],
    }
}

fn fixture() -> (
    Simulation<RotwkRules>,
    Arc<KindTable>,
    Entity,
    Entity,
    mint_sage_combat::DamageTypeId,
    u16,
) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["ZOwned", "ZEnemy"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let owned_kind = kinds.kind("ZOwned").unwrap();
    let enemy_kind = kinds.kind("ZEnemy").unwrap();
    let spawn = |kind, at, owner| {
        kinds
            .spawn(&data.combat, kind, at, Facing::EAST, owner)
            .unwrap()
    };
    let spawns = vec![
        spawn(owned_kind, Vec2::new(300.0, 300.0), PLAYER),
        spawn(enemy_kind, Vec2::new(320.0, 300.0), OTHER),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (owned, enemy) = (entities[0], entities[1]);
    let pierce = data.combat.damage_types.lookup("PIERCE").unwrap();
    (simulation, kinds, owned, enemy, pierce, owned_kind.0)
}

fn event_def(name: &str) -> EvaEventDef {
    EvaEventDef {
        name: name.to_string(),
        priority: 1,
        time_between_events_ms: 10_000,
        expiration_time_ms: 1_000,
        quiet_time_ms: 0,
        wait_before_playing_ms: 0,
        always_play_from_home_base: false,
        count_as_jump_to_location: false,
        side_sounds: vec![("Men".to_string(), Some(format!("{name}Sound")))],
        blocks: Vec::new(),
    }
}

fn minimal_eva(events: Vec<EvaEventDef>) -> Eva {
    Eva::new(
        &EvaDefs {
            events,
            announcers: Vec::new(),
            malformed: Vec::new(),
        },
        RETAIL_LOGIC_HZ,
    )
}

#[test]
fn on_struck_reports_the_damaged_owner_event_for_the_listeners_own_unit() {
    let (simulation, kind_table, owned, enemy, pierce, owned_kind) = fixture();
    let eva = minimal_eva(vec![event_def("DamagedOwner")]);
    let damaged = eva.id("DamagedOwner").unwrap();
    let mut kinds = BTreeMap::new();
    kinds.insert(
        owned_kind,
        KindEva {
            damaged: Some(damaged),
            ..KindEva::default()
        },
    );
    let mut announcer = Announcer {
        eva,
        kinds,
        second_until: BTreeMap::new(),
        attach_died: BTreeMap::new(),
        counters: Vec::new(),
        logical_fire: None,
        excluded: Vec::new(),
        last_rank: None,
        now: 0,
    };
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };
    let struck = crate::combat_facts::Struck {
        victim: owned,
        attacker: enemy,
        damage_type: pierce,
        dealt: 5.0,
    };
    announcer.on_struck(&facts, 0, &struck);
    assert!(announcer.eva.is_about_to_play(damaged));
}

#[test]
fn on_death_reports_the_die_owner_event_for_the_listeners_own_unit() {
    let (simulation, kind_table, owned, _enemy, _pierce, owned_kind) = fixture();
    let eva = minimal_eva(vec![event_def("DieOwner")]);
    let die_owner = eva.id("DieOwner").unwrap();
    let mut kinds = BTreeMap::new();
    kinds.insert(
        owned_kind,
        KindEva {
            die_owner: Some(die_owner),
            ..KindEva::default()
        },
    );
    let mut announcer = Announcer {
        eva,
        kinds,
        second_until: BTreeMap::new(),
        attach_died: BTreeMap::new(),
        counters: Vec::new(),
        logical_fire: None,
        excluded: Vec::new(),
        last_rank: None,
        now: 0,
    };
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };
    announcer.on_death(&facts, 0, owned);
    assert!(announcer.eva.is_about_to_play(die_owner));
}

#[test]
fn count_kills_reports_once_the_window_reaches_its_minimum() {
    let eva = minimal_eva(vec![event_def("ScoredKill")]);
    let event = eva.id("ScoredKill");
    let mut announcer = Announcer {
        eva,
        kinds: BTreeMap::new(),
        second_until: BTreeMap::new(),
        attach_died: BTreeMap::new(),
        counters: vec![Counter {
            def: mint_sage_ini::eva::ScoredKillEvaAnnouncerDef {
                name: "Test".to_string(),
                event: "ScoredKill".to_string(),
                only_kills_by_local_player: false,
                only_kills_against_local_player: false,
                minimum_count: 3,
                maximum_time_ms: 10_000,
                object_filter: Vec::new(),
            },
            event,
            filter: ObjectFilter::parse(&[]),
            window_frames: 300,
            trackers: vec![Tracker {
                scorer: Some(PLAYER),
                kills: VecDeque::from(vec![
                    (0, [0.0, 0.0, 0.0]),
                    (1, [0.0, 0.0, 0.0]),
                    (2, [0.0, 0.0, 0.0]),
                ]),
            }],
        }],
        logical_fire: None,
        excluded: Vec::new(),
        last_rank: None,
        now: 0,
    };
    announcer.count_kills(10);
    assert!(event.is_some_and(|id| announcer.eva.is_about_to_play(id)));
}

const ATTACH_DATA: &str = "\
Object ZRing\n  Side = Men\n  KindOf = SELECTABLE\n  \
  Behavior = AttachUpdate Tag_Attach\n    ObjectFilter = ANY\n    ScanRange = 50\n    \
    ParentOwnerAttachmentEvaEvent = AttachOwner\n    \
    ParentAllyAttachmentEvaEvent = AttachAlly\n    \
    ParentEnemyAttachmentEvaEvent = AttachEnemy\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n";

fn attach_fixture() -> (Simulation<RotwkRules>, Arc<KindTable>, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        ATTACH_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["ZRing"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let ring_kind = kinds.kind("ZRing").unwrap();
    let spawn = kinds
        .spawn(
            &data.combat,
            ring_kind,
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_players(2)
        .with_map_players(
            2,
            vec!["Ally".to_owned()],
            vec![crate::alliance::MapRelations {
                allies: vec![PLAYER],
                ..crate::alliance::MapRelations::default()
            }],
        );
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let ring = simulation.state().store(ids.body).unwrap().entities()[0];
    (simulation, kinds, ring)
}

fn bare_announcer(eva: Eva) -> Announcer {
    Announcer {
        eva,
        kinds: BTreeMap::new(),
        second_until: BTreeMap::new(),
        attach_died: BTreeMap::new(),
        counters: Vec::new(),
        logical_fire: None,
        excluded: Vec::new(),
        last_rank: None,
        now: 0,
    }
}

const ALLY: PlayerId = PlayerId::new(2);

#[test]
fn on_attach_plays_the_ally_event_for_an_allied_parent_and_enemy_otherwise() {
    let eva = minimal_eva(vec![
        event_def("AttachOwner"),
        event_def("AttachAlly"),
        event_def("AttachEnemy"),
    ]);
    let (owner, ally, enemy) = (
        eva.id("AttachOwner").unwrap(),
        eva.id("AttachAlly").unwrap(),
        eva.id("AttachEnemy").unwrap(),
    );

    let (simulation, kind_table, ring) = attach_fixture();
    let mut announcer = bare_announcer(eva.clone());
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };
    announcer.on_attach(&facts, 0, ring, Some(ALLY));
    assert!(
        !announcer.eva.is_about_to_play(owner),
        "an ally is not the owner"
    );
    assert!(announcer.eva.is_about_to_play(ally));
    assert!(!announcer.eva.is_about_to_play(enemy));

    let (simulation, kind_table, ring) = attach_fixture();
    let mut announcer = bare_announcer(eva);
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };
    announcer.on_attach(&facts, 0, ring, Some(OTHER));
    assert!(!announcer.eva.is_about_to_play(ally), "OTHER is not allied");
    assert!(announcer.eva.is_about_to_play(enemy));
}

#[test]
fn on_defeat_reports_ally_or_enemy_defeated_and_nothing_for_the_listener() {
    let eva = minimal_eva(vec![event_def("AllyDefeated"), event_def("EnemyDefeated")]);
    let (ally_event, enemy_event) = (
        eva.id("AllyDefeated").unwrap(),
        eva.id("EnemyDefeated").unwrap(),
    );
    let (simulation, kind_table, _ring) = attach_fixture();
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_defeat(&facts, 0, ALLY);
    assert!(announcer.eva.is_about_to_play(ally_event));
    assert!(!announcer.eva.is_about_to_play(enemy_event));

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_defeat(&facts, 0, OTHER);
    assert!(
        !announcer.eva.is_about_to_play(ally_event),
        "OTHER is not allied"
    );
    assert!(announcer.eva.is_about_to_play(enemy_event));

    let mut announcer = bare_announcer(eva);
    announcer.on_defeat(&facts, 0, PLAYER);
    assert!(!announcer.eva.is_about_to_play(ally_event));
    assert!(!announcer.eva.is_about_to_play(enemy_event));
}

#[test]
fn on_last_member_death_reports_the_single_event_for_the_members_kind() {
    let (simulation, kind_table, owned, _enemy, _pierce, owned_kind) = fixture();
    let eva = minimal_eva(vec![event_def("LastMemberDeath")]);
    let last_member_death = eva.id("LastMemberDeath").unwrap();
    let mut kinds = BTreeMap::new();
    kinds.insert(
        owned_kind,
        KindEva {
            last_member_death: Some(last_member_death),
            ..KindEva::default()
        },
    );
    let mut announcer = Announcer {
        eva,
        kinds,
        second_until: BTreeMap::new(),
        attach_died: BTreeMap::new(),
        counters: Vec::new(),
        logical_fire: None,
        excluded: Vec::new(),
        last_rank: None,
        now: 0,
    };
    let facts = AnnouncerFacts {
        local: PLAYER,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };
    announcer.on_last_member_death(&facts, 0, owned);
    assert!(announcer.eva.is_about_to_play(last_member_death));
}

const DETECTION_DATA: &str = "\
Object ZCloaked\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Behavior = InvisibilityUpdate Tag_Stealth\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n    End\n    \
    UpdatePeriod = 1\n    StartsActive = Yes\n    \
    EvaEventDetectedEnemy = DetectedEnemy\n    EvaEventDetectedAlly = DetectedAlly\n    \
    EvaEventDetectedOwner = DetectedOwner\n  End\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZDetector\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn detection_fixture() -> (Simulation<RotwkRules>, Arc<KindTable>, Entity, Entity) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DETECTION_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["ZCloaked", "ZDetector"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let cloaked_kind = kinds.kind("ZCloaked").unwrap();
    let detector_kind = kinds.kind("ZDetector").unwrap();
    let spawn = |kind, at, owner| {
        kinds
            .spawn(&data.combat, kind, at, Facing::EAST, owner)
            .unwrap()
    };
    let spawns = vec![
        spawn(cloaked_kind, Vec2::new(300.0, 300.0), PLAYER),
        spawn(detector_kind, Vec2::new(320.0, 300.0), OTHER),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_players(2)
        .with_map_players(
            2,
            vec!["Ally".to_owned()],
            vec![crate::alliance::MapRelations {
                allies: vec![PLAYER],
                ..crate::alliance::MapRelations::default()
            }],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, kinds, entities[0], entities[1])
}

#[test]
fn on_detection_picks_owner_ally_or_enemy_by_the_local_players_relationship() {
    let (simulation, kind_table, cloaked, detector) = detection_fixture();
    let eva = minimal_eva(vec![
        event_def("DetectedOwner"),
        event_def("DetectedAlly"),
        event_def("DetectedEnemy"),
    ]);
    let (owner_event, ally_event, enemy_event) = (
        eva.id("DetectedOwner").unwrap(),
        eva.id("DetectedAlly").unwrap(),
        eva.id("DetectedEnemy").unwrap(),
    );
    let facts_for = |local| AnnouncerFacts {
        local,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_detection(&facts_for(PLAYER), 0, cloaked, detector);
    assert!(announcer.eva.is_about_to_play(owner_event));
    assert!(!announcer.eva.is_about_to_play(ally_event));
    assert!(!announcer.eva.is_about_to_play(enemy_event));

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_detection(&facts_for(ALLY), 0, cloaked, detector);
    assert!(!announcer.eva.is_about_to_play(owner_event));
    assert!(announcer.eva.is_about_to_play(ally_event));
    assert!(!announcer.eva.is_about_to_play(enemy_event));

    let mut announcer = bare_announcer(eva);
    announcer.on_detection(&facts_for(OTHER), 0, cloaked, detector);
    assert!(!announcer.eva.is_about_to_play(owner_event));
    assert!(!announcer.eva.is_about_to_play(ally_event));
    assert!(announcer.eva.is_about_to_play(enemy_event));
}

#[test]
fn on_wall_climb_requires_a_hostile_climber() {
    let (simulation, kind_table, wall, climber) = detection_fixture();
    let eva = minimal_eva(vec![event_def("WallsBeingClimbed")]);
    let event = eva.id("WallsBeingClimbed").unwrap();
    let facts_for = |local| AnnouncerFacts {
        local,
        units: &[],
        kinds: kind_table.as_ref(),
        rules: simulation.rules(),
        world: simulation.state(),
        frame: mint_sim::Frame::new(0),
        defeats: &[],
    };

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_wall_climb(&facts_for(PLAYER), 0, climber, wall);
    assert!(
        announcer.eva.is_about_to_play(event),
        "PLAYER's own wall, climbed by hostile OTHER, must raise WallsBeingClimbed"
    );

    let mut announcer = bare_announcer(eva.clone());
    announcer.on_wall_climb(&facts_for(OTHER), 0, climber, wall);
    assert!(
        !announcer.eva.is_about_to_play(event),
        "a wall not the viewer's own must raise nothing"
    );

    let mut announcer = bare_announcer(eva);
    announcer.on_wall_climb(&facts_for(PLAYER), 0, wall, wall);
    assert!(
        !announcer.eva.is_about_to_play(event),
        "a climber allied (here, identical) to the wall's owner must raise nothing"
    );
}
