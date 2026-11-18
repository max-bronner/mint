use super::*;

const DATA: &str = "\
EmotionNugget ZTaunt\n  Type = TAUNT\n  IgnoreIfUnitBusy = Yes\n  Duration = 500\n  \
  InactiveDurationSameType = 60000\nEnd\n\
Object ZTaunter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  VisionRange = 300\n  \
  ShroudClearingRange = 300\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = EmotionTrackerUpdate Tag_Emotion\n    TauntAndPointUpdateDelay = 500\n    \
    TauntAndPointDistance = 200\n    AddEmotion = ZTaunt\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZNear\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFar\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const STEALTH: &str = "  Behavior = InvisibilityUpdate Tag_Stealth\n    \
    InvisibilityNugget\n      InvisibilityType = STEALTH\n      \
      ForbiddenConditions = MOVING FIRING_ANY\n    End\n    \
    UpdatePeriod = 100\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZNear";

fn scene(data: &str) -> (Simulation<RotwkRules>, Ids, [Entity; 3]) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZTaunter", "ZNear", "ZFar"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZTaunter", Vec2::new(300.0, 300.0), PLAYER),
        ("ZNear", Vec2::new(340.0, 300.0), OTHER),
        ("ZFar", Vec2::new(420.0, 300.0), OTHER),
    ]
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
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, [entities[0], entities[1], entities[2]])
}

fn taunting(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<Entity> {
    simulation
        .state()
        .get(ids.emotion, entity)
        .filter(|emotion| emotion.kind == mint_sage_ini::EmotionType::Taunt.code())
        .and_then(|emotion| emotion.cause)
}

#[test]
fn a_candidate_no_nugget_accepts_is_passed_over_for_the_next() {
    let (mut simulation, ids, [taunter, near, far]) = scene(DATA);
    let mut causes = Vec::new();
    for _ in 0..300 {
        simulation.tick().unwrap();
        if let Some(cause) = taunting(&simulation, ids, taunter) {
            if causes.last() != Some(&cause) {
                causes.push(cause);
            }
        }
    }
    assert_eq!(causes, vec![near, far], "taunt causes in order");
}

#[test]
fn a_hidden_undetected_scanner_taunts_nothing() {
    let data = DATA.replacen(
        "  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZNear",
        STEALTH,
        1,
    );
    assert_ne!(data, DATA);
    let (mut simulation, ids, [taunter, ..]) = scene(&data);
    for _ in 0..300 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.stealth, taunter)
            .is_some_and(|stealth| stealth.hidden)
        {
            assert_eq!(
                taunting(&simulation, ids, taunter),
                None,
                "a hidden taunter taunted at frame {}",
                simulation.frame().get()
            );
        }
    }
    assert!(
        simulation
            .state()
            .get(ids.stealth, taunter)
            .is_some_and(|stealth| stealth.hidden),
        "the taunter never hid"
    );
}
