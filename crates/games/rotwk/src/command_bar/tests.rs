#![allow(clippy::indexing_slicing, clippy::expect_used)]

use super::*;

const INI: &str = "
Object Porter
  KindOf = SELECTABLE DOZER
  CommandSet = PorterSet
  Body = ActiveBody Tag_01
    MaxHealth = 200
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Object Barracks
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = BarracksSet
  Body = StructureBody Tag_01
    MaxHealth = 400
  End
  Geometry = BOX
  GeometryMajorRadius = 20
  GeometryMinorRadius = 20
  GeometryHeight = 20
End
Object Horde
  KindOf = SELECTABLE INFANTRY
  CommandSet = HordeSet
  Body = ActiveBody Tag_01
    MaxHealth = 50
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Object Tower
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = TowerSet
  Body = StructureBody Tag_01
    MaxHealth = 100
  End
  Geometry = BOX
  GeometryMajorRadius = 10
  GeometryMinorRadius = 10
  GeometryHeight = 20
End
Object GarrisonTower
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = GarrisonSet
  Body = StructureBody Tag_01
    MaxHealth = 100
  End
  Geometry = BOX
  GeometryMajorRadius = 10
  GeometryMinorRadius = 10
  GeometryHeight = 20
End
Object Fortress
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = FortressSet
  Body = StructureBody Tag_01
    MaxHealth = 1000
  End
  Behavior = UnpauseSpecialPowerUpgrade Tag_02
    SpecialPowerTemplate = PowerOil
    TriggeredBy = Upgrade_Oil
  End
  Behavior = SpecialPowerModule Tag_03
    SpecialPowerTemplate = PowerOil
    StartsPaused = Yes
  End
  Behavior = UnpauseSpecialPowerUpgrade Tag_04
    SpecialPowerTemplate = PowerVision
    TriggeredBy = Upgrade_Tower
  End
  Behavior = OCLSpecialPower Tag_05
    SpecialPowerTemplate = PowerVision
    StartsPaused = Yes
  End
  Geometry = BOX
  GeometryMajorRadius = 30
  GeometryMinorRadius = 30
  GeometryHeight = 30
End
Object EvilFortress
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = EvilFortressSet
  Body = StructureBody Tag_01
    MaxHealth = 1000
  End
  Behavior = UnpauseSpecialPowerUpgrade Tag_02
    SpecialPowerTemplate = PowerFrost
    TriggeredBy = Upgrade_SanctumReady
  End
  Behavior = SpecialPowerModule Tag_03
    SpecialPowerTemplate = PowerFrost
    StartsPaused = Yes
  End
  Geometry = BOX
  GeometryMajorRadius = 30
  GeometryMinorRadius = 30
  GeometryHeight = 30
End
Object Shed
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  Body = StructureBody Tag_01
    MaxHealth = 100
  End
  Geometry = BOX
  GeometryMajorRadius = 10
  GeometryMinorRadius = 10
  GeometryHeight = 10
End
Object Soldier
  KindOf = SELECTABLE INFANTRY
  BuildCost = 200
  Body = ActiveBody Tag_01
    MaxHealth = 50
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Object Pad
  KindOf = STRUCTURE SELECTABLE IMMOBILE UNATTACKABLE EXPANSION_PAD
  CommandSet = PadSet
  Behavior = FoundationAIUpdate Tag_01
    BuildVariation = 2
  End
  Body = ImmortalBody Tag_02
    MaxHealth = 100
  End
End
Object PadTower
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  BuildCost = 300
  BuildTime = 10
  Body = StructureBody Tag_01
    MaxHealth = 500
  End
End
CommandSet PadSet
  1 = Build_PadShed
  2 = Build_PadTower
End
CommandButton Build_PadShed
  Command = FOUNDATION_CONSTRUCT
  Object = Shed
  Radial = Yes
End
CommandButton Build_PadTower
  Command = FOUNDATION_CONSTRUCT
  Object = PadTower
  Radial = Yes
End
CommandSet PorterSet
  1 = Command_Stop
  2 = Command_Extinguish
  3 = Build_Shed
  4 = Build_Shed2
  5 = Build_Shed3
End
CommandSet BarracksSet
  1 = Build_Soldier
  3 = Build_Soldier2
End
CommandSet HordeSet
  1 = Command_ToggleStance
  13 = Command_Stop
  16 = Command_StanceBattle
  17 = Command_StanceAggressive
  18 = Command_StanceHoldGround
End
CommandSet TowerSet
  1 = Command_ExitGarrison
  2 = Command_ExitGarrison
  3 = Command_ExitGarrison
  4 = Command_EvacuateAll
End
CommandSet GarrisonSet
  1 = Command_ExitAnyContainer
  2 = Command_ExitAnyContainer
  3 = Command_EvacuateAnyContainer
End
CommandSet FortressSet
  InitialVisible = 7
  1 = Build_Soldier
  2 = Select_Heroes
  3 = Fire_Oil
  4 = Use_Vision
  5 = Select_Upgrades
  6 = Command_Sell
  8 = Upgrade_A
  9 = Radial_Back
  15 = Upgrade_Castle
  16 = Upgrade_Player
  17 = Upgrade_A_Hidden
  10 = Revive_Ring
  11 = Revive_1
  12 = Revive_2
  13 = Revive_3
  14 = Radial_Back
