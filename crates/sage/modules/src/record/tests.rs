use super::*;
use mint_sage_ini::{BlockTable, Document};

fn parse(text: &str) -> Document {
    Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap()
}

pub(super) fn record(text: &str, name: &str) -> Option<ObjectRecord> {
    read(text, name).0
}

fn read(text: &str, name: &str) -> (Option<ObjectRecord>, Vec<mint_sage_ini::ValueError>) {
    let document = parse(text);
    let mut index = TemplateIndex::new();
    index.insert_document(&document);
    let errors = mint_sage_ini::ValueErrors::new();
    let record = ObjectRecord::read(&index, name, &errors);
    (record, errors.take())
}

const FULL: &str = "\
#define TEST_HEALTH 3000
#define TEST_COST 300
Object Keep
  Side = Men
  KindOf = STRUCTURE SELECTABLE IMMOBILE DOZER_FACTORY
  VisionRange = 160.0
  ShroudClearingRange = 400
  CrusherLevel = 2
  CrushableLevel = 3
  CommandSet = KeepCommandSet
  BuildCost = TEST_COST
  BuildTime = 20.0
  CommandPoints = 60
  PlacementViewAngle = -45
  SelectPortrait = KeepPortrait
  DisplayName = OBJECT:Keep
  Description = OBJECT:KeepDescription
  WeaponSet
    Conditions = None
    Weapon = PRIMARY KeepBow
    Weapon = SECONDARY KeepFire
  End
  WeaponSet
    Conditions = PLAYER_UPGRADE
    Weapon = PRIMARY KeepFlamingBow
  End
  ArmorSet
    Conditions = None
    Armor = KeepArmor
  End
  Body = StructureBody ModuleTag_01
    MaxHealth = TEST_HEALTH
  End
  Behavior = AutoDepositUpdate ModuleTag_02
    DepositTiming = 6000
    DepositAmount = 25
  End
  Behavior = ProductionUpdate ModuleTag_03
    MaxQueueEntries = 3
  End
  Behavior = QueueProductionExitUpdate ModuleTag_04
    UnitCreatePoint = X:0.0 Y:0.0 Z:0.0
    NaturalRallyPoint = X:80.0 Y:-5.0 Z:0.0
    NoExitPath = Yes
    InitialBurst = 2
    UseReturnToFormation = No
    CanRallyToSlaughter = Yes
  End
  Behavior = CastleBehavior ModuleTag_05
    CastleToUnpackForFaction = Men Fortress_Men
    CastleToUnpackForFaction = Mordor Fortress_Mordor 500
    FilterValidOwnedEntries = ANY +STRUCTURE +BASE_FOUNDATION
    InstantUnpack = Yes
  End
  Geometry = BOX
  GeometryMajorRadius = 33.0
  GeometryMinorRadius = 40.0
  GeometryHeight = 30.0
End
";

#[test]
fn every_field_a_template_declares_comes_off_its_own_block() {
    let keep = record(FULL, "keep").unwrap();
    assert_eq!(keep.name, "Keep");
    assert_eq!(keep.depth, 0);
    assert_eq!(keep.side.as_deref(), Some("Men"));
    assert!(keep.has_kind("structure") && keep.has_kind("DOZER_FACTORY"));
    assert!(!keep.dozer, "DOZER_FACTORY is not DOZER");
    assert_eq!(keep.vision_range, Some(160.0));
    assert_eq!(keep.shroud_clearing_range, Some(400.0));
    assert_eq!(keep.crush.crusher_level, 2);
    assert_eq!(keep.crush.crushable_level, 3);
    assert_eq!(keep.command_set.as_deref(), Some("KeepCommandSet"));
    assert_eq!(keep.build_cost, Some(300.0), "through the macro table");
    assert_eq!(keep.build_time, Some(20.0));
    assert_eq!(keep.command_points, Some(60.0));
    assert_eq!(keep.placement_view_angle, Some(-45.0));
    assert_eq!(keep.select_portrait.as_deref(), Some("KeepPortrait"));
    assert_eq!(keep.display_name.as_deref(), Some("OBJECT:Keep"));
    assert_eq!(keep.description.as_deref(), Some("OBJECT:KeepDescription"));
    assert_eq!(keep.max_health, Some(3000.0));
    assert_eq!(keep.income, Some((6000.0, 25.0)));
    assert!(keep.produces);
    assert_eq!(keep.max_queue_entries, Some(3.0));
    assert_eq!(keep.natural_rally_point, Some([80.0, -5.0]));
    assert!(keep.no_exit_path);
    assert_eq!(keep.production_initial_burst, Some(2.0));
    assert!(!keep.use_return_to_formation);
    assert!(keep.can_rally_to_slaughter);
    assert_eq!(keep.geometry.shape.as_deref(), Some("BOX"));
    assert_eq!(keep.geometry.radius, Some(40.0));
    assert_eq!(keep.geometry.major_radius, Some(33.0));
    assert_eq!(keep.geometry.height, Some(30.0));
    let castle = keep.castle.unwrap();
    assert_eq!(castle.for_side("MORDOR"), Some("Fortress_Mordor"));
    assert_eq!(castle.for_side("Elves"), None);
    assert!(castle.instant_unpack);
}

#[test]
fn the_default_weapon_and_armour_are_the_unconditional_sets_primary() {
    let keep = record(FULL, "Keep").unwrap();
    assert_eq!(keep.weapon.as_deref(), Some("KeepBow"));
    assert_eq!(keep.armor.as_deref(), Some("KeepArmor"));

    let text = "Object T\n  \
            WeaponSet\n    Conditions = PLAYER_UPGRADE\n    Weapon = PRIMARY Upgraded\n  End\n  \
            WeaponSet\n    Conditions = None\n    Weapon = SECONDARY Only\n  End\n\
            End\n";
    let t = record(text, "T").unwrap();
    assert_eq!(t.weapon, None, "the default set has no PRIMARY slot");

    let tie = "Object T
              WeaponSet
    Conditions = None
    Weapon = PRIMARY First
  End
              WeaponSet
    Conditions = None
    Weapon = PRIMARY Second
  End
            End
";
    assert_eq!(record(tie, "T").unwrap().weapon.as_deref(), Some("First"));
}

