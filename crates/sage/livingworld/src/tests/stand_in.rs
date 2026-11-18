use mint_sage_ini::living_world::LivingWorldData;

use crate::stand_in::catalog_from_object_ini;

const OBJECTS: &str = "
Object Base
  BuildCost = 700
  CommandPoints = 50
  AutoResolveUnitType = AutoResolveUnit_Hero
  KindOf = HERO PRELOAD SELECTABLE
End
ChildObject Kid Base
  CommandPoints = 20
End
ChildObject Grandkid Kid
End
ChildObject Plain Base
  KindOf = SELECTABLE
End
ChildObject Dropped Base
  KindOf = -HERO
End
";

#[test]
fn a_child_object_takes_what_it_does_not_declare_from_its_parents() {
    let catalog = catalog_from_object_ini([OBJECTS], &LivingWorldData::default());
    let kid = catalog.units["kid"];
    assert!(kid.hero);
    assert_eq!((kid.command_points, kid.build_cost), (20, 700));
    assert_eq!(kid.unit_type, Some(crate::rules::RESOLVE_TYPE_HERO));
    let grandkid = catalog.units["grandkid"];
    assert!(grandkid.hero);
    assert_eq!(grandkid.command_points, 20);
}

#[test]
fn a_cost_or_kind_written_as_a_define_name_reads_its_value() {
    let catalog = catalog_from_object_ini(
        [
            "#define PRICE 900\n#define HERO_KINDS HERO SELECTABLE\n",
            "Object Named\n  BuildCost = PRICE\n  CommandPoints = PRICE\n  KindOf = HERO_KINDS\nEnd\n",
        ],
        &LivingWorldData::default(),
    );
    let named = catalog.units["named"];
    assert_eq!((named.build_cost, named.command_points), (900, 900));
    assert!(named.hero);
}

#[test]
fn a_child_object_can_replace_or_remove_the_inherited_kind() {
    let catalog = catalog_from_object_ini([OBJECTS], &LivingWorldData::default());
    assert!(!catalog.units["plain"].hero);
    assert!(!catalog.units["dropped"].hero);
    assert!(catalog.units["base"].hero);
}