End
CommandSet EvilFortressSet
  InitialVisible = 7
  1 = Build_Soldier
  4 = Fire_Frost
  6 = Command_Sell
End
CommandButton Command_Stop
  Command = STOP
  InPalantir = Yes
End
CommandButton Command_Extinguish
  Command = SPECIAL_POWER
  InPalantir = Yes
  Radial = No
End
CommandButton Build_Shed
  Command = DOZER_CONSTRUCT
  Object = Shed
  Radial = Yes
End
CommandButton Build_Shed2
  Command = DOZER_CONSTRUCT
  Object = Shed
  Radial = Yes
End
CommandButton Build_Shed3
  Command = DOZER_CONSTRUCT
  Object = Shed
  Radial = Yes
End
CommandButton Build_Soldier
  Command = UNIT_BUILD
  Object = Soldier
  InPalantir = Yes
  Radial = Yes
End
CommandButton Build_Soldier2
  Command = UNIT_BUILD
  Object = Soldier
  InPalantir = Yes
  Radial = Yes
End
CommandButton Command_ToggleStance
  Command = TOGGLE_STANCE
  TextLabel = Label_HoldGround Label_Battle Label_Aggressive
  ButtonImage = Image_HoldGround Image_Battle Image_Aggressive
  Stances = HoldGround Battle Aggressive
  InPalantir = Yes
End
CommandButton Command_StanceBattle
  Command = SET_STANCE
  Stances = Battle
  InPalantir = No
End
CommandButton Command_StanceAggressive
  Command = SET_STANCE
  Stances = Aggressive
  InPalantir = No
End
CommandButton Command_StanceHoldGround
  Command = SET_STANCE
  Stances = HoldGround
  InPalantir = No
End
CommandButton Command_ExitGarrison
  Command = EXIT_CONTAINER
  InPalantir = Yes
End
CommandButton Command_EvacuateAll
  Command = EVACUATE
  InPalantir = Yes
End
CommandButton Command_ExitAnyContainer
  Command = EXIT_CONTAINER
  InPalantir = Yes
  RequiresValidContainer = Yes
End
CommandButton Command_EvacuateAnyContainer
  Command = EVACUATE
  InPalantir = Yes
  RequiresValidContainer = Yes
End
CommandButton Select_Heroes
  Command = PUSH_VISIBLE_COMMAND_RANGE
  CommandRangeStart = 9
  CommandRangeCount = 5
  Radial = Yes
End
CommandButton Select_Upgrades
  Command = PUSH_VISIBLE_COMMAND_RANGE
  CommandRangeStart = 7
  CommandRangeCount = 2
  Radial = Yes
End
CommandButton Fire_Oil
  Command = SPECIAL_POWER
  SpecialPower = PowerOil
  InPalantir = Yes
  Radial = No
  AutoAbility = Yes
  NeedDamagedTarget = Yes
End
CommandButton Use_Vision
  Command = SPECIAL_POWER
  SpecialPower = PowerVision
  Options = HIDE_WHILE_DISABLED
  InPalantir = Yes
  Radial = Yes
End
CommandButton Fire_Frost
  Command = SPECIAL_POWER
  SpecialPower = PowerFrost
  Options = HIDE_WHILE_DISABLED NEED_TARGET_POS
  InPalantir = Yes
  Radial = Yes
End
CommandButton Command_Sell
  Command = SELL
  InPalantir = Yes
End
CommandButton Upgrade_A
  Command = OBJECT_UPGRADE
  Upgrade = Upgrade_A
  InPalantir = No
  Radial = Yes
End
CommandButton Upgrade_Castle
  Command = CASTLE_UPGRADE
  Upgrade = Upgrade_Castle
  InPalantir = No
  Radial = Yes
End
CommandButton Upgrade_Player
  Command = PLAYER_UPGRADE
  Upgrade = Upgrade_Player
  InPalantir = No
  Radial = Yes
End
CommandButton Upgrade_A_Hidden
  Command = OBJECT_UPGRADE
  Upgrade = Upgrade_A
  Options = HIDE_WHILE_DISABLED
  InPalantir = No
  Radial = Yes
End
CommandButton Radial_Back
  Command = POP_VISIBLE_COMMAND_RANGE
  InPalantir = No
  Radial = Yes
End
CommandButton Revive_Ring
  Command = REVIVE
  Options = HIDE_WHILE_DISABLED NEED_UPGRADE
  NeededUpgrade = Upgrade_RingHero
  InPalantir = No
  Radial = Yes
End
CommandButton Revive_1
  Command = REVIVE
  Options = HIDE_WHILE_DISABLED
  InPalantir = No
  Radial = Yes
End
CommandButton Revive_2
  Command = REVIVE
  InPalantir = No
  Radial = Yes
End
CommandButton Revive_3
  Command = REVIVE
  Options = HIDE_WHILE_DISABLED
  InPalantir = No
  Radial = Yes