#[test]
fn geometry_is_small_is_inherited_like_any_other_plain_field() {
    let text = "Object Shrub\n  KindOf = IMMOBILE\n  \
              Geometry = CYLINDER\n  GeometryMajorRadius = 3.2\n  GeometryIsSmall = Yes\n\
            End\n\
            ChildObject ShrubVariant Shrub\nEnd\n\
            ChildObject NotSmall Shrub\n  GeometryIsSmall = No\nEnd\n";
    assert_eq!(record(text, "Shrub").unwrap().geometry_is_small, Some(true));
    assert_eq!(
        record(text, "ShrubVariant").unwrap().geometry_is_small,
        Some(true),
        "inherited, not restated"
    );
    assert_eq!(
        record(text, "NotSmall").unwrap().geometry_is_small,
        Some(false),
        "restated, overrides the parent"
    );
}

#[test]
fn display_name_and_description_are_none_cleared_plain_fields() {
    let text = "Object Tree\n  KindOf = IMMOBILE\nEnd\n\
            Object Fortress\n  DisplayName = OBJECT:Fortress\n  \
              Description = OBJECT:FortressDescription\nEnd\n\
            Object Cleared\n  DisplayName = NONE\n  Description = NONE\nEnd\n";
    assert_eq!(record(text, "Tree").unwrap().display_name, None);
    assert_eq!(record(text, "Tree").unwrap().description, None);
    assert_eq!(
        record(text, "Fortress").unwrap().display_name.as_deref(),
        Some("OBJECT:Fortress")
    );
    assert_eq!(
        record(text, "Fortress").unwrap().description.as_deref(),
        Some("OBJECT:FortressDescription")
    );
    assert_eq!(record(text, "Cleared").unwrap().display_name, None);
    assert_eq!(record(text, "Cleared").unwrap().description, None);
}

#[test]
fn powers_read_starts_paused_from_the_power_modules_alone() {
    let text = "Object Fort\n  ButtonImage = HIFort\n  \
              Behavior = UnpauseSpecialPowerUpgrade T1\n    SpecialPowerTemplate = Oil\n    TriggeredBy = Upgrade_Oil\n    StartsPaused = Yes\n  End\n  \
              Behavior = SpecialPowerModule T2\n    SpecialPowerTemplate = Oil\n    StartsPaused = Yes\n  End\n  \
              Behavior = WeaponFireSpecialAbilityUpdate T3\n    SpecialPowerTemplate = Oil\n  End\n  \
              Behavior = OCLSpecialPower T5\n    SpecialPowerTemplate = Vision\n    StartsPaused = Yes\n  End\n  \
              Behavior = SpecialPowerModule T6\n    SpecialPowerTemplate = Free\n  End\n  \
              Behavior = UnpauseSpecialPowerUpgrade T7\n    SpecialPowerTemplate = Released\n    StartsPaused = Yes\n  End\n  \
              Geometry = BOX\n  GeometryMajorRadius = 10\n  \
              GeometryContactPoint = X:80 Y:0 Z:60 Menu\n  \
              GeometryContactPoint = X:1 Y:2 Z:3 Repair\nEnd\n";
    let fort = record(text, "Fort").unwrap();
    assert_eq!(fort.button_image.as_deref(), Some("HIFort"));
    assert_eq!(fort.menu_point, Some([80.0, 0.0, 60.0]));
    assert!(fort.power("oil").unwrap().starts_paused);
    assert!(fort.power("Vision").unwrap().starts_paused);
    assert!(!fort.power("Free").unwrap().starts_paused);
    assert!(
        !fort.power("Released").unwrap().starts_paused,
        "a releasing module's own field does not pause its power"
    );
    assert!(fort.power("Missing").is_none());
    assert_eq!(fort.powers.len(), 4);
}

#[test]
fn powers_only_capture_a_deferring_modules_own_modifier() {
    let text = "Object Buffer\n  \
              Behavior = SpecialPowerModule T1\n    SpecialPowerTemplate = Buff\n    \
                AttributeModifier = BuffMod\n  End\n  \
              Behavior = SpecialPowerModule T2\n    SpecialPowerTemplate = Buff\n    \
                UpdateModuleStartsAttack = Yes\n  End\nEnd\n";
    let buffer = record(text, "Buffer").unwrap();
    let slot = buffer.power("Buff").unwrap();
    assert!(slot.defers_recharge);
    assert!(
        slot.modifier.is_none(),
        "the non-deferring module's modifier must not leak into the preparation-start slot"
    );
    assert!(slot.trigger_fx.is_none());
}

#[test]
fn powers_read_initiate_sound_from_the_first_module_declaring_it() {
    let text = "Object Hero\n  \
              Behavior = SpecialPowerModule T1\n    SpecialPowerTemplate = Blast\n    InitiateSound = VoiceBlastOne\n  End\n  \
              Behavior = WeaponFireSpecialAbilityUpdate T2\n    SpecialPowerTemplate = Blast\n    InitiateSound = VoiceBlastTwo\n  End\n  \
              Behavior = SpecialPowerModule T3\n    SpecialPowerTemplate = Quiet\n  End\nEnd\n";
    let hero = record(text, "Hero").unwrap();
    assert_eq!(
        hero.power("Blast").unwrap().initiate_sound.as_deref(),
        Some("VoiceBlastOne")
    );
    assert_eq!(hero.power("Quiet").unwrap().initiate_sound, None);
}

#[test]
fn initial_crew_names_a_created_template() {
    let text = "Object Engine
                Behavior = SiegeEngineContain Tag1
                    InitialCrew = Crewman 6
  End
            End
";
    let engine = record(text, "Engine").unwrap();
    assert_eq!(engine.created_templates(), ["Crewman"]);
}

#[test]
fn a_replace_object_block_names_its_replacements_as_created_templates() {
    let text = "Object Caster\n  \
              Behavior = ReplaceObjectUpdate Tag1\n    \
                ReplaceObject\n      TargetObjectFilter = NONE +INFANTRY ENEMIES\n      \
                ReplacementObjectName = ModAuthorHen ModAuthorGoat\n    End\n  \
              End\nEnd\n";
    let caster = record(text, "Caster").unwrap();
    assert_eq!(
        caster.created_templates(),
        ["ModAuthorHen", "ModAuthorGoat"]
    );
}

