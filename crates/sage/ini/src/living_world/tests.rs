use super::auto_resolve::{ScheduleSide, UnitType};
use super::campaign::ActNugget;
use super::players::{BuildingNuggetKind, StrengthenBonus};
use super::*;
use crate::table::BlockTable;
use crate::tree::Document;
use crate::value::MacroTable;
use crate::MemorySources;

fn read_with(text: &str, macros: &MacroTable) -> (LivingWorldData, Vec<LoadError>) {
    let document = Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap();
    read_documents(&[document], macros)
}

fn read(text: &str) -> (LivingWorldData, Vec<LoadError>) {
    read_with(text, &MacroTable::new())
}

fn clean(text: &str) -> LivingWorldData {
    let (data, errors) = read(text);
    assert!(errors.is_empty(), "{errors:?}");
    data
}

const FRAGMENTS: &[(&str, &str)] = &[
    ("LivingWorldMapInfo", "LivingWorldMapInfo\n MapObject = M\n @@\nEnd\n"),
    ("EyeTower", "LivingWorldMapInfo\n EyeTower\n  PupilAnimObject = P\n  @@\n End\nEnd\n"),
    ("LivingWorldObject", "LivingWorldObject Army\n ObjectType = ARMY\n @@\nEnd\n"),
    ("LivingWorldSound", "LivingWorldSound S\n Sound = X\n @@\nEnd\n"),
    ("BannerType", "BannerType B\n FlagObj = F\n Icon = I\n @@\nEnd\n"),
    ("LivingWorldAnimObject", "LivingWorldAnimObject A\n Model = M\n @@\nEnd\n"),
    ("LivingWorldArmyIcon", "LivingWorldArmyIcon I\n OnSelectedSound = S\n @@\nEnd\n"),
    ("ArmyIcon Object", "LivingWorldArmyIcon I\n Object O\n  Model = M\n  @@\n End\nEnd\n"),
    ("LivingWorldBuildingIcon", "LivingWorldBuildingIcon I\n OnSelectedSound = S\n @@\nEnd\n"),
    ("BuildingIcon Object", "LivingWorldBuildingIcon I\n Object O\n  Model = M\n  @@\n End\nEnd\n"),
    ("LivingWorldBuildPlotIcon", "LivingWorldBuildPlotIcon I\n OnSelectedSound = S\n @@\nEnd\n"),
    ("BuildPlotIcon Object", "LivingWorldBuildPlotIcon I\n Object O\n  Model = M\n  @@\n End\nEnd\n"),
    ("LivingWorldPlayerTemplate", "LivingWorldPlayerTemplate P\n Faction = F\n @@\nEnd\n"),
    ("LivingWorldAITemplate", "LivingWorldAITemplate\n DesiredHeroRatio = 1\n @@\nEnd\n"),
    ("LivingWorldPlayerArmy", "LivingWorldPlayerArmy\n Name = A\n @@\nEnd\n"),
    ("ArmyEntry", "LivingWorldPlayerArmy\n Name = A\n ArmyEntry\n  Quantity = 1\n  @@\n End\nEnd\n"),
    ("LivingWorldBuilding", "LivingWorldBuilding B\n Type = Fortress\n @@\nEnd\n"),
    ("IncreaseCommandPoints", "LivingWorldBuilding B\n BuildingNugget IncreaseCommandPoints T\n  Amount = +30\n  @@\n End\nEnd\n"),
    ("SpawnArmy nugget", "LivingWorldBuilding B\n BuildingNugget SpawnArmy T\n  QueueSize = 6\n  @@\n End\nEnd\n"),
    ("ArmyToSpawn", "LivingWorldBuilding B\n BuildingNugget SpawnArmy T\n  ArmyToSpawn\n   BuildTime = 1\n   @@\n  End\n End\nEnd\n"),
    ("StrengthenArmy", "LivingWorldBuilding B\n BuildingNugget StrengthenArmy T\n  BonusKey = K\n  @@\n End\nEnd\n"),
    ("UpgradeTroops", "LivingWorldBuilding B\n BuildingNugget UpgradeTroops T\n  NumUpgradesPerTurn = 1\n  @@\n End\nEnd\n"),
    ("IncreaseTreasury", "LivingWorldBuilding B\n BuildingNugget IncreaseTreasury T\n  TreasureAmount = 5\n  @@\n End\nEnd\n"),
    ("LivingWorldCampaign", "LivingWorldCampaign C\n IsEvilCampaign = No\n @@\nEnd\n"),
    ("AddPlayer", "LivingWorldCampaign C\n AddPlayer P\n  TeamNumber = 1\n  @@\n End\nEnd\n"),
    ("Act", "LivingWorldCampaign C\n Act One\n  EndAct = Yes\n  @@\n End\nEnd\n"),
    ("Act SpawnArmy", "LivingWorldCampaign C\n Act One\n  SpawnArmy\n   ScriptingName = A\n   @@\n  End\n End\nEnd\n"),
    ("EnableRegion", "LivingWorldCampaign C\n Act One\n  EnableRegion\n   Region = R\n   @@\n  End\n End\nEnd\n"),
    ("ForceBattle", "LivingWorldCampaign C\n Act One\n  ForceBattle\n   Region = R\n   @@\n  End\n End\nEnd\n"),
    ("MoveArmy", "LivingWorldCampaign C\n Act One\n  MoveArmy\n   ArmyScriptingName = A\n   @@\n  End\n End\nEnd\n"),
    ("SpawnBuilding", "LivingWorldCampaign C\n Act One\n  SpawnBuilding\n   Player = P\n   @@\n  End\n End\nEnd\n"),
    ("MoveCamera", "LivingWorldCampaign C\n Act One\n  MoveCamera\n   ViewAngle = 1\n   @@\n  End\n End\nEnd\n"),
    ("SplineCamera", "LivingWorldCampaign C\n Act One\n  SplineCamera\n   SummaryEvent = No\n   @@\n  End\n End\nEnd\n"),
    ("ControlPoint", "LivingWorldCampaign C\n Act One\n  SplineCamera\n   ControlPoint\n    Angle = 1\n    @@\n   End\n  End\n End\nEnd\n"),
    ("WorldText", "LivingWorldCampaign C\n Act One\n  WorldText\n   StringTag = S\n   @@\n  End\n End\nEnd\n"),
    ("Act AudioEvent", "LivingWorldCampaign C\n Act One\n  AudioEvent\n   Sound = S\n   @@\n  End\n End\nEnd\n"),
    ("UpdateAnimObject", "LivingWorldCampaign C\n Act One\n  UpdateAnimObject\n   AnimObjectName = A\n   @@\n  End\n End\nEnd\n"),
    ("EyeTowerPoints", "LivingWorldCampaign C\n Act One\n  EyeTowerPoints\n   LookPoint = X:1 Y:2\n   @@\n  End\n End\nEnd\n"),
    ("SetPlayerControlOfArmy", "LivingWorldCampaign C\n Act One\n  SetPlayerControlOfArmy\n   ArmyScriptingName = A\n   @@\n  End\n End\nEnd\n"),
    ("Scenario", "LivingWorldCampaign C\n Scenario\n  MinPlayers = 2\n  @@\n End\nEnd\n"),
    ("OwnershipSet", "LivingWorldCampaign C\n Scenario\n  OwnershipSet\n   StartRegion = R\n   @@\n  End\n End\nEnd\n"),
    ("SpawnBuildings", "LivingWorldCampaign C\n Scenario\n  OwnershipSet\n   SpawnBuildings\n    Region = R\n    @@\n   End\n  End\n End\nEnd\n"),
    ("SpawnArmies", "LivingWorldCampaign C\n Scenario\n  OwnershipSet\n   SpawnArmies\n    Region = R\n    @@\n   End\n  End\n End\nEnd\n"),
    ("StartingRestriction", "LivingWorldCampaign C\n Scenario\n  StartingRestriction\n   Teams = 1\n   @@\n  End\n End\nEnd\n"),
    ("Scenario PlayerDefeatCondition", "LivingWorldCampaign C\n Scenario\n  PlayerDefeatCondition\n   Teams = 1\n   @@\n  End\n End\nEnd\n"),
    ("Scenario TeamDefeatCondition", "LivingWorldCampaign C\n Scenario\n  TeamDefeatCondition\n   Teams = 1\n   @@\n  End\n End\nEnd\n"),
    ("Scenario TeamVictoryCondition", "LivingWorldCampaign C\n Scenario\n  TeamVictoryCondition\n   Teams = 1\n   @@\n  End\n End\nEnd\n"),
    ("LivingWorldVictoryType", "LivingWorldCampaign C\n LivingWorldVictoryType\n  DisplayGameType = G\n  @@\n End\nEnd\n"),
    ("VictoryType TeamVictoryCondition", "LivingWorldCampaign C\n LivingWorldVictoryType\n  TeamVictoryCondition\n   Teams = 1\n   @@\n  End\n End\nEnd\n"),
    ("Tutorial Turn", "LivingWorldCampaign C\n Tutorial\n  Turn 1\n   @@\n  End\n End\nEnd\n"),
    ("Tutorial Session", "LivingWorldCampaign C\n Tutorial\n  Turn 1\n   Phase Planning\n    Session\n     Audio = A\n     @@\n    End\n   End\n  End\n End\nEnd\n"),
    ("TaskAfterAudio", "LivingWorldCampaign C\n Tutorial\n  Turn 1\n   Phase Planning\n    Session\n     TaskAfterAudio\n      Task = EndPhase\n      @@\n     End\n    End\n   End\n  End\n End\nEnd\n"),
    ("LivingWorldRegionCampaign", "LivingWorldRegionCampaign R\n ArmyRetreatRounds = 8\n @@\nEnd\n"),
    ("Region", "LivingWorldRegionCampaign R\n Region A\n  MapName = M\n  @@\n End\nEnd\n"),
    ("Connection", "LivingWorldRegionCampaign R\n Region A\n  Connection\n   Region = B\n   @@\n  End\n End\nEnd\n"),
    ("RestrictBuildings", "LivingWorldRegionCampaign R\n Region A\n  RestrictBuildings\n   NumberAllowed = 1\n   @@\n  End\n End\nEnd\n"),
    ("ConcurrentRegionBonus", "LivingWorldRegionCampaign R\n ConcurrentRegionBonus B\n  Regions = A\n  @@\n End\nEnd\n"),
    ("LivingWorldRegionEffects", "LivingWorldRegionEffects E\n RegionObject = O\n @@\nEnd\n"),
    ("region effect", "LivingWorldRegionEffects E\n UnifiedEffect\n  Opacity = 1\n  @@\n End\nEnd\n"),
    ("ColorIntensityControlPoint", "LivingWorldRegionEffects E\n BordersEffect\n  ColorIntensityControlPoint\n   Intensity 1.0\n   @@\n  End\n End\nEnd\n"),
    ("effect ControlPoint", "LivingWorldRegionEffects E\n MouseoutEffectFlareup\n  ControlPoint\n   Angle = 1\n   @@\n  End\n End\nEnd\n"),
    ("StrategicHUD", "StrategicHUD\n @@\nEnd\n"),
    ("HUD element", "StrategicHUD\n Checklist\n  ItemVerticalSpacing = 1\n  @@\n End\nEnd\n"),
    ("HUD button", "StrategicHUD\n OptionsButton\n  Title = T\n  @@\n End\nEnd\n"),
    ("AutoResolveWeapon", "AutoResolveWeapon W\n MissPercentChance = 5\n @@\nEnd\n"),
    ("AutoResolveArmor", "AutoResolveArmor A\n Armor = AutoResolveUnit_Hero 100%\n @@\nEnd\n"),
    ("AutoResolveBody", "AutoResolveBody B\n CanBeAttacked = Yes\n @@\nEnd\n"),
    ("AutoResolveCombatChain", "AutoResolveCombatChain C\n Target = Target:AutoResolveUnit_Hero Priority:1\n @@\nEnd\n"),
    ("AutoResolveLeadership", "AutoResolveLeadership L\n AffectsHigherLevelFirst = Yes\n @@\nEnd\n"),
    ("BonusForLevel", "AutoResolveLeadership L\n BonusForLevel\n  MinLevel = 4\n  @@\n End\nEnd\n"),
    ("AutoResolveHandicapLevel", "AutoResolveHandicapLevel\n GUIDisplayedLevel = 5\n @@\nEnd\n"),
    ("ResourceBonus", "LivingWorldAutoResolveResourceBonus\n Sides = P\n @@\nEnd\n"),
    ("ResourceBonus Bonus", "LivingWorldAutoResolveResourceBonus\n Bonus\n  MinResourceBonus = 0\n  @@\n End\nEnd\n"),
    ("SciencePointBonus", "LivingWorldAutoResolveSciencePurchasePointBonus\n Sides = P\n @@\nEnd\n"),
    ("SciencePointBonus Bonus", "LivingWorldAutoResolveSciencePurchasePointBonus\n Bonus\n  MinSciencePurchasePointsForBonus = 0\n  @@\n End\nEnd\n"),
    ("Schedule", "AutoResolveReinforcementSchedule Attacker\n 1 = 0\n @@\nEnd\n"),
];