End
Object Captain
  KindOf = SELECTABLE INFANTRY HERO
  CommandSet = CaptainSet
  Body = ActiveBody Tag_01
    MaxHealth = 300
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
CommandSet CaptainSet
  1 = Command_CaptainLeadership
  2 = Command_CaptainStrike
  3 = Command_CaptainRocks
End
CommandButton Command_CaptainRocks
  Command = TOGGLE_WEAPONSET
  Options = TOGGLE_IMAGE_ON_WEAPONSET OK_FOR_MULTI_SELECT
  FlagsUsedForToggle = WEAPONSET_TOGGLE_1
  TextLabel = CONTROLBAR:Throw CONTROLBAR:Punch
  ButtonImage = ThrowImage PunchImage
  InPalantir = Yes
End
CommandButton Command_CaptainLeadership
  Command = SPECIAL_POWER
  SpecialPower = PowerLeadership
  Options = NONPRESSABLE
  InPalantir = Yes
End
CommandButton Command_CaptainStrike
  Command = SPECIAL_POWER
  SpecialPower = PowerStrike
  InPalantir = Yes
End
CommandButton Command_CancelConstruction
  Command = FOUNDATION_CONSTRUCT_CANCEL
  TextLabel = CONTROLBAR:CancelBuild
  ButtonImage = BCSell
  Radial = Yes
  InPalantir = Yes
End
";

const NAMES: [&str; 11] = [
    "Captain",
    "Porter",
    "Tower",
    "GarrisonTower",
    "Barracks",
    "Horde",
    "Fortress",
    "EvilFortress",
    "Shed",
    "Soldier",
    "Pad",
];

fn resolve_with(name: &str, upgrades: &[&str], heroes: &[Hero]) -> CommandBar {
    resolve_full(name, upgrades, heroes, &[])
}

fn resolve_full(name: &str, upgrades: &[&str], heroes: &[Hero], released: &[&str]) -> CommandBar {
    resolve_toggled(name, upgrades, heroes, released, false)
}

fn resolve_toggled(
    name: &str,
    upgrades: &[&str],
    heroes: &[Hero],
    released: &[&str],
    raised: bool,
) -> CommandBar {
    let data = crate::game::tests::data_from(INI);
    let (kinds, missing) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind(name).expect("resolved");
    let upgrades: Vec<String> = upgrades.iter().map(|&u| u.to_owned()).collect();
    let released: Vec<String> = released.iter().map(|&p| p.to_owned()).collect();
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &upgrades,
        object_upgrades: &upgrades,
        player_upgrades: &upgrades,
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes,
        released_powers: &released,
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| raised,
        foreign_owner: false,
    };
    resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    )
}

fn resolve_with_sell_refusal(name: &str, sell_refusal: Option<&'static str>) -> CommandBar {
    let data = crate::game::tests::data_from(INI);
    let (kinds, missing) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind(name).expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    )
}

fn resolve(name: &str) -> CommandBar {
    resolve_with(name, &[], &[])
}

fn resolve_under_construction(name: &str, cancel_refusal: Option<&'static str>) -> CommandBar {
    let data = crate::game::tests::data_from(INI);
    let (kinds, missing) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind(name).expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal,
        under_construction: true,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    )
}

#[test]
fn a_construction_site_shows_one_cancel_radial_slot_with_its_image() {
    let bar = resolve_under_construction("Barracks", None);
    assert_eq!(radial_names(&bar), vec![(1, "Command_CancelConstruction")]);
    assert_eq!(bar.radial[0].1.image.as_deref(), Some("BCSell"));
    assert!(bar.radial[0].1.enabled);
}

#[test]
fn a_refused_cancel_rests_disabled() {
    let bar = resolve_under_construction("Barracks", Some("a rebuild, not a paid construction"));
    assert!(!bar.radial[0].1.enabled);
    assert_eq!(bar.radial[0].1.availability, Availability::Disabled);
}

fn ring_names(bar: &CommandBar) -> Vec<Option<&str>> {
    bar.ring
        .iter()
        .map(|s| s.as_ref().map(|s| s.command_button.as_str()))
        .collect()
}

fn radial_names(bar: &CommandBar) -> Vec<(u32, &str)> {
    bar.radial
        .iter()
        .map(|(n, s)| (*n, s.command_button.as_str()))
        .collect()
}

fn hero(name: &str, fielded: bool, affordable: bool) -> Hero {
    Hero {
        name: name.to_owned(),
        image: Some(format!("HI{name}")),
        fielded,
        affordable,
        allowed: true,
    }
}

#[test]
fn the_ring_holds_slots_one_to_six_by_position() {
    let bar = resolve("Barracks");
    assert_eq!(
        ring_names(&bar),
        [
            Some("Build_Soldier"),
            None,
            Some("Build_Soldier2"),
            None,
            None,
            None
        ]
    );
    assert_eq!(bar.ring_count(), 2);
    assert_eq!(
        bar.ring[2].as_ref().map(|s| &s.action),
        Some(&SlotAction::Build(1))
    );
}