#[test]
fn upgraded_command_sets_are_every_target_once() {
    let text = "Object Barracks\n  \
              Behavior = CommandSetUpgrade Tag2\n    \
                TriggeredBy = Upgrade_Level2\n    \
                ConflictsWith = Upgrade_Level3\n    \
                CommandSet = BarracksCommandSetLevel2\n  End\n  \
              Behavior = CommandSetUpgrade Tag3\n    \
                TriggeredBy = Upgrade_Level3\n    \
                CommandSet = BarracksCommandSetLevel3\n    \
                CommandSetAlt = BarracksCommandSetLevel3Elf\n  End\n  \
              Behavior = CommandSetUpgrade Again\n    \
                TriggeredBy = Upgrade_Other\n    \
                CommandSet = barrackscommandsetlevel2\n  End\n  \
              Behavior = CommandSetUpgrade Nothing\n    \
                TriggeredBy = X\n    CommandSet = NONE\n  End\n\
            End\n";
    let barracks = record(text, "Barracks").unwrap();
    assert_eq!(
        barracks.upgraded_command_sets,
        [
            "BarracksCommandSetLevel2",
            "BarracksCommandSetLevel3",
            "BarracksCommandSetLevel3Elf"
        ]
    );
    assert_eq!(barracks.declared_modules.len(), 4);
    assert_eq!(barracks.declared_modules[1].tag.as_deref(), Some("Tag3"));
}

#[test]
fn weapon_and_armour_sets_are_read_in_declaration_order() {
    let keep = record(FULL, "Keep").unwrap();
    let selections: Vec<(&[String], Option<&str>)> = keep
        .weapon_sets
        .iter()
        .map(|set| (set.conditions.as_slice(), set.value.as_deref()))
        .collect();
    assert_eq!(
        selections,
        [
            (&[][..], Some("KeepBow")),
            (&["PLAYER_UPGRADE".to_owned()][..], Some("KeepFlamingBow")),
        ]
    );
    assert_eq!(keep.armor_sets.len(), 1);
    assert_eq!(keep.armor_sets[0].value.as_deref(), Some("KeepArmor"));

    let text = "Object Parent\n  \
              ArmorSet\n    Conditions = None\n    Armor = Light\n  End\n  \
              ArmorSet\n    Conditions = player_upgrade\n    Armor = Heavy\n  End\n\
            End\n\
            ChildObject Child Parent\n  \
              ArmorSet\n    Conditions = None\n    Armor = Cloth\n  End\n\
            End\n\
            ChildObject Heir Parent\nEnd\n";
    let parent = record(text, "Parent").unwrap();
    assert_eq!(parent.armor_sets[1].conditions, ["PLAYER_UPGRADE"]);
    assert_eq!(record(text, "Child").unwrap().armor_sets.len(), 1);
    assert_eq!(record(text, "Heir").unwrap().armor_sets.len(), 2);
}

#[test]
fn locomotor_sets_read_condition_locomotor_and_speed() {
    let text = "#define MEMBER_SPEED 45\n\
            Object Soldier\n  \
              LocomotorSet\n    Locomotor = HumanLocomotor\n    Condition = SET_NORMAL\n    Speed = MEMBER_SPEED\n  End\n  \
              LocomotorSet\n    Locomotor = HumanWanderLocomotor\n    Condition = set_wander\n    Speed = 17\n  End\n\
            End\n\
            ChildObject Recruit Soldier\n  \
              LocomotorSet\n    Locomotor = HumanLocomotor\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n\
            End\n\
            ChildObject Veteran Soldier\nEnd\n";
    let soldier = record(text, "Soldier").unwrap();
    assert_eq!(
        soldier.locomotor_sets,
        [
            LocomotorSlot {
                condition: "SET_NORMAL".to_owned(),
                locomotor: Some("HumanLocomotor".to_owned()),
                speed: Some(45.0),
            },
            LocomotorSlot {
                condition: "SET_WANDER".to_owned(),
                locomotor: Some("HumanWanderLocomotor".to_owned()),
                speed: Some(17.0),
            },
        ]
    );
    assert_eq!(
        soldier.locomotor_for("set_normal").and_then(|s| s.speed),
        Some(45.0),
        "the lookup is case-insensitive"
    );
    assert_eq!(
        soldier.locomotor_for("SET_PANIC").and_then(|s| s.speed),
        Some(45.0),
        "an undeclared condition falls back to SET_NORMAL"
    );
    assert_eq!(record(text, "Recruit").unwrap().locomotor_sets.len(), 1);
    assert_eq!(record(text, "Veteran").unwrap().locomotor_sets.len(), 2);
}

#[test]
fn the_bodys_damage_thresholds_are_read_beside_its_health() {
    let text = "#define HURT 2000\nObject Keep\n  Body = StructureBody Tag\n    MaxHealth = 3000\n    MaxHealthDamaged = HURT\n    MaxHealthReallyDamaged = 1000\n  End\nEnd\nObject Hut\n  Body = ActiveBody Tag\n    MaxHealth = 10\n  End\nEnd\n";
    let keep = record(text, "Keep").unwrap();
    assert_eq!(keep.max_health, Some(3000.0));
    assert_eq!(keep.max_health_damaged, Some(2000.0));
    assert_eq!(keep.max_health_really_damaged, Some(1000.0));
    let hut = record(text, "Hut").unwrap();
    assert_eq!(hut.max_health_damaged, None);
}

#[test]
fn the_bodys_dodge_percent_is_read_as_a_fraction_in_every_written_shape() {
    let text = "#define HERO_DODGE 50\n\
        Object Bare\n  Body = ActiveBody Tag\n    MaxHealth = 10\n    DodgePercent = 33\n  End\nEnd\n\
        Object Signed\n  Body = ActiveBody Tag\n    MaxHealth = 10\n    DodgePercent = 80%\n  End\nEnd\n\
        Object Macro\n  Body = ActiveBody Tag\n    MaxHealth = 10\n    DodgePercent = HERO_DODGE\n  End\nEnd\n\
        Object Absent\n  Body = ActiveBody Tag\n    MaxHealth = 10\n  End\nEnd\n";
    assert_eq!(record(text, "Bare").unwrap().dodge_percent, Some(0.33));
    assert_eq!(record(text, "Signed").unwrap().dodge_percent, Some(0.8));
    assert_eq!(record(text, "Macro").unwrap().dodge_percent, Some(0.5));
    assert_eq!(record(text, "Absent").unwrap().dodge_percent, None);
}

