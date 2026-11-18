#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use mint_sage_ini::{BlockTable, Document};

fn tree_from(text: &str) -> FxTree {
    tree_of(text, "")
}

fn tree_of(effects: &str, objects: &str) -> FxTree {
    let document = Document::parse("effects.ini", effects, &BlockTable::particle_path()).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);

    let document = Document::parse("objects.ini", objects, &BlockTable::corpus_path()).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut templates = ObjectIndex::new();
    templates.insert_document(&document);
    let mut names = NameFieldIndex::with_siblings(&[
        crate::tree::WHICH_SPECIAL_WEAPON,
        crate::tree::UNPACK_TIME,
    ]);
    names.insert_document(&document);

    FxTree::build(Sources {
        index: &index,
        lists: &lists,
        objects: &templates,
        names: &names,
    })
}

fn object_root(tree: &FxTree, name: &str) -> usize {
    tree.roots(Category::Objects, name)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("{name} is an object root"))
}

fn lines_under(tree: &FxTree, index: usize) -> Vec<String> {
    tree.node(index)
        .unwrap()
        .children
        .iter()
        .filter_map(|child| tree.node(*child).map(Node::line))
        .collect()
}

fn system(name: &str, system_fields: &str, modules: &str) -> String {
    format!(
        "FXParticleSystem {name}\n  System\n    Lifetime = 10 10\n{system_fields}  End\n{modules}End\n"
    )
}

fn list_root(tree: &FxTree, name: &str) -> usize {
    tree.find_root(name)
        .unwrap_or_else(|| panic!("{name} is a root"))
}

fn kinds_under(tree: &FxTree, index: usize) -> Vec<(Role, NodeKind, String)> {
    tree.node(index)
        .unwrap()
        .children
        .iter()
        .map(|child| {
            let node = tree.node(*child).unwrap();
            (node.role, node.kind, node.name.clone())
        })
        .collect()
}

#[test]
fn a_list_composes_nuggets_then_system_then_slave_per_particle_and_collision() {
    let tree = tree_from(&format!(
        "{}{}{}{}{}",
        system(
            "Core",
            "    SlaveSystem = Trail\n    PerParticleAttachedSystem = Spark\n",
            "  Event = TerrainCollision\n    EventFX = FX_Hit\n  End\n"
        ),
        system("Trail", "", ""),
        system("Spark", "", ""),
        "FXList FX_Hit\n  ParticleSystem\n    Name = Spark\n  End\nEnd\n",
        "FXList FX_Blast\n\
         \x20 Sound\n    Name = BlastSound\n  End\n\
         \x20 ParticleSystem\n    Name = Core\n    OrientToObject = Yes\n  End\n\
         End\n",
    ));

    let root = list_root(&tree, "FX_Blast");
    let nuggets = kinds_under(&tree, root);
    assert_eq!(
        nuggets,
        vec![
            (Role::Nugget, NodeKind::OtherNugget, "Sound".to_owned()),
            (
                Role::Nugget,
                NodeKind::ParticleNugget,
                "ParticleSystem".to_owned()
            ),
        ],
        "nuggets in file order, the Sound one inert"
    );

    let particle_nugget = tree.node(root).unwrap().children[1];
    let core = kinds_under(&tree, particle_nugget);
    assert_eq!(
        core,
        vec![(Role::NuggetSystem, NodeKind::System, "Core".to_owned())]
    );

    let core = tree.node(particle_nugget).unwrap().children[0];
    assert_eq!(
        kinds_under(&tree, core),
        vec![
            (Role::Slave, NodeKind::System, "Trail".to_owned()),
            (Role::PerParticle, NodeKind::System, "Spark".to_owned()),
            (Role::Collision, NodeKind::FxList, "FX_Hit".to_owned()),
        ]
    );
}

#[test]
fn a_nugget_naming_no_system_is_shown_as_dangling() {
    let tree = tree_from("FXList FX_Empty\n  ParticleSystem\n    Name = Missing\n  End\nEnd\n");
    let root = list_root(&tree, "FX_Empty");
    let nugget = tree.node(root).unwrap().children[0];
    assert_eq!(
        kinds_under(&tree, nugget),
        vec![(Role::NuggetSystem, NodeKind::Dangling, "Missing".to_owned())]
    );
}

