use super::*;

const DATA: &str = "\
Locomotor Hover\n  Surfaces = GROUND\n  Acceleration = 100\n  Braking = 100\n  \
  TurnTime = 500\n  PreferredHeight = 10\n  \
  ZAxisBehavior = FIXED_RELATIVE_TO_GROUND_AND_BUILDINGS\nEnd\n\
Object Hoverer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Hover\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Tower\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 30\nEnd\n";

const AT: Vec2 = Vec2::new(100.0, 100.0);

fn stands_at(tower_gap: Option<f32>) -> (f32, f32) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Hoverer", "Tower"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |template: &str, at: Vec2| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(template).unwrap(),
                at,
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    };
    let mut spawns = vec![spawn("Hoverer", AT)];
    if let Some(gap) = tower_gap {
        spawns.push(spawn("Tower", Vec2::new(AT.x + gap, AT.y)));
    }
    let mut map = flat_map(32, 32);
    map.samples = vec![1280; map.samples.len()];
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hoverer = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation.tick().unwrap();
    (
        simulation.state().get(ids.elevation, hoverer).unwrap().0,
        simulation.rules().ground_height(AT.x, AT.y),
    )
}

#[test]
fn a_body_over_buildings_stands_at_its_height_plus_the_tallest_structure_beside_it() {
    let (open, ground) = stands_at(None);
    assert!(ground > 30.0, "the premise: high terrain, {ground}");
    assert_eq!(open, 10.0, "over open ground it stands at PreferredHeight");
    assert_eq!(stands_at(Some(3.0)).0, 40.0, "beside a 30-high tower");
    assert_eq!(
        stands_at(Some(40.0)).0,
        10.0,
        "a tower out of reach adds nothing"
    );
}