#[test]
fn a_dodge_percent_that_is_not_a_percentage_is_refused() {
    let text =
        "Object Bad\n  Body = ActiveBody Tag\n    MaxHealth = 10\n    DodgePercent = purple\n  End\nEnd\n";
    let (record, errors) = read(text, "Bad");
    assert_eq!(record.unwrap().dodge_percent, None);
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_malformed_build_completion_flank_delay_pre_built_or_damage_creation_line_is_refused() {
    let cases = [
        "Object Bad
  BuildCompletion = SOMETIMES
End
",
        "Object Bad
  Behavior = HordeContain Tag
    FlankedDelay = soon
  End
End
",
        "Object Bad
  Behavior = HordeContain Tag
    FrontAngle = wide
  End
End
",
        "Object Bad
  Behavior = CastleBehavior Tag
    PreBuiltList = ModFarm many
  End
End
",
        "Object Bad
  Behavior = CastleBehavior Tag
    PreBuiltList = ModFarm
  End
End
",
        "Object Bad
  Body = StructureBody Tag
    MaxHealth = 10
    DamageCreationList = ModChunks FLAME
  End
End
",
        "Object Bad
  Body = StructureBody Tag
    MaxHealth = 10
    DamageCreationList = ModChunks CATAPULT_ROCK DAMAGED
  End
End
",
        "Object Bad
  Body = ActiveBody Tag
    MaxHealth = 10
    DamageCreationList = ModChunks
  End
End
",
    ];
    for text in cases {
        let (record, errors) = read(text, "Bad");
        assert!(record.is_some(), "{text}");
        assert_eq!(errors.len(), 1, "{text}: {errors:?}");
    }
}

#[test]
fn the_health_box_fields_are_read_and_an_excluded_record_does_not_raise_the_bar() {
    let text = "Object Keep\n  \
              Geometry = CYLINDER\n  GeometryMajorRadius = 50\n  GeometryHeight = 60\n  \
              AdditionalGeometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 225\n  \
              GeometryUsedForHealthBox = No\n  \
              HealthBoxHeightOffset = -20\n  HealthBoxScale = 2.5\n\
            End\n";
    let keep = record(text, "Keep").unwrap();
    assert_eq!(
        keep.geometry.height,
        Some(225.0),
        "the fold still sees the spire"
    );
    assert_eq!(keep.geometry.health_box_height, Some(60.0));
    assert_eq!(keep.health_box_offset, Some(-20.0));
    assert_eq!(keep.health_box_scale, Some(2.5));
    let plain =
        "Object Hut\n  Geometry = BOX\n  GeometryMajorRadius = 5\n  GeometryHeight = 12\nEnd\n";
    let hut = record(plain, "Hut").unwrap();
    assert_eq!(hut.geometry.health_box_height, Some(12.0));
    assert_eq!(hut.health_box_offset, None);
}

#[test]
fn a_block_naming_only_additional_geometry_still_resolves() {
    let text = "Object Plot\n  \
              AdditionalGeometry = CYLINDER\n  GeometryName = Orig\n  \
              GeometryMajorRadius = 55\n  GeometryHeight = 40\n  \
              AdditionalGeometry = CYLINDER\n  GeometryName = V2\n  \
              GeometryMajorRadius = 16\n  GeometryHeight = 76\n\
            End\n";
    let plot = record(text, "Plot").unwrap();
    assert_eq!(plot.geometry.pieces.len(), 2);
    assert_eq!(plot.geometry.shown_radius(&[]), Some(55.0));
}

#[test]
fn a_child_inherits_what_it_does_not_restate_and_overrides_what_it_does() {
    let text = format!(
            "{FULL}ChildObject Reskin Keep\n  BuildCost = 99\n  Body = ActiveBody Tag_09\n    MaxHealth = 5\n  End\nEnd\n"
        );
    let child = record(&text, "reskin").unwrap();
    assert_eq!(child.depth, 1);
    assert_eq!(child.build_cost, Some(99.0));
    assert_eq!(
        child.max_health,
        Some(5.0),
        "the child's body replaces the parent's"
    );
    assert_eq!(child.command_set.as_deref(), Some("KeepCommandSet"));
    assert_eq!(child.weapon.as_deref(), Some("KeepBow"));
    assert_eq!(child.income, Some((6000.0, 25.0)));
    assert_eq!(child.geometry.radius, Some(40.0));
    assert!(child.has_kind("STRUCTURE"));
}

#[test]
fn a_childs_own_weapon_set_replaces_every_inherited_one() {
    let text = format!(
            "{FULL}ChildObject Rearmed Keep\n  WeaponSet\n    Conditions = PLAYER_UPGRADE\n    Weapon = PRIMARY Upgraded\n  End\nEnd\n"
        );
    let child = record(&text, "Rearmed").unwrap();
    assert_eq!(
        child.weapon.as_deref(),
        Some("Upgraded"),
        "the inherited unconditional set must be gone, not preferred"
    );
    assert_eq!(
        child.armor.as_deref(),
        Some("KeepArmor"),
        "armour untouched"
    );
}

#[test]
fn kind_of_replaces_edits_or_clears_as_bitflags_parse_does() {
    let base = "Object Base\n  KindOf = STRUCTURE SELECTABLE\nEnd\n";
    let edited = format!("{base}ChildObject Edited Base\n  KindOf = +DOZER -SELECTABLE\nEnd\n");
    let t = record(&edited, "Edited").unwrap();
    assert_eq!(t.kind_of, ["STRUCTURE", "DOZER"]);
    assert!(t.dozer);
    let replaced = format!("{base}ChildObject Replaced Base\n  KindOf = INFANTRY\nEnd\n");
    assert_eq!(record(&replaced, "Replaced").unwrap().kind_of, ["INFANTRY"]);
    let cleared = format!("{base}ChildObject Cleared Base\n  KindOf = NONE\nEnd\n");
    assert!(record(&cleared, "Cleared").unwrap().kind_of.is_empty());
}

#[test]
fn a_dozer_ai_update_alone_makes_a_dozer() {
    let text = "Object Porter\n  KindOf = INFANTRY\n  Behavior = DozerAIUpdate Tag\n  End\nEnd\n";
    assert!(record(text, "Porter").unwrap().dozer);
}

#[test]
fn hostile_numbers_and_orphan_fields_read_as_absent_rather_than_panicking() {
    let text = "Object Odd\n  \
            GeometryMajorRadius = 99.0\n  \
            BuildCost = NaN\n  BuildTime = inf\n  CommandPoints = NOT_A_MACRO\n  \
            Geometry = BOX\n  GeometryMajorRadius = inf\n  \
            WeaponSet\n    Weapon = PRIMARY\n  End\n  \
            Behavior = AutoDepositUpdate Tag\n    DepositTiming = 6000\n  End\n  \
            Behavior = CastleBehavior Tag2\n    CastleToUnpackForFaction = Men\n  End\n\
            End\n";
    let odd = record(text, "Odd").unwrap();
    assert_eq!(odd.build_cost, None);
    assert_eq!(odd.build_time, None);
    assert_eq!(odd.command_points, None);
    assert_eq!(odd.geometry.radius, None);
    assert_eq!(odd.weapon, None, "a slot with no weapon name");
    assert_eq!(odd.income, None, "an amount is required");
    assert!(odd.castle.unwrap().unpack.is_empty());
    assert!(record(text, "NotThere").is_none());
}

#[test]
fn terrain_resource_behavior_feeds_the_same_income_field() {
    let text = "Object Farm\n  \
            Behavior = TerrainResourceBehavior ModuleTag_MoneyProduction\n    \
            MaxIncome = 25\n    IncomeInterval = 6000\n  End\n\
            End\n";
    let farm = record(text, "Farm").unwrap();
    assert_eq!(farm.income, Some((6000.0, 25.0)));
}

#[test]
fn claim_decal_and_priority_share_the_module_readers_defaults() {
    let text = "Object Farm\n  \
            Behavior = TerrainResourceBehavior ModuleTag_MoneyProduction\n  End\n\
            End\n";
    let farm = record(text, "Farm").unwrap();
    assert!(
        farm.claim_decal_visible,
        "an omitted Visible shows the ring"
    );
    assert!(!farm.claim_high_priority, "an omitted HighPriority is low");
}

#[test]
fn auto_deposit_update_takes_precedence_over_terrain_resource_behavior() {
    let text = "Object Both\n  \
            Behavior = TerrainResourceBehavior ModuleTag_Terrain\n    \
            MaxIncome = 1\n    IncomeInterval = 1\n  End\n  \
            Behavior = AutoDepositUpdate ModuleTag_Auto\n    \
            DepositAmount = 99\n    DepositTiming = 4000\n  End\n\
            End\n";
    let both = record(text, "Both").unwrap();
    assert_eq!(both.income, Some((4000.0, 99.0)));
    assert_eq!(
        both.claim_max_income,
        Some(1.0),
        "MaxIncome must survive beside an AutoDepositUpdate"
    );
}

#[test]
fn every_truncation_of_a_template_reads_without_panicking() {
    for end in 0..=FULL.len() {
        let Some(prefix) = FULL.get(..end) else {
            continue;
        };
        let Ok(document) = Document::parse("t.ini", prefix, &BlockTable::corpus_path()) else {
            continue;
        };
        let mut index = TemplateIndex::new();
        index.insert_document(&document);
        let _ = ObjectRecord::read(&index, "Keep", &mint_sage_ini::ValueErrors::new());
    }
}

#[test]
fn a_parent_cycle_terminates() {
    let text = "ChildObject A B\n  BuildCost = 1\nEnd\nChildObject B A\n  CommandSet = S\nEnd\n";
    let a = record(text, "A").unwrap();
    assert_eq!(a.build_cost, Some(1.0));
    assert_eq!(a.command_set.as_deref(), Some("S"));
}

#[test]
fn a_pad_reads_its_build_variation_and_a_castle_its_keep_death_rule() {
    let text = "\
Object Pad
  KindOf = STRUCTURE EXPANSION_PAD
  Behavior = FoundationAIUpdate Tag_F
    BuildVariation = 2
  End
End
ChildObject SidePad Pad
  Behavior = FoundationAIUpdate Tag_F
    BuildVariation = 1
  End
End
Object Site
  Behavior = CastleBehavior Tag_C
    KeepDeathKillsEverything = Yes
  End
End
Object MapCastle
  Behavior = CastleBehavior Tag_C
  End
End
";
    let pad = record(text, "Pad").unwrap().foundation.unwrap();
    assert_eq!(pad.build_variation_condition(), Some("BUILD_VARIATION_TWO"));
    let side = record(text, "SidePad").unwrap().foundation.unwrap();
    assert_eq!(
        side.build_variation_condition(),
        Some("BUILD_VARIATION_ONE")
    );
    assert!(record(text, "Site").unwrap().foundation.is_none());
    assert!(
        record(text, "Site")
            .unwrap()
            .castle
            .unwrap()
            .keep_death_kills_everything
    );
    assert!(
        !record(text, "MapCastle")
            .unwrap()
            .castle
            .unwrap()
            .keep_death_kills_everything
    );
}

#[test]
fn the_owned_filter_follows_the_object_filter_shapes() {
    let castle = |filter: &str| Castle {
        owned_filter: filter.split_whitespace().map(str::to_owned).collect(),
        ..Castle::default()
    };
    let pad = vec![String::from("STRUCTURE"), String::from("EXPANSION_PAD")];
    let rock = vec![String::from("IMMOBILE")];
    assert!(castle("ANY +STRUCTURE +BASE_FOUNDATION").keeps(&pad));
    assert!(!castle("ANY +STRUCTURE +BASE_FOUNDATION").keeps(&rock));
    assert!(!castle("ALL +STRUCTURE +BASE_FOUNDATION").keeps(&pad));
    assert!(castle("NONE +BASE_FOUNDATION").keeps(&pad));
    assert!(!castle("ANY +STRUCTURE -EXPANSION_PAD").keeps(&pad));
    assert!(castle("").keeps(&rock));
}

#[test]
fn unit_builds_are_the_unit_build_buttons_in_slot_order() {
    let document = parse(
            "CommandSet Set\n  3 = Build_B\n  1 = Build_A\n  2 = Upgrade\n  4 = Missing\n  5 = NoObject\nEnd\n\
             CommandButton Build_A\n  Command = UNIT_BUILD\n  Object = HordeA\nEnd\n\
             CommandButton Build_B\n  Command = UNIT_BUILD\n  Object = HordeB\nEnd\n\
             CommandButton Upgrade\n  Command = OBJECT_UPGRADE\n  Object = Thing\nEnd\n\
             CommandButton NoObject\n  Command = UNIT_BUILD\nEnd\n",
        );
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    sets.insert_document(&document);
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    let builds = unit_builds("set", &sets, &buttons);
    let objects: Vec<&str> = builds.iter().map(|b| b.object.as_str()).collect();
    assert_eq!(objects, ["HordeA", "HordeB"]);
    assert!(unit_builds("Nowhere", &sets, &buttons).is_empty());
}

#[test]
fn dozer_constructs_are_the_dozer_construct_buttons_and_not_unit_build() {
    let document = parse(
        "CommandSet PorterSet\n  1 = Build_Barracks\n  2 = Train_Soldier\nEnd\n\
             CommandButton Build_Barracks\n  Command = DOZER_CONSTRUCT\n  Object = Barracks\nEnd\n\
             CommandButton Train_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n",
    );
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    sets.insert_document(&document);
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    let constructs = dozer_constructs("PorterSet", &sets, &buttons);
    let objects: Vec<&str> = constructs.iter().map(|b| b.object.as_str()).collect();
    assert_eq!(objects, ["Barracks"]);
    let builds = unit_builds("PorterSet", &sets, &buttons);
    assert_eq!(
        builds.iter().map(|b| b.object.as_str()).collect::<Vec<_>>(),
        ["Soldier"]
    );
}

#[test]
fn object_and_player_upgrade_buttons_are_read_and_do_not_cross_match() {
    let document = parse(
            "CommandSet Set\n  1 = Upg_Level2\n  2 = Upg_Oil\n  3 = Build_Soldier\nEnd\n\
             CommandButton Upg_Level2\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Level2\nEnd\n\
             CommandButton Upg_Oil\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_Oil\nEnd\n\
             CommandButton Build_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n",
        );
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    sets.insert_document(&document);
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    let object = object_upgrades("Set", &sets, &buttons);
    assert_eq!(
        object
            .iter()
            .map(|u| u.upgrade.as_str())
            .collect::<Vec<_>>(),
        ["Upgrade_Level2"]
    );
    let player = player_upgrades("Set", &sets, &buttons);
    assert_eq!(
        player
            .iter()
            .map(|u| u.upgrade.as_str())
            .collect::<Vec<_>>(),
        ["Upgrade_Oil"]
    );
    assert!(object_upgrades("Nowhere", &sets, &buttons).is_empty());
}

#[test]
fn an_upgrade_button_with_no_upgrade_field_is_skipped() {
    let document = parse(
        "CommandSet Set\n  1 = NoUpgrade\nEnd\n\
             CommandButton NoUpgrade\n  Command = OBJECT_UPGRADE\nEnd\n",
    );
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    sets.insert_document(&document);
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    assert!(object_upgrades("Set", &sets, &buttons).is_empty());
}

#[test]
fn special_power_buttons_read_both_commands_and_their_options() {
    let document = parse(
        "CommandSet Set\n  1 = Fire_Oil\n  2 = Fire_Toggled\n  3 = Missing\nEnd\n\
             CommandButton Fire_Oil\n  Command = SPECIAL_POWER\n  \
                SpecialPower = SpecialAbilityBoilingOil\nEnd\n\
             CommandButton Fire_Toggled\n  Command = SPECIAL_POWER_TOGGLE\n  \
                SpecialPower = SpecialAbilityStealth\n  Options = NEED_TARGET_POS\nEnd\n\
             CommandButton Missing\n  Command = SPECIAL_POWER\nEnd\n",
    );
    let mut sets = CommandSetIndex::new();
    let mut buttons = CommandButtonIndex::new();
    sets.insert_document(&document);
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    let powers = special_power_buttons("Set", &sets, &buttons);
    assert_eq!(
        powers.iter().map(|p| p.power.as_str()).collect::<Vec<_>>(),
        ["SpecialAbilityBoilingOil", "SpecialAbilityStealth"]
    );
    assert_eq!(powers[0].options, Vec::<String>::new());
    assert_eq!(powers[1].options, ["NEED_TARGET_POS"]);
}

#[test]
fn the_registry_finds_every_template_by_any_casing() {
    let document = parse(&format!("{FULL}ObjectReskin Other Keep\nEnd\n"));
    let mut index = TemplateIndex::new();
    index.insert_document(&document);
    let records = TemplateRecords::build(&index).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records.get("OTHER").map(|r| r.depth), Some(1));
    assert!(records.get("keep").is_some());
    assert!(records.get("nothing").is_none());
}