#[test]
fn an_unknown_field_in_any_block_is_one_error_and_the_block_without_it_is_clean() {
    for (what, fragment) in FRAGMENTS {
        let valid = fragment.replace("@@", "");
        let (_, errors) = read(&valid);
        assert!(errors.is_empty(), "{what} valid fragment: {errors:?}");
        let bogus = fragment.replace("@@", "Bogus = 1");
        let (_, errors) = read(&bogus);
        assert_eq!(errors.len(), 1, "{what}: {errors:?}");
        assert!(
            errors
                .first()
                .is_some_and(|e| e.to_string().contains("Bogus")),
            "{what}: {errors:?}"
        );
    }
}

#[test]
fn an_unbalanced_document_is_an_error() {
    let (_, errors) = read("LivingWorldAITemplate\n DesiredHeroRatio = 1\n");
    assert!(errors.iter().any(|e| matches!(e, LoadError::Structural(_))));
}

#[test]
fn the_reinforcement_schedule_reads_numbered_rounds_and_each_remaining() {
    let data = clean(
        "AutoResolveReinforcementSchedule Defender\n 1 = 0\n 2 = 15\n EachRemaining = +1\nEnd\n\
         AutoResolveReinforcementSchedule attacker\n 1 = 0\nEnd\n",
    );
    let defender = &data.reinforcement_schedules[0];
    assert_eq!(defender.side, ScheduleSide::Defender);
    assert_eq!(defender.rounds.get(&2), Some(&15));
    assert_eq!(defender.each_remaining, Some(1));
    assert_eq!(data.reinforcement_schedules[1].side, ScheduleSide::Attacker);
}

