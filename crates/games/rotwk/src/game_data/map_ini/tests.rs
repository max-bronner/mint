use mint_sage_ini::{BlockTable, Document};

use super::compose_document;
use crate::game_data::{game_data_from_documents, GameData};

const GLOBAL: &str = "\
Object Porter
  CommandSet = PorterCommandSet
  BuildTime = 20
  VisionRange = 150
  Behavior = LifetimeUpdate ModuleTag_Life
    MinLifetime = 1000
    MaxLifetime = 1000
  End
  Behavior = DestroyDie ModuleTag_Die
  End
End
ChildObject PorterChild Porter
  BuildCost = 50
End
Science SCIENCE_Root
  SciencePurchasePointCost = 1
End
Science SCIENCE_Leaf
  PrerequisiteSciences = SCIENCE_Root
  SciencePurchasePointCost = 5
End
";

fn global() -> GameData {
    let table = BlockTable::corpus_path();
    let document = Document::parse("global.ini", GLOBAL, &table).unwrap();
    game_data_from_documents(&[document], 0, std::time::Instant::now()).0
}

fn composed(map_ini: &str) -> Result<GameData, Vec<String>> {
    let mut data = global();
    let document = Document::parse("map.ini", map_ini, &BlockTable::corpus_path()).unwrap();
    compose_document(&mut data, document).map_err(|refusal| refusal.errors)?;
    Ok(data)
}

#[test]
fn an_object_override_replaces_the_fields_it_writes_and_keeps_the_rest() {
    let data = composed("Object Porter\n  CommandSet = TutorialCommandSet\n  BuildTime = 5\nEnd\n")
        .unwrap();
    let porter = data.templates.get("Porter").unwrap();
    assert_eq!(
        (
            porter.command_set.as_deref(),
            porter.build_time,
            porter.vision_range
        ),
        (Some("TutorialCommandSet"), Some(5.0), Some(150.0))
    );
}

#[test]
fn a_map_child_object_derives_a_new_template() {
    let data = composed("ChildObject MapPorter Porter\n  BuildCost = 75\nEnd\n").unwrap();
    let derived = data.templates.get("MapPorter").unwrap();
    assert_eq!(
        (derived.build_cost, derived.build_time),
        (Some(75.0), Some(20.0))
    );
}

#[test]
fn add_and_remove_module_edit_the_overridden_templates_modules() {
    let data = composed(
        "Object Porter\n  RemoveModule = ModuleTag_Die\n  AddModule\n    Behavior = DestroyDie \
         ModuleTag_NewDie\n    End\n  End\nEnd\n",
    )
    .unwrap();
    let tags: Vec<&str> = data
        .templates
        .get("Porter")
        .unwrap()
        .declared_modules
        .iter()
        .filter_map(|module| module.tag.as_deref())
        .collect();
    assert_eq!(tags, ["ModuleTag_Life", "ModuleTag_NewDie"]);
}

#[test]
fn a_science_override_clears_its_prerequisites_and_keeps_its_cost() {
    let data = composed("Science SCIENCE_Leaf\n  PrerequisiteSciences = None\nEnd\n").unwrap();
    let leaf = data.sciences.get("SCIENCE_Leaf").unwrap();
    assert_eq!((leaf.prerequisites.len(), leaf.cost), (0, Some(5)));
}

#[test]
fn a_map_ini_retail_throws_on_is_refused() {
    for (map_ini, why) in [
        ("Object Porter\n  NotAField = 1\nEnd\n", "an unknown field"),
        ("#define PORTER_TIME 5\n", "a #define"),
        (
            "Object Porter\n  Behavior = DestroyDie ModuleTag_Plain\n  End\nEnd\n",
            "a module outside AddModule",
        ),
        (
            "Object Porter\n  RemoveModule = ModuleTag_Missing\nEnd\n",
            "a RemoveModule naming no tag",
        ),
        (
            "Object Porter\n  ReplaceModule ModuleTag_Die\n    Behavior = LifetimeUpdate \
             ModuleTag_NewLife\n    End\n  End\nEnd\n",
            "a ReplaceModule naming another module type",
        ),
    ] {
        assert!(composed(map_ini).is_err(), "{why} must refuse the load");
    }
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS2 (BFME II) and MINT_CORPUS (ROTWK)"]
fn every_shipped_map_ini_composes_over_the_global_data() {
    let Some(rotwk) = std::env::var_os("MINT_CORPUS").map(std::path::PathBuf::from) else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };
    let bfme2 = std::env::var_os("MINT_CORPUS2").map(std::path::PathBuf::from);
    let order: Vec<&std::path::Path> = bfme2
        .iter()
        .map(std::path::PathBuf::as_path)
        .chain(std::iter::once(rotwk.as_path()))
        .collect();
    let vfs = crate::mount(order);
    let Ok((data, _)) = crate::game_data::load_game_data(&vfs) else {
        panic!("the global data refused to load");
    };
    let maps: std::collections::BTreeSet<String> = vfs
        .mounts()
        .flat_map(|mount| mount.entries())
        .map(|path| path.as_str().to_ascii_lowercase())
        .filter(|path| path.ends_with("/map.ini"))
        .collect();
    let mut refused: Vec<String> = Vec::new();
    for path in &maps {
        let mut copy = data.clone();
        match super::compose_map_ini(&mut copy, &vfs, path) {
            Ok(line) => println!("{}", line.unwrap_or_default()),
            Err(refusal) => refused.extend(refusal.errors),
        }
    }
    println!(
        "{} map.ini file(s), {} refusal(s)",
        maps.len(),
        refused.len()
    );
    assert!(refused.is_empty(), "{refused:#?}");
}