#[test]
fn crush_fields_are_read_and_an_absent_crushable_level_is_uncrushable() {
    let text = "\
Object Rider
  CrusherLevel = 3
  MountedCrusherLevel = 1
  MinCrushVelocityPercent = 40
  CrushDecelerationPercent = 20%
  CrushKnockback = 40
  CrushOnlyWhileCharging = Yes
End
Object Soldier
  CrushableLevel = -1
  Body = PorcupineFormationBodyModule ModuleTag_01
    MaxHealth = 100
    CrusherLevelResisted = 1
    CrushDamageWeaponTemplate = Spikes
  End
End
Object Wall
  CrusherLevel = -5
  CrushableLevel = 9999
End
";
    let rider = record(text, "rider").unwrap().crush;
    assert_eq!(rider.crusher_level, 3);
    assert_eq!(rider.crushable_level, UNCRUSHABLE);
    assert_eq!(rider.mounted_crusher_level, Some(1));
    assert_eq!(rider.mounted_crushable_level, None);
    assert_eq!(rider.min_velocity, 0.4);
    assert_eq!(rider.deceleration, 0.2);
    assert_eq!(rider.knockback, 40.0);
    assert_eq!(rider.z_factor, 1.0);
    assert!(rider.only_while_charging);
    assert_eq!(rider.resisted, None);

    let soldier = record(text, "soldier").unwrap().crush;
    assert_eq!(soldier.crusher_level, 0);
    assert_eq!(soldier.crushable_level, -1);
    assert_eq!(
        soldier.resisted,
        Some(CrushResistance {
            level: 1,
            weapon: Some("Spikes".to_owned())
        })
    );

    let wall = record(text, "wall").unwrap().crush;
    assert_eq!(wall.crusher_level, 0, "a negative level clamps to zero");
    assert_eq!(wall.crushable_level, 127, "an absurd level clamps to 127");
}