#[test]
fn a_build_slot_carries_its_targets_cost_and_other_slots_carry_none() {
    let bar = resolve("Barracks");
    assert_eq!(
        bar.ring[0].as_ref().and_then(|s| s.cost),
        Some(200),
        "Build_Soldier's target, Soldier, sets BuildCost = 200"
    );
    let porter = resolve("Porter");
    assert_eq!(
        porter.ring[0].as_ref().and_then(|s| s.cost),
        None,
        "Command_Stop is not a build slot"
    );
}

#[test]
fn a_structure_shows_nothing_in_the_side_bar_and_its_radial_in_the_ring() {
    let barracks = resolve("Barracks");
    assert!(barracks.bar.is_empty());
    assert_eq!(
        radial_names(&barracks),
        [(1, "Build_Soldier"), (3, "Build_Soldier2")]
    );
    assert!(resolve("Fortress").bar.is_empty());
}

#[test]
fn a_builders_constructions_fill_the_side_bar() {
    let bar = resolve("Porter");
    let names: Vec<_> = bar.bar.iter().map(|s| s.command_button.as_str()).collect();
    assert_eq!(names, ["Build_Shed", "Build_Shed2", "Build_Shed3"]);
    assert_eq!(bar.bar[2].action, SlotAction::Build(2));
    assert!(bar.radial.is_empty());
    assert_eq!(
        ring_names(&bar),
        [
            Some("Command_Stop"),
            Some("Command_Extinguish"),
            None,
            None,
            None,
            None
        ]
    );
}

#[test]
fn a_hordes_stances_appear_nowhere() {
    let bar = resolve("Horde");
    assert!(bar.bar.is_empty(), "{:?}", bar.bar);
    assert!(bar.radial.is_empty());
    assert_eq!(bar.ring_count(), 1);
    assert!(matches!(
        bar.ring[0].as_ref().map(|s| &s.action),
        Some(&SlotAction::ToggleStance(_))
    ));
}

#[test]
fn the_sell_button_dims_exactly_when_the_simulation_would_refuse_it() {
    let ready = resolve_with_sell_refusal("Fortress", None);
    let sell = ready.ring[5].as_ref().expect("sell slot");
    assert_eq!(sell.action, SlotAction::Sell);
    assert!(sell.enabled, "nothing refuses it: usable");
    assert_eq!(sell.availability, Availability::Enabled);

    let refused = resolve_with_sell_refusal("Fortress", Some("already being sold"));
    let sell = refused.ring[5].as_ref().expect("sell slot");
    assert!(!sell.enabled, "the simulation would refuse it: dimmed");
    assert_eq!(sell.availability, Availability::Disabled);
}

#[test]
fn the_men_fortress_matches_the_owners_retail_reading() {
    let bar = resolve("Fortress");
    assert_eq!(
        ring_names(&bar),
        [
            Some("Build_Soldier"),
            None,
            Some("Fire_Oil"),
            None,
            None,
            Some("Command_Sell")
        ]
    );
    assert!(!bar.ring[2].as_ref().expect("oil").enabled, "oil disabled");
    assert!(bar.ring[0].as_ref().expect("builder").enabled);
    assert_eq!(bar.radial_root, Some(7));
    let root: Vec<_> = radial_names(&bar)
        .into_iter()
        .filter(|(n, _)| *n <= 7)
        .collect();
    assert_eq!(
        root,
        [
            (1, "Build_Soldier"),
            (2, "Select_Heroes"),
            (5, "Select_Upgrades")
        ]
    );
    assert_eq!(
        bar.radial[1].1.action,
        SlotAction::RadialPush { start: 9, count: 5 }
    );
    assert_eq!(
        bar.radial
            .iter()
            .find(|(n, _)| *n == 9)
            .map(|(_, s)| &s.action),
        Some(&SlotAction::RadialPop)
    );
}

#[test]
fn a_released_power_is_usable_and_holding_its_upgrade_alone_is_not_enough() {
    let held_only = resolve_with("Fortress", &["Upgrade_Oil", "Upgrade_Tower"], &[]);
    assert!(!held_only.ring[2].as_ref().expect("oil").enabled);
    let bar = resolve_full(
        "Fortress",
        &["Upgrade_Oil", "Upgrade_Tower"],
        &[],
        &["PowerOil", "powervision"],
    );
    assert!(bar.ring[2].as_ref().expect("oil").enabled);
    assert_eq!(
        bar.ring[3].as_ref().map(|s| s.command_button.as_str()),
        Some("Use_Vision")
    );
    assert!(radial_names(&bar).contains(&(4, "Use_Vision")));
}

#[test]
fn a_context_mode_button_waits_for_a_click_that_carries_no_target() {
    let options = |list: &[&str]| list.iter().map(|o| (*o).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        crate::templates::target_kind(&options(&["CONTEXTMODE_COMMAND"])),
        TargetKind::Confirm
    );
    assert_eq!(
        crate::templates::target_kind(&options(&["NEED_TARGET_POS", "CONTEXTMODE_COMMAND"])),
        TargetKind::Position
    );
    assert_eq!(
        crate::templates::target_kind(&options(&[
            "NEED_TARGET_ENEMY_OBJECT",
            "CONTEXTMODE_COMMAND"
        ])),
        TargetKind::Object
    );
}

