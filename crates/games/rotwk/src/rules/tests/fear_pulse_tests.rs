use super::*;

fn data() -> String {
    String::from(
        "ModifierList ZResistTerror\n  Category = LEADERSHIP\n  \
           Modifier = RESIST_TERROR 50%\n  Duration = 0\nEnd\n\
         EmotionNugget ZTerror\n  \
           Type = TERROR\n  ModelConditions = EMOTION_TERROR\n  Duration = 3000\nEnd\n\
         Object ZRadiator\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE SCARY\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Behavior = RadiateFearUpdate Tag_Fear\n    InitiallyActive = Yes\n    \
             EmotionPulseRadius = 100\n    EmotionPulseInterval = 1000\n    \
             GenerateTerror = Yes\n    VictimFilter = ANY +INFANTRY\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZStatue\n  Side = Neutral\n  KindOf = SELECTABLE IMMOBILE STRUCTURE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
           Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    \
             BonusName = ZResistTerror\n    RefreshDelay = 1000\n    Range = 200\n    \
             ObjectFilter = ANY +INFANTRY\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
         Object ZNear\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    AddEmotion = ZTerror\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         ChildObject ZFar ZNear\nEnd\n\
         ChildObject ZGuarded ZNear\nEnd\n\
         Object ZRider\n  Side = Neutral\n  KindOf = CAVALRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    AddEmotion = ZTerror\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object ZImmune\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Behavior = EmotionTrackerUpdate Tag_Emotion\n    \
             TauntAndPointUpdateDelay = 1000\n    ImmuneToFearLevel = 0\n    \
             AddEmotion = ZTerror\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n",
    )
}

const RADIATOR_AT: Vec2 = Vec2::new(300.0, 300.0);
const STATUE_AT: Vec2 = Vec2::new(500.0, 500.0);

fn scene() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data(), &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = mint_sage_ini::ValueErrors::new();
    let mut emotions = mint_sage_ini::EmotionIndex::new();
    emotions.insert_document(&doc, &errors);
    assert!(errors.is_empty());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [
        "ZRadiator",
        "ZStatue",
        "ZNear",
        "ZFar",
        "ZRider",
        "ZImmune",
        "ZGuarded",
    ];
    let places = [
        RADIATOR_AT,
        STATUE_AT,
        Vec2::new(390.0, 300.0),
        Vec2::new(450.0, 300.0),
        Vec2::new(300.0, 390.0),
        Vec2::new(300.0, 210.0),
        Vec2::new(370.0, 370.0),
    ];
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
    let spawns = roots
        .iter()
        .zip(places)
        .map(|(name, at)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    PLAYER,
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
    simulation.tick().unwrap();
    (simulation, ids, entities)
}

fn felt(simulation: &mut Simulation<RotwkRules>, ids: Ids, entity: Entity, frames: u32) -> bool {
    let mut any = false;
    for _ in 0..frames {
        simulation.tick().unwrap();
        any |= simulation.state().get(ids.emotion, entity).is_some();
    }
    any
}

#[test]
fn a_pulse_frightens_inside_its_radius_and_not_beyond() {
    let (mut simulation, ids, entities) = scene();
    let (near, far) = (entities[2], entities[3]);
    assert!(felt(&mut simulation, ids, near, 40));
    assert_eq!(
        simulation.state().get(ids.emotion, near).unwrap().kind,
        mint_sage_ini::EmotionType::Terror.code(),
        "GenerateTerror names the type"
    );
    assert!(
        simulation.state().get(ids.emotion, far).is_none(),
        "150 away is outside EmotionPulseRadius 100"
    );
}

#[test]
fn a_pulse_passes_over_what_its_victim_filter_excludes() {
    let (mut simulation, ids, entities) = scene();
    let rider = entities[4];
    assert!(!felt(&mut simulation, ids, rider, 40));
}

#[test]
fn an_object_immune_to_that_level_of_fear_feels_nothing() {
    let (mut simulation, ids, entities) = scene();
    let immune = entities[5];
    assert!(!felt(&mut simulation, ids, immune, 40));
}

#[test]
fn a_resist_terror_leadership_shrugs_off_a_terror_pulse() {
    let (mut simulation, ids, entities) = scene();
    let guarded = entities[6];
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .rules()
            .bonuses_of(simulation.state(), ids, guarded)
            .resist_terror,
        0.5,
        "the statue's aura reached it"
    );
    assert!(!felt(&mut simulation, ids, guarded, 40));
}