#[test]
fn hero_and_fortress_fields_are_read() {
    let text = "\
#define HERO_COST 1500
Object Hero
  KindOf = HERO INFANTRY
  IsTrainable = Yes
  Body = RespawnBody ModuleTag_01
    MaxHealth = 1000
  End
  Behavior = RespawnUpdate ModuleTag_02
    RespawnRules = AutoSpawn:No Cost:625 Time:60000 Health:100%
    RespawnRules = Level:3 AutoSpawn:Yes Cost:HERO_COST Time:1000 Health:50%
    RespawnEntry = Level:5 Cost:2000
    RespawnEntry = Level:7 Time:500
    DeathAnim = DYING
    RespawnFX = FX_HeroRevive
    AutoRespawnAtObjectFilter = NONE +CASTLE_KEEP
  End
  Behavior = ExperienceLevelCreate ModuleTag_03
    LevelToGrant = 5
  End
End
Object Fort
  KindOf = STRUCTURE
  IsTrainable = No
  CommandSet = FortSet
  Behavior = ProductionUpdate ModuleTag_01
    ProductionModifier
      RequiredUpgrade = Upgrade_Banners
      CostMultiplier = 0.80
      ModifierFilter = NONE +Porter
    End
    ProductionModifier
      RequiredUpgrade = Upgrade_Healing
      CostMultiplier = 0.80
      TimeMultiplier = 0.80
      HeroRevive = Yes
      ModifierFilter = NONE +HERO
    End
  End
End
CommandSet FortSet
  1 = Build_Porter
  2 = Revive_Ring
  3 = Revive_1
End
CommandButton Build_Porter
  Command = UNIT_BUILD
  Object = Porter
End
CommandButton Revive_Ring
  Command = REVIVE
  Options = NEED_UPGRADE
  NeededUpgrade = Upgrade_RingHero
End
CommandButton Revive_1
  Command = REVIVE
End
";
    let document = parse(text);
    let mut index = TemplateIndex::new();
    index.insert_document(&document);
    let errors = mint_sage_ini::ValueErrors::new();
    let hero = ObjectRecord::read(&index, "Hero", &errors).unwrap();
    assert_eq!(hero.is_trainable, Some(true));
    assert_eq!(hero.level_on_create, Some(5));
    assert_eq!(hero.respawn.len(), 2);
    let first = hero.respawn[0];
    assert_eq!(first.level, None);
    assert!(!first.auto_spawn);
    assert_eq!(
        (first.cost, first.time_ms, first.health),
        (Some(625.0), Some(60_000.0), Some(1.0))
    );
    let second = hero.respawn[1];
    assert_eq!(second.level, Some(3));
    assert!(second.auto_spawn);
    assert_eq!((second.cost, second.health), (Some(1500.0), Some(0.5)));
    assert_eq!(hero.respawn_entries.len(), 2);
    assert_eq!(
        (hero.respawn_entries[0].level, hero.respawn_entries[0].cost),
        (Some(5), Some(2000.0))
    );
    assert_eq!(
        (
            hero.respawn_entries[1].level,
            hero.respawn_entries[1].time_ms
        ),
        (Some(7), Some(500.0))
    );
    assert_eq!(hero.death_anim.as_deref(), Some("DYING"));
    assert_eq!(hero.respawn_fx.as_deref(), Some("FX_HeroRevive"));
    assert_eq!(hero.auto_respawn_at_object_filter, ["NONE", "+CASTLE_KEEP"]);
    let fort = ObjectRecord::read(&index, "Fort", &errors).unwrap();
    assert_eq!(fort.is_trainable, Some(false));
    assert_eq!(fort.production_modifiers.len(), 2);
    let revive = &fort.production_modifiers[1];
    assert_eq!(revive.required_upgrade.as_deref(), Some("Upgrade_Healing"));
    assert!(revive.hero_revive && !revive.hero_purchase);
    assert_eq!((revive.cost_multiplier, revive.time_multiplier), (0.8, 0.8));
    assert_eq!(revive.filter, ["NONE", "+HERO"]);
    assert_eq!(fort.production_modifiers[0].time_multiplier, 1.0);
    let mut sets = CommandSetIndex::new();
    sets.insert_document(&document);
    let mut buttons = CommandButtonIndex::new();
    buttons.insert_document(&document, &mint_sage_ini::MacroTable::new());
    assert_eq!(
        revive_buttons("FortSet", &sets, &buttons),
        ["Revive_Ring", "Revive_1"]
    );
}