#[test]
fn upgrade_and_power_buttons_resolve_to_real_slot_actions() {
    let bar = resolve("Fortress");
    assert_eq!(
        bar.ring[2].as_ref().map(|s| &s.action),
        Some(&SlotAction::FireSpecialPower {
            button: 0,
            target_kind: crate::templates::TargetKind::None,
            name: "Fire_Oil".to_owned()
        }),
        "Fire_Oil carries no NEED_TARGET_* option; NeedDamagedTarget is no target"
    );
    let evil = resolve_full(
        "EvilFortress",
        &["Upgrade_SanctumReady"],
        &[],
        &["PowerFrost"],
    );
    assert_eq!(
        evil.ring[3].as_ref().map(|s| &s.action),
        Some(&SlotAction::FireSpecialPower {
            button: 0,
            target_kind: crate::templates::TargetKind::Position,
            name: "Fire_Frost".to_owned()
        }),
        "Fire_Frost carries NEED_TARGET_POS"
    );
    let upgrade = resolve("Fortress")
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_A")
        .map(|(_, slot)| slot.action.clone());
    assert_eq!(upgrade, Some(SlotAction::ObjectUpgrade(0)));
}

#[test]
fn a_recharging_special_power_button_is_not_ready_and_carries_its_clock() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let power_clock = |power: &str| (power == "PowerOil").then_some(42);
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &["Upgrade_Oil".to_owned()],
        object_upgrades: &["Upgrade_Oil".to_owned()],
        player_upgrades: &["Upgrade_Oil".to_owned()],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &["PowerOil".to_owned()],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &power_clock,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let oil = bar
        .ring
        .iter()
        .flatten()
        .find(|slot| slot.command_button == "Fire_Oil")
        .expect("Fire_Oil shown, released");
    assert_eq!(oil.availability, Availability::NotReady { percent: 42 });
    assert!(!oil.enabled);
    assert_eq!(
        oil.progress,
        Some(42),
        "the button's own {{key}}_Timer feed"
    );
}

#[test]
fn only_the_auto_cast_holders_own_button_carries_the_marker() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let auto_cast_on = |power: &str| power == "PowerOil" || power == "PowerVision";
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &["Upgrade_Oil".to_owned()],
        object_upgrades: &["Upgrade_Oil".to_owned()],
        player_upgrades: &["Upgrade_Oil".to_owned()],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &["PowerOil".to_owned(), "PowerVision".to_owned()],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &auto_cast_on,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let slots: Vec<&ResolvedSlot> = bar.ring.iter().flatten().collect();
    let oil = slots
        .iter()
        .find(|slot| slot.command_button == "Fire_Oil")
        .expect("Fire_Oil shown");
    assert!(
        oil.auto_cast,
        "PowerOil holds the slot and offers auto-cast"
    );
    let vision = slots
        .iter()
        .find(|slot| slot.command_button == "Use_Vision")
        .expect("Use_Vision shown, released");
    assert!(
        !vision.auto_cast,
        "Use_Vision does not declare AutoAbility, so it never carries the marker"
    );
}

#[test]
fn an_upgrade_in_production_is_held() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let state = |researching: &[String]| {
        let ctx = SlotContext {
            passengers: &[],
            passenger_entities: &[],
            capacity: None,
            afford: &|_| true,
            allowed_to_build: &|_| true,
            cost: &cost,
            upgrades: &[],
            object_upgrades: &[],
            player_upgrades: &[],
            castle_upgrades: &[],
            researching,
            upgrade_afford: &|_| true,
            upgrade_affected: &|_| true,
            heroes: &[],
            released_powers: &[],
            command_set: None,
            sell_refusal: None,
            self_repair: None,
            gate: None,
            cancel_refusal: None,
            under_construction: false,
            power_clock: &|_| None,
            auto_cast_on: &|_| false,
            weapon_set_raised: &|_| false,
            foreign_owner: false,
        };
        resolve_command_bar(
            &kinds,
            &data.command_sets,
            &data.command_buttons,
            Some(kind),
            &ctx,
        )
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_A")
        .map(|(_, slot)| slot.availability)
    };
    assert_eq!(state(&[]), Some(Availability::Enabled));
    assert_eq!(state(&["upgrade_a".to_owned()]), Some(Availability::Held));
}

