use super::*;
use mint_sage_formation::{FormationAssistant, FormationTemplate, Selection};

const ANCHOR: Vec2 = Vec2::new(500.0, 400.0);

fn assistant(preview_object: Option<&str>) -> FormationAssistant {
    let text = format!(
        "FormationAssistant\n    UnitDefinition Inf\n        ObjectFilter = NONE +INFANTRY\n{}    End\nEnd\n",
        preview_object
            .map(|name| format!("        PreviewObject = {name}\n"))
            .unwrap_or_default()
    );
    let mut sources = mint_sage_ini::MemorySources::new();
    sources.insert("data/ini/formationassistant.ini", text.as_bytes().to_vec());
    let document = mint_sage_ini::Reader::new(&sources)
        .read(
            "data/ini/formationassistant.ini",
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
    let definition = FormationAssistant::find(document.roots())
        .unwrap()
        .unwrap()
        .definitions
        .remove(0);
    FormationAssistant {
        row_padding: 10.0,
        column_padding: 10.0,
        activation_drag_distance: 0.0,
        activation_time: 0.3,
        definitions: vec![definition],
        templates: vec![FormationTemplate {
            name: "Line".into(),
            rows: vec![vec![vec![0], vec![0]]],
            last_row_of_class: vec![Some(0)],
        }],
        selections: vec![Selection {
            template: 0,
            max_drag_length: f32::MAX,
            max_units_selected: u32::MAX,
        }],
        facing_arrow_templates: [None, None, None],
    }
}

fn selected_pair(
    data: &str,
    names: &[&str],
    assistant: FormationAssistant,
) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (built, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let owned_names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let (kinds, missing) = KindTable::closure(&built, &owned_names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&built);
    let spawns: Vec<Spawn> = [490.0f32, 410.0]
        .iter()
        .map(|y| {
            kinds
                .spawn(
                    &built.combat,
                    kinds.kind("Soldier").unwrap(),
                    Vec2::new(500.0, *y),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(built.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_drag_formation(Arc::new(assistant))
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids)
}

const SOLDIER_WITH_DECAL: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\n  \
  FormationPreviewDecal\n    Texture = PreviewDot\n    Width = 20.0\n    Height = 16.0\n  End\n\
End\n";

#[test]
fn a_units_own_preview_decal_draws_at_its_slot_halved() {
    let (simulation, ids) = selected_pair(SOLDIER_WITH_DECAL, &["Soldier"], assistant(None));
    let rules = simulation.rules();
    let items = rules.formation_preview(
        simulation.state(),
        ids,
        PLAYER,
        ANCHOR,
        Vec2::new(100.0, 0.0),
        crate::rules::PreviewDecals::Item,
    );
    assert_eq!(items.len(), 2, "{items:?}");
    for item in &items {
        assert_eq!(item.texture, "PreviewDot");
        assert!((item.half_width - 10.0).abs() < 1e-3, "{item:?}");
        assert!((item.half_height - 8.0).abs() < 1e-3, "{item:?}");
        assert!((item.centre.x - ANCHOR.x).abs() < 1e-3, "{item:?}");
    }
    let dy = (items[0].centre.y - items[1].centre.y).abs();
    assert!(dy > 5.0, "the pair collapsed onto one slot: {items:?}");
}

const SOLDIER_NO_DECAL_AND_HOLDER: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object DecalHolder\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\n  \
  FormationPreviewDecal\n    Texture = FallbackDot\n    Width = 10.0\n    Height = 10.0\n  End\n\
End\n";

#[test]
fn a_unit_with_no_own_decal_falls_back_to_preview_object() {
    let (simulation, ids) = selected_pair(
        SOLDIER_NO_DECAL_AND_HOLDER,
        &["Soldier", "DecalHolder"],
        assistant(Some("DecalHolder")),
    );
    let rules = simulation.rules();
    let items = rules.formation_preview(
        simulation.state(),
        ids,
        PLAYER,
        ANCHOR,
        Vec2::new(100.0, 0.0),
        crate::rules::PreviewDecals::Item,
    );
    assert_eq!(items.len(), 2, "{items:?}");
    for item in &items {
        assert_eq!(item.texture, "FallbackDot");
    }
}
