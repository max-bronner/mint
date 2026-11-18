use super::*;

fn data() -> String {
    String::from(
        "Locomotor ZHooves\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
           TurnTime = 100\nEnd\n\
         EmotionNugget ZBrace\n  \
           Type = BRACE_FOR_BEING_CRUSHED\n  ModelConditions = EMOTION_AFRAID\n  \
           Duration = 3000\nEnd\n\
         Object ZRider\n  Side = Neutral\n  KindOf = CAVALRY SELECTABLE\n  \
           CrusherLevel = 2\n  CrushableLevel = 2\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           LocomotorSet\n    Locomotor = ZHooves\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
           Behavior = NotifyTargetsOfImminentProbableCrushingUpdate Tag_Notify\n    \
             TimeBetweenUpdatesMS = 400\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
         Object ZFootman\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           CrushableLevel = 0\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    FearScanDistance = 10\n    \
             AddEmotion = ZBrace\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data(), &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZRider", "ZFootman"];
    let (kinds, missing) = KindTable::closure(
        &data,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZRider", Vec2::new(150.0, 300.0), PLAYER),
        ("ZFootman", Vec2::new(300.0, 300.0), OTHER),
        ("ZFootman", Vec2::new(250.0, 360.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_emotions(Arc::new(emotions))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let rider = entities[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![rider],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    (simulation, ids, entities[1], entities[2])
}

#[test]
fn only_the_crushable_enemy_ahead_of_a_charging_crusher_braces() {
    let (mut simulation, ids, ahead, aside) = scene();
    let mut braced = false;
    for _ in 0..40 {
        simulation.tick().unwrap();
        braced |= simulation
            .state()
            .get(ids.emotion, ahead)
            .is_some_and(|emotion| {
                emotion.kind == mint_sage_ini::EmotionType::BraceForBeingCrushed.code()
            });
        assert!(
            simulation.state().get(ids.emotion, aside).is_none(),
            "the footman beside the path braced"
        );
    }
    assert!(braced, "the footman ahead never braced");
}