#[test]
fn a_cycle_of_slave_systems_stops_at_the_repeat() {
    let tree = tree_from(&format!(
        "{}{}",
        system("A", "    SlaveSystem = B\n", ""),
        system("B", "    SlaveSystem = A\n", ""),
    ));
    let a = list_root(&tree, "A");
    let b = tree.node(a).unwrap().children[0];
    assert_eq!(tree.node(b).unwrap().kind, NodeKind::System);
    let stopped = tree.node(b).unwrap().children[0];
    assert_eq!(tree.node(stopped).unwrap().kind, NodeKind::Stopped);
    assert!(tree.node(stopped).unwrap().children.is_empty());
}

#[test]
fn the_filter_narrows_roots_case_insensitively() {
    let tree = tree_from(&format!(
        "{}{}",
        system("Ember", "", ""),
        system("Smoke", "", "")
    ));
    let header = tree.category(Category::Systems).unwrap();
    let mut expanded = BTreeSet::new();
    expanded.insert(header);
    for bucket in tree.node(header).unwrap().children.clone() {
        expanded.insert(bucket);
    }

    let names = |filter: &str| -> Vec<String> {
        tree.rows(filter, &expanded)
            .into_iter()
            .filter(|row| tree.node(*row).map(|node| node.kind) == Some(NodeKind::System))
            .filter_map(|row| tree.node(row).map(|node| node.name.clone()))
            .collect()
    };
    assert_eq!(names(""), vec!["Ember".to_owned(), "Smoke".to_owned()]);
    assert_eq!(names("EMB"), vec!["Ember".to_owned()]);
    assert!(names("nothing").is_empty());
    assert_eq!(
        tree.rows("nothing", &expanded).len(),
        Category::ALL.len(),
        "only the two category headers survive"
    );
}

#[test]
fn a_subtree_reports_every_system_below_it_once() {
    let tree = tree_from(&format!(
        "{}{}{}",
        system("Core", "    SlaveSystem = Trail\n", ""),
        system("Trail", "", ""),
        "FXList FX_Twice\n\
         \x20 ParticleSystem\n    Name = Core\n  End\n\
         \x20 ParticleSystem\n    Name = Trail\n  End\n\
         End\n",
    ));
    let root = list_root(&tree, "FX_Twice");
    let systems = tree.subtree_systems(root);
    assert_eq!(
        systems.into_iter().collect::<Vec<_>>(),
        vec!["core".to_owned(), "trail".to_owned()]
    );
}

fn object(header: &str, states: &str, modules: &str) -> String {
    format!(
        "{header}\n\
         \x20 Draw = W3DScriptedModelDraw ModuleTag_Draw\n\
         {states}\
         \x20 End\n\
         {modules}\
         End\n"
    )
}

#[test]
fn a_field_of_an_invented_module_gets_a_row_when_its_value_names_an_effect() {
    let tree = tree_of(
        &system("Sparks", "", ""),
        &object(
            "Object TestThing",
            "",
            "  Behavior = MadeUpModule ModuleTag_Invented\n\
             \x20   InventedField = Sparks\n\
             \x20   NotAnEffect = SomethingElse\n\
             \x20 End\n",
        ),
    );
    let root = object_root(&tree, "TestThing");
    let rows: Vec<String> = lines_under(&tree, root)
        .into_iter()
        .map(|line| line.split("  ").take(2).collect::<Vec<_>>().join("  "))
        .collect();
    assert_eq!(
        rows,
        vec!["field: Sparks  ModuleTag_Invented.InventedField".to_owned()]
    );
}