#[test]
fn the_reinforcement_schedule_refuses_what_the_catch_all_refuses() {
    for line in [
        "#1 = 0",
        "0 = 0",
        "x = 0",
        "-1 = 0",
        "2 = 15\n 2 = 20",
        "1 = soon",
    ] {
        let text = format!("AutoResolveReinforcementSchedule Attacker\n 1 = 0\n {line}\nEnd\n");
        let (_, errors) = read(&text);
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
    }
    let (_, errors) = read("AutoResolveReinforcementSchedule Foo\n 1 = 0\nEnd\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn connects_to_tokens_and_connection_blocks_fill_one_list_in_order() {
    let data = clean(
        "LivingWorldRegionCampaign R\n Region A\n  ConnectsTo = B C\n  Connection\n   Region = D\n   \
         DetourPoint = X:1 Y:2\n   DetourPoint = X:3 Y:4\n  End\n End\n Region B\n End\nEnd\n",
    );
    let region = data.region_campaigns.get("R").unwrap().region("A").unwrap();
    let connected: Vec<_> = region.connected().collect();
    assert_eq!(connected, ["B", "C", "D"]);
    assert_eq!(region.connections[2].detour_points.len(), 2);
    assert_eq!(region.connections[0].detour_points.len(), 0);
}

#[test]
fn an_empty_connects_to_followed_by_connection_blocks_appends_only_the_blocks() {
    let data = clean(
        "LivingWorldRegionCampaign R\n Region A\n  ConnectsTo =\n  Connection\n   Region = B\n  End\n End\nEnd\n",
    );
    let region = data.region_campaigns.get("R").unwrap().region("A").unwrap();
    assert_eq!(region.connections.len(), 1);
}

#[test]
fn strengthen_army_bonus_reads_labels_in_any_order_with_one_as_the_default() {
    let data = clean(
        "LivingWorldBuilding B\n BuildingNugget StrengthenArmy T\n  StrengtheningRange = THIS_TERRIORITY\n  \
         Bonus = 2 Armor:81% Weapon:10%\n  Bonus = 3 Experience:50%\n End\nEnd\n",
    );
    let BuildingNuggetKind::StrengthenArmy(nugget) =
        &data.buildings.get("B").unwrap().nuggets[0].kind
    else {
        panic!("not a StrengthenArmy nugget");
    };
    assert_eq!(
        nugget.bonuses[0],
        StrengthenBonus {
            level: 2,
            weapon: 0.1,
            armor: 0.81,
            experience: 1.0
        }
    );
    assert_eq!(nugget.bonuses[1].experience, 0.5);
}

#[test]
fn a_labelled_compound_value_refuses_an_unknown_label_or_a_repeat() {
    for bonus in [
        "2 Speed:1",
        "2 Armor:10% Armor:20%",
        "x Armor:10%",
        "2 Armor",
    ] {
        let text = format!(
            "LivingWorldBuilding B\n BuildingNugget StrengthenArmy T\n  Bonus = {bonus}\n End\nEnd\n"
        );
        let (_, errors) = read(&text);
        assert_eq!(errors.len(), 1, "{bonus}: {errors:?}");
    }
    let (_, errors) = read(
        "AutoResolveWeapon W\n DamagePerRound = Damage:5 Damage:6 Against:AutoResolveUnit_Hero\nEnd\n\
         AutoResolveWeapon V\n LevelBonus = Level:2 Bonus:10%\n LevelBonus = Level:2 Bonus:20%\nEnd\n",
    );
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn a_building_nugget_must_name_a_registered_type_and_a_tag_used_once() {
    let (_, errors) = read("LivingWorldBuilding B\n BuildingNugget Bogus T\n End\nEnd\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (_, errors) = read("LivingWorldBuilding B\n BuildingNugget IncreaseTreasury\n End\nEnd\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (_, errors) = read(
        "LivingWorldBuilding B\n BuildingNugget IncreaseTreasury T\n End\n BuildingNugget UpgradeTroops T\n End\nEnd\n",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn geometry_appends_and_a_region_effect_keeps_every_control_point() {
    let data = clean(
        "LivingWorldRegionEffects E\n UnifiedEffect\n  Geometry = A\n  Geometry B\n  ColorIntensityControlPoint\n   \
         Intensity 1.0\n   Time 0.5\n  End\n End\nEnd\n",
    );
    let effect = data
        .region_effects
        .get("E")
        .unwrap()
        .effect("UnifiedEffect")
        .unwrap();
    assert_eq!(effect.geometry, ["A", "B"]);
    assert_eq!(
        effect.color_intensity_control_points[0].time,
        Some(Millis(500))
    );
}

#[test]
fn a_repeated_region_effects_name_is_an_error() {
    let (_, errors) = read("LivingWorldRegionEffects E\nEnd\nLivingWorldRegionEffects E\nEnd\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_tutorial_task_outside_the_vocabulary_is_an_error() {
    let text = "LivingWorldCampaign C\n Tutorial\n  Turn 1\n   Phase P\n    Session\n     TaskAfterAudio\n      Task = Bogus\n     End\n    End\n   End\n  End\n End\nEnd\n";
    let (_, errors) = read(text);
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn compound_and_closed_list_values_refuse_what_retail_refuses() {
    for text in [
        "LivingWorldCampaign C\n Scenario\n  PlayerDefeatCondition\n   Teams = 0\n  End\n End\nEnd\n",
        "LivingWorldRegionCampaign R\n Region A\n  CenterPoint = X:1\n End\nEnd\n",
        "LivingWorldRegionCampaign R\n Region A\n  CenterPoint = Y:1 X:1\n End\nEnd\n",
        "LivingWorldRegionCampaign R\n Region A\n  CPLimit = many\n End\nEnd\n",
        "LivingWorldRegionCampaign R\n Region A\n  RestrictBuildings\n   Buildings = Castle\n  End\n End\nEnd\n",
        "LivingWorldBuilding B\n Type = Tower\nEnd\n",
        "LivingWorldBuilding B\n CanDefendTerritory = maybe\nEnd\n",
    ] {
        let (_, errors) = read(text);
        assert_eq!(errors.len(), 1, "{text}: {errors:?}");
    }
}

#[test]
fn values_read_through_the_primitives() {
    let mut macros = MacroTable::new();
    let defines = Document::parse(
        "d.ini",
        "#define BASE 100\n#define BOOSTED #ADD( BASE 25 )\n#define TWICE #MULTIPLY( BOOSTED 2 )\n",
        &BlockTable::corpus_path(),
    )
    .unwrap();
    macros.insert_defines(defines.defines());
    let (data, errors) = read_with(
        "AutoResolveBody Hero\n HitpointsAtLevel = Hitpoints:TWICE Level:1\n HitpointsAtLevel = Hitpoints:12.5 Level:2\n \
         RegenerateAtLevel = Regenerate:3 Level:1\nEnd\n\
         AutoResolveHandicapLevel\n GUIDisplayedLevel = 5\n ArmorMultiplier = #DIVIDE( 1.0, 0.5 )\n WeaponMultiplier = BASE\nEnd\n\
         AutoResolveArmor A\n Armor = AutoResolveUnit_Hero 72.9%\nEnd\n\
         AutoResolveLeadership L\n Affects = AutoResolveUnit_Archer AutoResolveUnit_Hero\n BonusForLevel\n  WeaponMultiplier = 150%\n  MinLevel = 4\n End\nEnd\n\
         LivingWorldMapInfo\n Pos = 1\nEnd\n",
        &macros,
    );
    assert_eq!(
        errors.len(),
        1,
        "only the unknown MapInfo field: {errors:?}"
    );
    let body = data.auto_resolve_bodies.get("Hero").unwrap();
    assert_eq!(body.hitpoints_at_level[0].amount, 250.0);
    assert_eq!(body.hitpoints_at_level[1].amount, 12.5);
    assert_eq!(body.regenerate_at_level[0].amount, 3.0);
    let handicap = &data.auto_resolve_handicaps[0];
    assert_eq!(handicap.armor_multiplier, Some(2.0));
    assert_eq!(handicap.weapon_multiplier, Some(100.0));
    let armor = data.auto_resolve_armors.get("A").unwrap();
    assert_eq!(armor.armor[0].against, UnitType::Hero);
    assert!((armor.armor[0].taken - 0.729).abs() < 1e-6);
    let leadership = data.auto_resolve_leaderships.get("L").unwrap();
    assert_eq!(leadership.affects.0, [UnitType::Archer, UnitType::Hero]);
    assert_eq!(leadership.bonuses[0].weapon_multiplier, Some(Percent(1.5)));
}

#[test]
fn a_scenario_reads_its_ownership_sets_conditions_and_one_based_teams() {
    let data = clean(
        "LivingWorldCampaign C\n IsEvilCampaign = No\n Scenario\n  MinPlayers = 2\n  DisableRegions = A B\n  \
         HistoricalScenario = Yes\n  OwnershipSet\n   Regions = A B\n   StartRegion = A\n   SpawnBuildings\n    \
         Buildings = X Y\n    Region = A\n   End\n   SpawnArmies\n    Armies = H1\n    Region = B\n   End\n  End\n  \
         StartingRestriction\n   Factions = FactionMen\n   Regions = A\n   Teams = 1\n  End\n  \
         TeamVictoryCondition\n   Teams = 1 2\n   ControlledRegions = A B\n  End\n End\n\
         LivingWorldVictoryType\n DisplayGameType = G\nEnd\nLivingWorldVictoryType\n DisplayGameType = H\nEnd\nEnd\n",
    );
    let campaign = data.campaigns.get("C").unwrap();
    let scenario = &campaign.scenarios[0];
    assert_eq!(scenario.disable_regions.0, ["A", "B"]);
    assert_eq!(
        scenario.ownership_sets[0].spawn_buildings[0].buildings.0,
        ["X", "Y"]
    );
    assert_eq!(
        scenario.ownership_sets[0].spawn_armies[0].region.as_deref(),
        Some("B")
    );
    assert_eq!(scenario.starting_restrictions[0].teams.0, [0]);
    assert_eq!(scenario.conditions.team_victory[0].teams.0, [0, 1]);
    assert_eq!(campaign.victory_types.len(), 2);
    assert_eq!(
        campaign.victory_types[1].display_game_type.as_deref(),
        Some("H")
    );
}

#[test]
fn an_act_keeps_its_nuggets_in_order() {
    let data = clean(
        "LivingWorldCampaign C\n Act One\n  SpawnArmy\n   ScriptingName = A\n   Position = X:1 Y:2\n  End\n  \
         MoveCamera\n   ScrollTime = 1.5\n   Position = X:1 Y:2 Z:3\n  End\n  JumpToAct = Two\n  \
         CallActSubroutine = S1\n  CallActSubroutine = S2\n  EyeTowerPoints\n   LookPoint = X:1 Y:2\n   \
         LookPoint = X:3 Y:4\n  End\n End\nEnd\n",
    );
    let act = &data.campaigns.get("C").unwrap().acts[0];
    assert_eq!(act.name, "One");
    assert!(matches!(act.nuggets[0], ActNugget::SpawnArmy(_)));
    let ActNugget::MoveCamera(camera) = &act.nuggets[1] else {
        panic!("not a MoveCamera nugget");
    };
    assert_eq!(camera.scroll_time, Some(Millis(1500)));
    let ActNugget::EyeTowerPoints(points) = &act.nuggets[2] else {
        panic!("not an EyeTowerPoints nugget");
    };
    assert_eq!(points.look_point.len(), 2);
    assert_eq!(act.call_act_subroutine.0, ["S1", "S2"]);
    assert_eq!(act.jump_to_act.as_deref(), Some("Two"));
}

#[test]
fn a_hud_element_written_twice_adds_to_the_first() {
    let data = clean(
        "StrategicHUD\n Checklist\n  ItemFont = \"Albertus MT\" 12 No\n  ItemNormalColor = R:255 G:255 B:0 A:100\n End\n \
         Checklist\n  ItemVerticalSpacing = 2\n  ItemNewColor = R:1 G:2 B:3\n End\nEnd\n",
    );
    let checklist = data
        .strategic_hud
        .as_ref()
        .unwrap()
        .checklist
        .as_ref()
        .unwrap();
    assert_eq!(checklist.item_font.as_ref().unwrap().size, 12);
    assert_eq!(checklist.item_normal_color.unwrap().a, 100);
    assert_eq!(checklist.item_new_color.unwrap().a, 255);
    assert_eq!(checklist.item_vertical_spacing, Some(2.0));
}

#[test]
fn every_declared_player_template_field_is_read() {
    let text = "LivingWorldPlayerTemplate PlayerMen\n Faction = FactionMen\n FactionIcon = I\n DefaultArmyIconName = A\n \
         BuildPlotIconName = B\n BuildPlotSelectionPortraitName = C\n GarrisonSelectionPortraitName = D\n \
         GarrisonDisplayNameTag = E\n FactionDozerTemplateName = Z\n FactionInnUnitTemplateName = Y\n \
         StartingWorldCP = 1\n StartingTacticalCP = 2\n StartingHeroCP = 3\n MaxWorldCP = 4\n MaxTacticalCP = 5\n \
         MaxHeroCP = 6\n ScenarioStartResources = 7\n ScenarioMaxResources = 8\n Music = M\n AutoResolveLoop = L\nEnd\n";
    let data = clean(text);
    let template = data.player_templates.get("playermen").unwrap();
    assert_eq!(template.faction.as_deref(), Some("FactionMen"));
    assert_eq!(template.scenario_max_resources, Some(8));
    assert_eq!(template.auto_resolve_loop.as_deref(), Some("L"));
    assert_eq!(template.starting_tactical_cp, Some(2));
}

#[test]
fn a_banner_type_keeps_its_flag_glow_and_icon() {
    let data = clean(
        "BannerType BannerMen\n FlagObj = Gondor_Enabl\n ;GlowObj = Banner_Glow01\n Icon = Banner_Men\nEnd\n\
         BannerType BannerWild\n FlagObj = Wild_Enabled\n GlowObj = G\n Icon = Banner_Wild\nEnd\n",
    );
    let men = data.banner_types.get("bannermen").unwrap();
    assert_eq!(men.icon.as_deref(), Some("Banner_Men"));
    assert_eq!(men.flag_obj.as_deref(), Some("Gondor_Enabl"));
    assert_eq!(men.glow_obj, None);
    let wild = data.banner_types.get("bannerwild").unwrap();
    assert_eq!(wild.glow_obj.as_deref(), Some("G"));
    assert_eq!(wild.icon.as_deref(), Some("Banner_Wild"));
}

#[test]
fn spawn_armies_lists_the_act_nuggets_and_the_buildings_armies_to_spawn() {
    let data = clean(
        "LivingWorldCampaign C\n Act One\n  SpawnArmy\n   ScriptingName = A\n   HeroTemplateName = HeroA\n   Banner = BannerElves\n  End\n End\nEnd\n\
         LivingWorldBuilding B\n BuildingNugget SpawnArmy Q\n  ArmyToSpawn\n   HeroTemplateName = HeroB\n   Banner = BannerMordor\n  End\n End\nEnd\n",
    );
    let banners: Vec<(Option<&str>, Option<&str>)> = data
        .spawn_armies()
        .iter()
        .map(|a| (a.hero_template_name.as_deref(), a.banner.as_deref()))
        .collect();
    assert_eq!(
        banners,
        vec![
            (Some("HeroA"), Some("BannerElves")),
            (Some("HeroB"), Some("BannerMordor"))
        ]
    );
}

#[test]
fn every_reference_resolves_or_is_reported_by_block_and_field() {
    let data = clean(
        "LivingWorldRegionCampaign R\n Region A\n  MapName = \"MAP A\"\n  ConnectsTo = B Gone\n End\n Region B\n  \
         MapName = \"MAP B\"\n  ConnectsTo = A\n End\nEnd\n\
         LivingWorldPlayerArmy\n Name = Army\n ArmyEntry\n  ThingTemplate = Hero\n End\n ArmyEntry\n  ThingTemplate = Nobody\n End\nEnd\n\
         LivingWorldPlayerTemplate PlayerMen\n Faction = FactionMen\nEnd\n\
         LivingWorldBuilding Fort\n AvailableTo = PlayerMen PlayerGhost\n BattleThingTemplate = Hero\nEnd\n\
         LivingWorldCampaign C\n Scenario\n  RegionCampaign = R\n  DisableRegions = A Nowhere\n  OwnershipSet\n   \
         SpawnBuildings\n    Buildings = Fort Moat\n    Region = B\n   End\n   SpawnArmies\n    Armies = H1\n   End\n  End\n End\n \
         Act One\n  SpawnArmy\n   ScriptingName = H1\n   PlayerArmy = Army\n  End\n End\nEnd\n",
    );
    let objects: std::collections::BTreeSet<String> = ["hero".to_owned()].into();
    let factions: std::collections::BTreeSet<String> = ["factionmen".to_owned()].into();
    let maps: std::collections::BTreeSet<String> = ["map a".to_owned()].into();
    let missing = resolve(
        &data,
        &ExternalNames {
            objects: &objects,
            player_templates: &factions,
            maps: Some(&maps),
        },
    );
    let report: Vec<(&str, &str, &str)> = missing
        .iter()
        .map(|m| (m.block, m.field, m.missing.as_str()))
        .collect();
    assert_eq!(
        report,
        [
            ("Region", "Connection", "Gone"),
            ("Region", "MapName", "MAP B"),
            ("LivingWorldPlayerArmy", "ArmyEntry/ThingTemplate", "Nobody"),
            ("LivingWorldBuilding", "AvailableTo", "PlayerGhost"),
            ("LivingWorldCampaign", "Scenario/DisableRegions", "Nowhere"),
            ("LivingWorldCampaign", "SpawnBuildings/Buildings", "Moat"),
        ]
    );
    let graph = data.region_campaigns.get("R").unwrap().graph();
    assert_eq!((graph.regions, graph.edges, graph.asymmetric), (2, 3, 1));
}

fn sources(files: &[(&str, &str)]) -> MemorySources {
    let mut sources = MemorySources::new();
    for (path, text) in files {
        sources.insert(path, text.as_bytes().to_vec());
    }
    sources
}

#[test]
fn the_loader_reads_the_files_the_legend_names_and_no_others() {
    let sources = sources(&[
        (
            "data/ini/default/subsystemlegend.ini",
            "LoadSubsystem TheLivingWorldPlayerTemplateStore\n  Loader = INI\n  InitFile = Data\\INI\\LivingWorldPlayers.ini\nEnd\n\
             LoadSubsystem TheLivingWorldCampaignManager\n  Loader = INI\n  InitFile = Data\\INI\\Campaigns\\Risk.ini\nEnd\n\
             LoadSubsystem TheThingFactory\n  Loader = INI\n  InitPath = Data\\INI\\Object\nEnd\n\
             LoadSubsystem ThePlayerTemplateStore\n  Loader = INI\n  InitFile = Data\\INI\\Default\\PlayerTemplate.ini\nEnd\n",
        ),
        (
            "data/ini/livingworldplayers.ini",
            "#define CASH 7\nLivingWorldPlayerTemplate PlayerMen\n Faction = FactionMen\n ScenarioStartResources = CASH\nEnd\n",
        ),
        (
            "data/ini/campaigns/risk.ini",
            "#include \"extra.inc\"\n;#include \"skipped.inc\"\nLivingWorldCampaign C\n IsEvilCampaign = No\nEnd\n",
        ),
        (
            "data/ini/campaigns/extra.inc",
            "LivingWorldPlayerArmy\n Name = FromInclude\nEnd\n",
        ),
        (
            "data/ini/campaigns/skipped.inc",
            "LivingWorldPlayerArmy\n Name = Skipped\nEnd\n",
        ),
        (
            "data/ini/livingworldregions.ini",
            "LivingWorldRegionCampaign NotLoaded\nEnd\n",
        ),
        (
            "data/ini/object/a.ini",
            "Object Hero\nEnd\nChildObject Kid Hero\nEnd\n",
        ),
        (
            "data/ini/default/playertemplate.ini",
            "PlayerTemplate FactionMen\nEnd\n",
        ),
    ]);
    let load = load_living_world(&sources);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    assert_eq!(load.data.player_templates.len(), 1);
    assert_eq!(
        load.data
            .player_templates
            .get("PlayerMen")
            .unwrap()
            .scenario_start_resources,
        Some(7)
    );
    assert!(load.data.armies.contains("FromInclude"));
    assert!(!load.data.armies.contains("Skipped"));
    assert_eq!(load.data.region_campaigns.len(), 0);
    assert!(load.object_names.contains("hero") && load.object_names.contains("kid"));
    assert!(load.player_template_names.contains("factionmen"));
}

#[test]
fn a_cut_short_document_never_panics() {
    let text = FRAGMENTS
        .iter()
        .map(|(_, fragment)| fragment.replace("@@", ""))
        .collect::<String>();
    let lines: Vec<&str> = text.lines().collect();
    for cut in 0..lines.len() {
        let prefix = lines[..cut].join("\n");
        let _ = read(&prefix);
    }
}