#[test]
fn a_castle_upgrade_holds_on_the_castles_own_held_upgrade_not_the_objects() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let state = |upgrades: &[String], castle_upgrades: &[String], researching: &[String]| {
        let ctx = SlotContext {
            passengers: &[],
            passenger_entities: &[],
            capacity: None,
            afford: &|_| true,
            allowed_to_build: &|_| true,
            cost: &cost,
            upgrades,
            object_upgrades: &[],
            player_upgrades: &[],
            castle_upgrades,
            researching,
            upgrade_afford: &|_| true,
            upgrade_affected: &|_| true,
            heroes: &[],
            released_powers: &[],
            command_set: None,
            sell_refusal: None,
            self_repair: None,
            gate: None,
            cancel_refusal: None,
            under_construction: false,
            power_clock: &|_| None,
            auto_cast_on: &|_| false,
            weapon_set_raised: &|_| false,
            foreign_owner: false,
        };
        resolve_command_bar(
            &kinds,
            &data.command_sets,
            &data.command_buttons,
            Some(kind),
            &ctx,
        )
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_Castle")
        .map(|(_, slot)| slot.availability)
    };
    assert_eq!(
        state(&["Upgrade_Castle".to_owned()], &[], &[]),
        Some(Availability::Enabled),
        "the object's own upgrade mask is not what this button reads"
    );
    assert_eq!(
        state(&[], &["Upgrade_Castle".to_owned()], &[]),
        Some(Availability::Held),
        "the castle already holds it"
    );
    assert_eq!(
        state(&[], &[], &["Upgrade_Castle".to_owned()]),
        Some(Availability::Enabled),
        "queued, not held - a CASTLE_UPGRADE button stays enabled"
    );
}

#[test]
fn an_unaffordable_upgrade_button_shows_cant_afford_not_enabled() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let afford = |name: &str| -> bool {
        name != "Upgrade_A" && name != "Upgrade_Castle" && name != "Upgrade_Player"
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &afford,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let state_of = |name: &str| {
        bar.radial
            .iter()
            .find(|(_, slot)| slot.command_button == name)
            .map(|(_, slot)| slot.availability)
    };
    assert_eq!(state_of("Upgrade_A"), Some(Availability::CantAfford));
    assert_eq!(state_of("Upgrade_Castle"), Some(Availability::CantAfford));
    assert_eq!(state_of("Upgrade_Player"), Some(Availability::CantAfford));
}

fn held_ctx<'a>(
    cost: &'a dyn Fn(Kind) -> Option<i64>,
    object_upgrades: &'a [String],
    player_upgrades: &'a [String],
    foreign_owner: bool,
) -> SlotContext<'a> {
    SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost,
        upgrades: &[],
        object_upgrades,
        player_upgrades,
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner,
    }
}

#[test]
fn a_held_player_upgrade_is_held_and_shows_the_cant_afford_frame() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let player_upgrades = ["Upgrade_Player".to_owned()];
    let ctx = held_ctx(&cost, &[], &player_upgrades, false);
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let slot = bar
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_Player")
        .map(|(_, slot)| slot)
        .expect("Upgrade_Player shown");
    assert_eq!(slot.availability, Availability::Held);
    assert_eq!(crate::hud::button_state(slot, false), "_cantAfford");
}

#[test]
fn an_object_upgrade_held_only_by_the_player_stays_enabled() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let player_upgrades = ["Upgrade_A".to_owned()];
    let ctx = held_ctx(&cost, &[], &player_upgrades, false);
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let state = bar
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_A")
        .map(|(_, slot)| slot.availability);
    assert_eq!(state, Some(Availability::Enabled));
}

#[test]
fn hide_while_disabled_does_not_hide_a_held_object_upgrade() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let object_upgrades = ["Upgrade_A".to_owned()];
    let ctx = held_ctx(&cost, &object_upgrades, &[], false);
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let state = bar
        .radial
        .iter()
        .find(|(_, slot)| slot.command_button == "Upgrade_A_Hidden")
        .map(|(_, slot)| slot.availability);
    assert_eq!(state, Some(Availability::Held));
}

#[test]
fn a_foreign_owners_enabled_button_is_visually_enabled_only() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    let kind = kinds.kind("Fortress").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = held_ctx(&cost, &[], &[], true);
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    let slot = bar
        .ring
        .iter()
        .flatten()
        .find(|slot| slot.command_button == "Command_Sell")
        .expect("Command_Sell shown");
    assert_eq!(slot.availability, Availability::VisuallyEnabled);
    assert!(!slot.enabled);
    assert_eq!(crate::hud::button_state(slot, false), "_visuallyEnabled");
}

#[test]
fn special_power_buttons_are_indexed_among_themselves_only() {
    let bar = resolve_full(
        "Fortress",
        &["Upgrade_Oil", "Upgrade_Tower"],
        &[],
        &["PowerOil", "PowerVision"],
    );
    let vision = bar
        .ring
        .iter()
        .flatten()
        .find(|slot| slot.command_button == "Use_Vision")
        .map(|slot| slot.action.clone());
    assert_eq!(
        vision,
        Some(SlotAction::FireSpecialPower {
            button: 1,
            target_kind: crate::templates::TargetKind::None,
            name: "Use_Vision".to_owned()
        })
    );
}

#[test]
fn a_nonpressable_button_is_passive_and_keeps_the_power_indices() {
    let bar = resolve("Captain");
    let action = |p: usize| bar.ring[p].as_ref().map(|slot| slot.action.clone());
    assert_eq!(action(0), Some(SlotAction::Passive));
    assert_eq!(
        action(1),
        Some(SlotAction::FireSpecialPower {
            button: 1,
            target_kind: crate::templates::TargetKind::None,
            name: "Command_CaptainStrike".to_owned()
        })
    );
}