#[test]
fn a_respawn_entry_with_a_bad_cost_is_refused() {
    let (record, errors) = read(
        "Object Hero\n  \
           Behavior = RespawnUpdate ModuleTag_01\n    \
             RespawnRules = Cost:625 Time:60000\n    \
             RespawnEntry = Level:3 Cost:lots\n  \
           End\nEnd\n",
        "Hero",
    );
    let hero = record.unwrap();
    assert_eq!(hero.respawn_entries.len(), 1);
    assert_eq!(hero.respawn_entries[0].level, Some(3));
    assert_eq!(hero.respawn_entries[0].cost, None);
    assert_eq!(errors.len(), 1);
}

#[test]
fn a_weapon_set_reads_every_slot_and_refuses_an_unknown_one() {
    let text = "Object Mod\n  \
        WeaponSet\n    \
          Conditions = None\n    \
          Weapon = PRIMARY ModBow\n    \
          Weapon = TERTIARY ModRam\n    \
          Weapon = SENARY ModGhost\n    \
          AutoChooseSources = TERTIARY FROM_PLAYER FROM_SCRIPT\n    \
          PreferredAgainst = PRIMARY INFANTRY\n    \
          OnlyAgainst = TERTIARY STRUCTURE BLOCKING_GATE\n  \
        End\n\
        End\n";
    let (record, refused) = read(text, "Mod");
    let record = record.unwrap();
    let set = &record.weapon_sets[0];
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(
        set.slots
            .iter()
            .map(|slot| (slot.slot, slot.weapon.as_str()))
            .collect::<Vec<_>>(),
        [
            (WeaponSlot::Primary, "ModBow"),
            (WeaponSlot::Tertiary, "ModRam"),
        ]
    );
    assert_eq!(set.slots[0].preferred_against, ["INFANTRY"]);
    assert_eq!(set.slots[0].sources, SourceMask::default());
    assert_eq!(set.slots[1].only_against, ["STRUCTURE", "BLOCKING_GATE"]);
    assert!(!set.slots[1]
        .sources
        .allows(mint_sage_combat::CommandSource::Ai));
    assert!(set.slots[1]
        .sources
        .allows(mint_sage_combat::CommandSource::Player));
}

#[test]
fn only_in_condition_reads_onto_its_own_slot_and_refuses_an_empty_line() {
    let text = "Object MountainTroll\n  \
        WeaponSet\n    \
          Conditions = None\n    \
          Weapon = PRIMARY TrollPunch\n    \
          Weapon = TERTIARY TrollPunch\n    \
          OnlyInCondition = TERTIARY MOVING\n    \
          OnlyInCondition = PRIMARY\n  \
        End\n\
        End\n";
    let (record, refused) = read(text, "MountainTroll");
    let record = record.unwrap();
    let set = &record.weapon_sets[0];
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(set.slots[0].only_in_condition, Vec::<String>::new());
    assert_eq!(set.slots[1].only_in_condition, ["MOVING"]);
}

#[test]
fn the_unapplied_weapon_set_fields_are_read_typed() {
    let text = "Object WildMountainGiant\n  \
        WeaponSet\n    \
          Conditions = None\n    \
          Weapon = PRIMARY GiantPunch\n    \
          ReadyStatusSharedWithinSet = Yes\n    \
          ShareWeaponReloadTime = Yes\n    \
          WeaponLockSharedAcrossSets = Yes\n    \
          DefaultWeaponChoiceCritera = SELECT_AT_RANDOM\n  \
        End\n\
        End\n";
    let (record, refused) = read(text, "WildMountainGiant");
    let record = record.unwrap();
    assert_eq!(refused.len(), 0, "{refused:?}");
    let set = &record.weapon_sets[0];
    assert!(set.ready_status_shared_within_set);
    assert!(set.share_weapon_reload_time);
    assert!(set.weapon_lock_shared_across_sets);
    assert_eq!(
        set.default_weapon_choice_criteria,
        Some(mint_sage_combat::WeaponChoiceCriteria::SelectAtRandom)
    );
}