#[test]
fn a_module_naming_a_weapon_reaches_its_effects_with_the_modules_pose_and_delay() {
    let tree = tree_of(
        &system("Sparks", "", ""),
        &format!(
            "{}Weapon TestBlast
  FireFX = Sparks
End
Weapon Silent
  Damage = 5
End
",
            object(
                "Object TestCaster",
                "",
                "  Behavior = MadeUpAbility ModuleTag_Cast
                     WhichSpecialWeapon = 2
                     UnpackTime = 900
                     SpecialWeapon = TestBlast
                     OtherWeapon = Silent
                   End
",
            )
        ),
    );
    let root = object_root(&tree, "TestCaster");
    let children = tree.node(root).unwrap().children.clone();
    assert_eq!(children.len(), 1, "a weapon with no effect gets no row");
    let weapon = tree.node(children[0]).unwrap();
    assert_eq!(weapon.kind, NodeKind::Weapon);
    assert_eq!(weapon.conditions, vec!["SPECIAL_WEAPON_TWO".to_owned()]);
    assert_eq!(weapon.fire_after, Some(0.9));
    assert_eq!(tree.node(weapon.children[0]).unwrap().name, "Sparks");
}

#[test]
fn a_child_object_inherits_the_effect_fields_it_does_not_rewrite() {
    let tree = tree_of(
        &format!("{}{}", system("Inherited", "", ""), system("Own", "", "")),
        &format!(
            "{}{}",
            object(
                "Object TestParent",
                "",
                "  Behavior = MadeUpModule ModuleTag_A\n\
                 \x20   Kept = Inherited\n\
                 \x20   Replaced = Inherited\n\
                 \x20 End\n",
            ),
            object(
                "ChildObject TestChild TestParent",
                "",
                "  Behavior = MadeUpModule ModuleTag_A\n\
                 \x20   Replaced = Own\n\
                 \x20 End\n",
            ),
        ),
    );
    let root = object_root(&tree, "TestChild");
    let rows: Vec<String> = lines_under(&tree, root)
        .into_iter()
        .map(|line| line.split("  ").take(2).collect::<Vec<_>>().join("  "))
        .collect();
    assert_eq!(
        rows,
        vec![
            "field: Own  ModuleTag_A.Replaced".to_owned(),
            "field: Inherited  ModuleTag_A.Kept".to_owned()
        ]
    );
}

#[test]
fn the_three_typed_rows_hang_off_the_state_that_declared_them() {
    let tree = tree_of(
        &format!(
            "{}FXList FX_Enter\nEnd\nFXList FX_Event\nEnd\n",
            system("Smoke", "", "")
        ),
        &object(
            "Object TestThing",
            "    DefaultModelConditionState\n\
             \x20     Model = TESTMDL\n\
             \x20     ParticleSysBone = B_SMOKE Smoke FollowBone:Yes\n\
             \x20   End\n\
             \x20   AnimationState = MOVING\n\
             \x20     Animation = Walk\n\
             \x20       AnimationName = TESTMDL_WLKA\n\
             \x20     End\n\
             \x20     EnteringStateFX = FX_Enter\n\
             \x20     FXEvent = Frame:7 Name:FX_Event FrameStop:9\n\
             \x20   End\n",
            "",
        ),
    );
    let root = object_root(&tree, "TestThing");
    let states = tree.node(root).unwrap().children.clone();
    let first: Vec<String> = lines_under(&tree, states[0])
        .into_iter()
        .map(|line| line.split("  ").take(2).collect::<Vec<_>>().join("  "))
        .collect();
    assert_eq!(
        first,
        vec!["sysbone: Smoke  bone B_SMOKE, FollowBone".to_owned()]
    );
    let second: Vec<String> = lines_under(&tree, states[1])
        .into_iter()
        .map(|line| line.split("  ").take(2).collect::<Vec<_>>().join("  "))
        .collect();
    assert_eq!(
        second,
        vec![
            "entering: FX_Enter  0 nuggets".to_owned(),
            "fx event: FX_Event  Frame:7 FrameStop:9".to_owned()
        ]
    );
}

#[test]
fn a_particle_sys_bone_naming_no_system_is_shown_as_dangling() {
    let tree = tree_of(
        "",
        &object(
            "Object TestThing",
            "    DefaultModelConditionState\n\
             \x20     Model = TESTMDL\n\
             \x20     ParticleSysBone = B_SMOKE Missing\n\
             \x20   End\n",
            "",
        ),
    );
    let root = object_root(&tree, "TestThing");
    let state = tree.node(root).unwrap().children[0];
    let bone = tree.node(state).unwrap().children[0];
    assert_eq!(tree.node(bone).unwrap().kind, NodeKind::Dangling);
}