#[test]
fn a_weapon_set_toggle_shows_its_second_face_while_toggled() {
    let image = |raised| {
        resolve_toggled("Captain", &[], &[], &[], raised).ring[2]
            .as_ref()
            .map(|slot| slot.image.clone())
    };
    assert_eq!(image(false), Some(None));
    assert_eq!(image(true), Some(Some("PunchImage".to_owned())));
}

#[test]
fn the_angmar_frostball_is_hidden_until_its_upgrade() {
    let bar = resolve("EvilFortress");
    assert_eq!(
        ring_names(&bar),
        [
            Some("Build_Soldier"),
            None,
            None,
            None,
            None,
            Some("Command_Sell")
        ]
    );
    let upgraded = resolve_full(
        "EvilFortress",
        &["Upgrade_SanctumReady"],
        &[],
        &["PowerFrost"],
    );
    let frost = upgraded.ring[3].as_ref().expect("frostball shown");
    assert_eq!(frost.command_button, "Fire_Frost");
    assert!(frost.enabled);
}

#[test]
fn hero_slots_fill_from_the_hero_list_in_order() {
    let heroes = [
        hero("RingHero", false, true),
        hero("A", false, true),
        hero("B", true, true),
        hero("C", false, false),
    ];
    let bar = resolve_with("Fortress", &[], &heroes);
    let page: Vec<_> = bar
        .radial
        .iter()
        .filter(|(n, _)| (10..=14).contains(n))
        .map(|(n, s)| (*n, s.label.as_str(), s.enabled, s.image.as_deref()))
        .collect();
    assert_eq!(
        page,
        [
            (11, "A", true, Some("HIA")),
            (13, "C", false, Some("HIC")),
            (14, "Radial_Back", true, None)
        ]
    );
    let c = bar.radial.iter().find(|(n, _)| *n == 13).map(|(_, s)| s);
    assert_eq!(
        c.map(|s| s.availability),
        Some(Availability::CantAfford),
        "only the money is missing"
    );
    let short = resolve_with("Fortress", &["Upgrade_RingHero"], &heroes[..2]);
    let labels: Vec<_> = short
        .radial
        .iter()
        .filter(|(n, _)| (10..=13).contains(n))
        .map(|(_, s)| s.label.as_str())
        .collect();
    assert_eq!(labels, ["RingHero", "A"], "slots 12-13 have no hero");
    let actions: Vec<_> = bar
        .radial
        .iter()
        .filter(|(n, _)| (10..=13).contains(n))
        .map(|(_, s)| s.action.clone())
        .collect();
    assert_eq!(actions, [SlotAction::Revive(1), SlotAction::Revive(3)]);
}

#[test]
fn a_forbidden_revive_hero_shows_extra_disabled() {
    let heroes = [Hero {
        name: "RingHero".to_owned(),
        image: Some("HIRingHero".to_owned()),
        fielded: false,
        affordable: true,
        allowed: false,
    }];
    let bar = resolve_with("Fortress", &["Upgrade_RingHero"], &heroes);
    let slot = bar.radial.iter().find(|(n, _)| *n == 10).map(|(_, s)| s);
    assert_eq!(
        slot.map(|s| s.availability),
        Some(Availability::ExtraDisabled)
    );
}

#[test]
fn a_forbidden_build_target_shows_extra_disabled() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, missing) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind("Barracks").expect("resolved");
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| false,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    );
    assert_eq!(
        bar.ring[0].as_ref().map(|s| s.availability),
        Some(Availability::ExtraDisabled)
    );
}

#[test]
fn no_selection_shows_nothing() {
    let data = crate::game::tests::data_from(INI);
    let (kinds, _) = KindTable::closure(&data, &["Porter".to_owned()]);
    let cost = |k: Kind| {
        kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    let bar = resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        None,
        &ctx,
    );
    assert_eq!(bar, CommandBar::default());
}

#[test]
fn a_pads_foundation_buttons_are_radial_builds_indexed_in_slot_order() {
    let bar = resolve("Pad");
    assert_eq!(
        radial_names(&bar),
        [(1, "Build_PadShed"), (2, "Build_PadTower")]
    );
    let actions: Vec<&SlotAction> = bar.radial.iter().map(|(_, slot)| &slot.action).collect();
    assert_eq!(
        actions,
        [
            &SlotAction::FoundationConstruct(0),
            &SlotAction::FoundationConstruct(1)
        ]
    );
    assert_eq!(bar.radial[1].1.label, "PadTower");
    assert_eq!(bar.radial[1].1.cost, Some(300));
    assert!(bar.radial.iter().all(|(_, slot)| slot.enabled));
}

fn resolve_container(name: &str, passengers: &[Option<String>], capacity: u32) -> CommandBar {
    let data = crate::game::tests::data_from(INI);
    let (kinds, missing) = KindTable::closure(&data, &NAMES.map(str::to_owned));
    assert!(missing.is_empty(), "{missing:?}");
    let kind = kinds.kind(name).expect("resolved");
    let ctx = SlotContext {
        passengers,
        passenger_entities: &[],
        capacity: Some(capacity),
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &|_| None,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &[],
        command_set: None,
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        foreign_owner: false,
    };
    resolve_command_bar(
        &kinds,
        &data.command_sets,
        &data.command_buttons,
        Some(kind),
        &ctx,
    )
}