#[test]
fn an_unrecognised_weapon_choice_criteria_is_refused() {
    let text = "Object WildMountainGiant\n  \
        WeaponSet\n    \
          Conditions = None\n    \
          Weapon = PRIMARY GiantPunch\n    \
          DefaultWeaponChoiceCritera = SELECT_AT_WHIM\n  \
        End\n\
        End\n";
    let (record, refused) = read(text, "WildMountainGiant");
    let record = record.unwrap();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(record.weapon_sets[0].default_weapon_choice_criteria, None);
}

#[test]
fn a_present_value_that_does_not_parse_is_refused_with_where_it_was_written() {
    for (line, field, value) in [
        ("BuildCost = sideways", "BuildCost", "sideways"),
        ("IsTrainable = maybe", "IsTrainable", "maybe"),
        ("RadarPriority = BRIGHT", "RadarPriority", "BRIGHT"),
        (
            "GeometryOffset = X:sideways Y:1",
            "GeometryOffset",
            "X:sideways",
        ),
    ] {
        let text = format!(
            "Object Keep
  Geometry = BOX
  {line}
End
"
        );
        let (record, refused) = read(&text, "Keep");
        assert!(record.is_some(), "{line}");
        assert_eq!(refused.len(), 1, "{line}: {refused:?}");
        let error = &refused[0];
        assert_eq!(
            (
                error.context.as_str(),
                error.field.as_str(),
                error.value.as_str()
            ),
            ("Keep", field, value),
            "{line}"
        );
    }
    let (_, refused) = read(
        "Object Keep
  Geometry = BOX
End
",
        "Keep",
    );
    assert!(
        refused.is_empty(),
        "an absent field is a default, not an error"
    );
}

#[test]
fn an_unset_radar_priority_takes_eas_default() {
    let text = "Object Unit\n  RadarPriority = unit\nEnd\n\
        ChildObject Heir Unit\nEnd\n\
        Object Pad\n  KindOf = STRUCTURE\nEnd\n\
        Object Flag\n  KindOf = STRUCTURE CAPTURABLE\nEnd\n\
        Object Tower\n  RadarPriority = INVALID\n  \
          Behavior = GarrisonContain ModuleTag_01\n  End\nEnd\n\
        Object Rock\n  RadarPriority = NOT_ON_RADAR\n  KindOf = CAPTURABLE\nEnd\n";
    let priority = |name: &str| record(text, name).unwrap().radar_priority;
    assert_eq!(priority("Unit"), RadarPriority::Unit);
    assert_eq!(priority("Heir"), RadarPriority::Unit);
    assert_eq!(priority("Pad"), RadarPriority::Invalid);
    assert_eq!(priority("Flag"), RadarPriority::Structure);
    assert_eq!(priority("Tower"), RadarPriority::Structure);
    assert_eq!(priority("Rock"), RadarPriority::NotOnRadar);
}

#[test]
fn contact_points_read_tags_and_refuse_an_unknown_one() {
    let text = "Object Base\n  \
          GeometryContactPoint = X:9 Y:9 Z:9\n\
        End\n\
        ChildObject Mod Base\n  \
          GeometryContactPoint = X:-15 Y: 15 Z:0 Repair\n  \
          GeometryContactPoint = X: 12 Y:-56 Z:56\n  \
          AttackContactPoint = X:0 Y:0 Z:92 Swoop Grab\n  \
          GeometryContactPoint = X:1 Y:1 Z:0 Rampart\n\
        End\n";
    let (record, refused) = read(text, "Mod");
    let points = record.unwrap().contact_points;
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(
        points,
        [
            ContactPoint {
                position: [-15.0, 15.0, 0.0],
                tags: ContactTags::REPAIR,
            },
            ContactPoint {
                position: [12.0, -56.0, 56.0],
                tags: ContactTags::default(),
            },
            ContactPoint {
                position: [0.0, 0.0, 92.0],
                tags: ContactTags::SWOOP.union(ContactTags::GRAB),
            },
        ]
    );
}

#[test]
fn flammability_reads_its_quartet_and_refuses_an_out_of_range_value() {
    let text = "Object FireTestProp\n  \
          Flammability\n    \
            Fuel = 500\n    \
            MaxBurnRate = 20\n    \
            Decay = 2\n    \
            Resistance = 4\n  \
          End\n\
        End\n";
    let (record, refused) = read(text, "FireTestProp");
    assert!(refused.is_empty(), "{refused:?}");
    let material = record.unwrap().flammability.unwrap();
    assert_eq!(material.fuel, 500);
    assert_eq!(material.max_burn_rate, 20);
    assert_eq!(material.decay, 2);
    assert_eq!(material.resistance, 4);

    let bad = "Object FireTestBadProp\n  \
          Flammability\n    \
            Resistance = 999\n  \
          End\n\
        End\n";
    let (record, refused) = read(bad, "FireTestBadProp");
    assert_eq!(
        record.unwrap().flammability,
        None,
        "a refused field reads as absent"
    );
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].field, "Resistance");
}

#[test]
fn signed_set_conditions_edit_the_empty_list_and_a_mixed_line_is_refused() {
    let text = "Object Mod\n  \
        WeaponSet\n    Conditions = WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Bow\n  End\n  \
        WeaponSet\n    Conditions = -WEAPONSET_TOGGLE_1\n    Weapon = PRIMARY Spear\n  End\n  \
        WeaponSet\n    Conditions = +MOUNTED -CLOSE_RANGE\n    Weapon = PRIMARY Lance\n  End\n  \
        WeaponSet\n    Conditions = +MOUNTED CLOSE_RANGE\n    Weapon = PRIMARY Mixed\n  End\n\
        End\n";
    let (record, refused) = read(text, "Mod");
    let record = record.unwrap();
    assert_eq!(record.weapon.as_deref(), Some("Spear"));
    let conditions: Vec<&[String]> = record
        .weapon_sets
        .iter()
        .map(|set| set.conditions.as_slice())
        .collect();
    assert_eq!(
        conditions,
        [
            &["WEAPONSET_TOGGLE_1".to_owned()][..],
            &[],
            &["MOUNTED".to_owned()],
            &[]
        ]
    );
    assert_eq!(refused.len(), 1, "{refused:?}");
}