#[test]
fn each_exit_container_slot_draws_its_own_passenger() {
    let bar = resolve_container(
        "Tower",
        &[
            Some("Archer_Image".to_owned()),
            Some("Pike_Image".to_owned()),
        ],
        3,
    );
    let images: Vec<Option<String>> = bar
        .ring
        .iter()
        .take(3)
        .map(|slot| slot.as_ref().and_then(|slot| slot.image.clone()))
        .collect();
    assert_eq!(
        images,
        vec![
            Some("Archer_Image".to_owned()),
            Some("Pike_Image".to_owned()),
            None
        ]
    );
    let enabled: Vec<bool> = bar
        .ring
        .iter()
        .take(3)
        .map(|slot| slot.as_ref().is_some_and(|slot| slot.enabled))
        .collect();
    assert_eq!(enabled, vec![true, true, false]);
}

#[test]
fn a_requires_valid_container_button_is_enabled_on_an_empty_container() {
    let bar = resolve_container("GarrisonTower", &[], 2);
    let enabled: Vec<bool> = bar
        .ring
        .iter()
        .take(3)
        .map(|slot| slot.as_ref().is_some_and(|slot| slot.enabled))
        .collect();
    assert_eq!(enabled, vec![true, true, true]);
    let empty = bar
        .ring
        .iter()
        .flatten()
        .filter(|slot| matches!(slot.action, SlotAction::EmptySlot))
        .count();
    assert_eq!(empty, 2, "an empty slot's press does nothing");
}

#[test]
fn an_exit_slot_past_the_contain_capacity_is_hidden() {
    let bar = resolve_container("Tower", &[], 2);
    assert!(bar.ring.get(2).is_some_and(Option::is_none));
}

#[test]
fn a_stance_toggle_offers_the_sets_set_stance_buttons_in_its_own_order() {
    let bar = resolve_with("Horde", &[], &[]);
    let names: Vec<&str> = bar
        .stance_flyout
        .iter()
        .map(|slot| slot.command_button.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Command_StanceHoldGround",
            "Command_StanceBattle",
            "Command_StanceAggressive"
        ]
    );
}

#[test]
fn a_stance_toggle_shows_the_face_of_the_current_stance() {
    let data = crate::game::tests::data_from(INI);
    let mut bar = resolve_with("Horde", &[], &[]);
    show_stance_face(&mut bar, &data.command_buttons, Stance::Battle);
    let toggle = bar.ring.first().and_then(Option::as_ref).expect("toggle");
    assert_eq!(
        (toggle.image.as_deref(), toggle.label.as_str()),
        (Some("Image_Battle"), "Label_Battle")
    );
}

fn string_table(pairs: &[(&str, &str)]) -> mint_sage_str::StringTable {
    let mut table = mint_sage_str::StringTable::new();
    table.insert(
        pairs
            .iter()
            .map(|&(label, text)| mint_sage_str::StringRecord {
                label: label.to_owned(),
                text: text.to_owned(),
                speech: None,
            }),
    );
    table
}

fn slot(label: &str, action: SlotAction, enabled: bool) -> ResolvedSlot {
    ResolvedSlot {
        action,
        label: label.to_owned(),
        command_button: String::new(),
        enabled,
        availability: if enabled {
            Availability::Enabled
        } else {
            Availability::Disabled
        },
        image: None,
        cost: None,
        progress: None,
        count: 0,
        auto_cast: false,
    }
}

#[test]
fn hotkey_letter_reads_the_character_after_the_ampersand() {
    let strings = string_table(&[("CONTROLBAR:AttackMove", "Attack &Move")]);
    assert_eq!(hotkey_letter(&strings, "CONTROLBAR:AttackMove"), Some('m'));
}

#[test]
fn hotkey_letter_is_none_without_an_ampersand_or_an_unresolved_label() {
    let strings = string_table(&[("CONTROLBAR:Stop", "Stop")]);
    assert_eq!(hotkey_letter(&strings, "CONTROLBAR:Stop"), None);
    assert_eq!(hotkey_letter(&strings, "CONTROLBAR:Missing"), None);
}

#[test]
fn armed_hotkey_action_skips_a_disabled_slot_with_the_same_letter() {
    let strings = string_table(&[
        ("CONTROLBAR:Stop", "&Stop"),
        ("CONTROLBAR:AttackMove", "Attack &Move"),
    ]);
    let mut bar = CommandBar::default();
    bar.ring[0] = Some(slot("CONTROLBAR:Stop", SlotAction::Stop, false));
    bar.bar.push(slot(
        "CONTROLBAR:AttackMove",
        SlotAction::ArmAttackMove,
        true,
    ));
    assert_eq!(armed_hotkey_action(&bar, &strings, 's'), None);
    assert_eq!(
        armed_hotkey_action(&bar, &strings, 'm'),
        Some(SlotAction::ArmAttackMove)
    );
}
