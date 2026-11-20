use crate::tree::{Block, Node};

use super::grammar::{
    fields, index_enum, integer, label, labelled, labels_in_any_order, percent, walk, Coord2, Cx,
    Defined, Rgb, TokenList, Tokens,
};

fields! {
    pub struct PlayerTemplateDef {
        faction: Option<String> = "Faction",
        faction_icon: Option<String> = "FactionIcon",
        default_army_icon_name: Option<String> = "DefaultArmyIconName",
        build_plot_icon_name: Option<String> = "BuildPlotIconName",
        build_plot_selection_portrait_name: Option<String> = "BuildPlotSelectionPortraitName",
        garrison_selection_portrait_name: Option<String> = "GarrisonSelectionPortraitName",
        garrison_display_name_tag: Option<String> = "GarrisonDisplayNameTag",
        faction_dozer_template_name: Option<String> = "FactionDozerTemplateName",
        faction_inn_unit_template_name: Option<String> = "FactionInnUnitTemplateName",
        starting_world_cp: Option<i32> = "StartingWorldCP",
        starting_tactical_cp: Option<i32> = "StartingTacticalCP",
        starting_hero_cp: Option<i32> = "StartingHeroCP",
        max_world_cp: Option<i32> = "MaxWorldCP",
        max_tactical_cp: Option<i32> = "MaxTacticalCP",
        max_hero_cp: Option<i32> = "MaxHeroCP",
        scenario_start_resources: Option<i32> = "ScenarioStartResources",
        scenario_max_resources: Option<i32> = "ScenarioMaxResources",
        music: Option<String> = "Music",
        auto_resolve_loop: Option<String> = "AutoResolveLoop",
    }
}

impl Defined for PlayerTemplateDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

fn read_plain<T: Default>(
    block: &Block,
    cx: &Cx,
    mut field: impl FnMut(&mut T, &crate::tree::Field, &str) -> bool,
) -> T {
    let ctx = label(block);
    let mut out = T::default();
    walk(block, cx, &ctx, |node| match node {
        Node::Field(f) => field(&mut out, f, &ctx),
        _ => false,
    });
    out
}

fn header_name(block: &Block, cx: &Cx) -> String {
    if block.name().is_none() {
        cx.missing_name(block);
    }
    block.name().unwrap_or_default().to_owned()
}

impl PlayerTemplateDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let mut out = read_plain::<Self>(block, cx, |out, f, ctx| out.read_field(f, cx, ctx));
        out.name = header_name(block, cx);
        out
    }
}

fields! {
    pub struct AiTemplateDef {
        desired_soldier_ratio: Option<i32> = "DesiredSoldierRatio",
        desired_archer_ratio: Option<i32> = "DesiredArcherRatio",
        desired_pikemen_ratio: Option<i32> = "DesiredPikemenRatio",
        desired_cavalry_ratio: Option<i32> = "DesiredCavalryRatio",
        desired_monster_ratio: Option<i32> = "DesiredMonsterRatio",
        desired_hero_ratio: Option<i32> = "DesiredHeroRatio",
        desired_fortress_ratio: Option<i32> = "DesiredFortressRatio",
        desired_siege_ratio: Option<i32> = "DesiredSiegeRatio",
        desired_support_ratio: Option<i32> = "DesiredSupportRatio",
        building_score_armory: Option<i32> = "BuildingScoreArmory",
        building_score_barracks: Option<i32> = "BuildingScoreBarracks",
        building_score_castle: Option<i32> = "BuildingScoreCastle",
        building_score_farm: Option<i32> = "BuildingScoreFarm",
        bonus_preference_resource: Option<i32> = "BonusPreferenceResource",
        bonus_preference_army: Option<i32> = "BonusPreferenceArmy",
        bonus_preference_legendary: Option<i32> = "BonusPreferenceLegendary",
        bonus_preference_attack: Option<i32> = "BonusPreferenceAttack",
        bonus_preference_defense: Option<i32> = "BonusPreferenceDefense",
        bonus_preference_experience: Option<i32> = "BonusPreferenceExperience",
        bonus_preference_treasury: Option<i32> = "BonusPreferenceTreasury",
    }
}

impl Defined for AiTemplateDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl AiTemplateDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let mut out = read_plain::<Self>(block, cx, |out, f, ctx| out.read_field(f, cx, ctx));
        out.name = block.name().unwrap_or_default().to_owned();
        out
    }
}

fields! {
    pub struct ArmyEntryDef {
        default: Option<bool> = "Default",
        thing_template: Option<String> = "ThingTemplate",
        quantity: Option<i32> = "Quantity",
    }
}

fields! {
    pub struct PlayerArmyDef {
        army_name: Option<String> = "Name",
        display_name_tag: Option<String> = "DisplayNameTag",
        color: Option<Rgb> = "Color",
        night_color: Option<Rgb> = "NightColor",
        survival_threshhold: Option<i32> = "SurvivalThreshhold",
    }
    with {
        pub entries: Vec<ArmyEntryDef>,
    }
}

impl Defined for PlayerArmyDef {
    fn def_name(&self) -> &str {
        self.army_name.as_deref().unwrap_or_default()
    }
}

impl PlayerArmyDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self::default();
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "ArmyEntry" => {
                let mut entry =
                    read_plain::<ArmyEntryDef>(child, cx, |e, f, c| e.read_field(f, cx, c));
                entry.name = child.name().unwrap_or_default().to_owned();
                out.entries.push(entry);
                true
            }
            _ => false,
        });
        if out.army_name.is_none() {
            cx.errors.at(
                &block.file,
                block.line,
                &ctx,
                "Name",
                "",
                "present in a LivingWorldPlayerArmy",
            );
        }
        out
    }
}

fields! {
    pub struct SpawnArmyDef {
        icon: Option<String> = "Icon",
        banner: Option<String> = "Banner",
        initial_region: Option<String> = "InitialRegion",
        player_army: Option<String> = "PlayerArmy",
        palantir_movie: Option<String> = "PalantirMovie",
        icon_size: Option<String> = "IconSize",
        scripting_name: Option<String> = "ScriptingName",
        tooltip_string_tag: Option<String> = "TooltipStringTag",
        hero_template_name: Option<String> = "HeroTemplateName",
        construct_button_image: Option<String> = "ConstructButtonImage",
        construct_button_title: Option<String> = "ConstructButtonTitle",
        construct_button_help: Option<String> = "ConstructButtonHelp",
        position: Option<Coord2> = "Position",
        spawn_for_templates: Tokens = "SpawnForTemplates",
        is_city: Option<bool> = "IsCity",
        spawn_at_act_start: Option<bool> = "SpawnAtActStart",
        move_speed: Option<f32> = "MoveSpeed",
        build_time: Option<i32> = "BuildTime",
    }
}

impl SpawnArmyDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let mut out = read_plain::<Self>(block, cx, |out, f, ctx| out.read_field(f, cx, ctx));
        out.name = block.name().unwrap_or_default().to_owned();
        out
    }
}

index_enum! {
    pub enum BuildingType ("building type") {
        None = "NONE",
        Fortress = "FORTRESS",
        Armory = "ARMORY",
        Resource = "RESOURCE",
        Barracks = "BARRACKS",
    }
}

index_enum! {
    pub enum CommandPointKind ("command-point kind") {
        Tactical = "TACTICAL",
        World = "WORLD",
    }
}

index_enum! {
    pub enum StrengtheningRange ("strengthening range") {
        ThisTerritory = "THIS_TERRIORITY",
        AllArmies = "ALL_ARMIES",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncreaseCommandPoints {
    pub kind: Option<CommandPointKind>,
    pub amount: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthenBonus {
    pub level: i32,
    pub weapon: f32,
    pub armor: f32,
    pub experience: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StrengthenArmy {
    pub range: Option<StrengtheningRange>,
    pub bonus_key: Option<String>,
    pub bonuses: Vec<StrengthenBonus>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeTroops {
    pub num_upgrades_per_turn: Option<i32>,
    pub upgradeable_units: TokenList,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpawnArmyNugget {
    pub queue_size: Option<i32>,
    pub armies: Vec<SpawnArmyDef>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncreaseTreasury {
    pub treasure_amount: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildingNuggetKind {
    IncreaseCommandPoints(IncreaseCommandPoints),
    SpawnArmy(SpawnArmyNugget),
    StrengthenArmy(StrengthenArmy),
    UpgradeTroops(UpgradeTroops),
    IncreaseTreasury(IncreaseTreasury),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildingNugget {
    pub tag: String,
    pub kind: BuildingNuggetKind,
}

fn strengthen_bonus(args: &[String], cx: &Cx) -> Option<StrengthenBonus> {
    let (level, rest) = args.split_first()?;
    let parts = labels_in_any_order(&labelled(rest)?, &["Weapon", "Armor", "Experience"])?;
    let scalar = |at: usize| match parts.get(at)? {
        Some(token) => percent(token, cx.macros),
        None => Some(1.0),
    };
    Some(StrengthenBonus {
        level: integer(level, cx.macros)?,
        weapon: scalar(0)?,
        armor: scalar(1)?,
        experience: scalar(2)?,
    })
}

fn read_nugget(block: &Block, cx: &Cx) -> Option<BuildingNugget> {
    let ctx = label(block);
    let (Some(kind), Some(tag)) = (block.args.first(), block.args.get(1)) else {
        cx.block_error(&ctx, block, "`BuildingNugget <Type> <Tag>`");
        return None;
    };
    let kind = match kind.as_str() {
        "IncreaseCommandPoints" => {
            let mut out = IncreaseCommandPoints::default();
            walk(block, cx, &ctx, |node| {
                let Node::Field(f) = node else { return false };
                match f.keyword.as_str() {
                    "Type" => super::grammar::Slot::read(&mut out.kind, f, cx, &ctx),
                    "Amount" => super::grammar::Slot::read(&mut out.amount, f, cx, &ctx),
                    _ => return false,
                }
                true
            });
            BuildingNuggetKind::IncreaseCommandPoints(out)
        }
        "SpawnArmy" => {
            let mut out = SpawnArmyNugget::default();
            walk(block, cx, &ctx, |node| match node {
                Node::Field(f) if f.keyword == "QueueSize" => {
                    super::grammar::Slot::read(&mut out.queue_size, f, cx, &ctx);
                    true
                }
                Node::Block(child) if child.keyword == "ArmyToSpawn" => {
                    out.armies.push(SpawnArmyDef::read(child, cx));
                    true
                }
                _ => false,
            });
            BuildingNuggetKind::SpawnArmy(out)
        }
        "StrengthenArmy" => {
            let mut out = StrengthenArmy::default();
            walk(block, cx, &ctx, |node| {
                let Node::Field(f) = node else { return false };
                match f.keyword.as_str() {
                    "StrengtheningRange" => super::grammar::Slot::read(&mut out.range, f, cx, &ctx),
                    "BonusKey" => super::grammar::Slot::read(&mut out.bonus_key, f, cx, &ctx),
                    "Bonus" => match strengthen_bonus(&f.args, cx) {
                        Some(b) if out.bonuses.iter().all(|e| e.level != b.level) => {
                            out.bonuses.push(b);
                        }
                        _ => cx.errors.field(
                            &ctx,
                            f,
                            "<level> [Weapon:<pct>] [Armor:<pct>] [Experience:<pct>], each level once",
                        ),
                    },
                    _ => return false,
                }
                true
            });
            BuildingNuggetKind::StrengthenArmy(out)
        }
        "UpgradeTroops" => {
            let mut out = UpgradeTroops::default();
            walk(block, cx, &ctx, |node| {
                let Node::Field(f) = node else { return false };
                match f.keyword.as_str() {
                    "NumUpgradesPerTurn" => {
                        super::grammar::Slot::read(&mut out.num_upgrades_per_turn, f, cx, &ctx);
                    }
                    "UpgradeableUnits" => {
                        super::grammar::Slot::read(&mut out.upgradeable_units, f, cx, &ctx);
                    }
                    _ => return false,
                }
                true
            });
            BuildingNuggetKind::UpgradeTroops(out)
        }
        "IncreaseTreasury" => {
            let mut out = IncreaseTreasury::default();
            walk(block, cx, &ctx, |node| {
                let Node::Field(f) = node else { return false };
                if f.keyword != "TreasureAmount" {
                    return false;
                }
                super::grammar::Slot::read(&mut out.treasure_amount, f, cx, &ctx);
                true
            });
            BuildingNuggetKind::IncreaseTreasury(out)
        }
        _ => {
            cx.block_error(&ctx, block, "a registered BuildingNugget type");
            return None;
        }
    };
    Some(BuildingNugget {
        tag: tag.clone(),
        kind,
    })
}

fields! {
    pub struct BuildingDef {
        battle_thing_template: Option<String> = "BattleThingTemplate",
        building_icon: Option<String> = "BuildingIcon",
        construct_button_image: Option<String> = "ConstructButtonImage",
        construct_button_title: Option<String> = "ConstructButtonTitle",
        construct_button_help: Option<String> = "ConstructButtonHelp",
        display_name_tag: Option<String> = "DisplayNameTag",
        display_description_tag: Option<String> = "DisplayDescriptionTag",
        turns_to_build: Option<i32> = "TurnsToBuild",
        strategic_resource_cost: Option<i32> = "StrategicResourceCost",
        can_defend_territory: Option<bool> = "CanDefendTerritory",
        create_unit_during_auto_resolve: Option<bool> = "CreateUnitDuringAutoResolve",
        building_type: Option<BuildingType> = "Type",
        available_to: Tokens = "AvailableTo",
    }
    with {
        pub nuggets: Vec<BuildingNugget>,
    }
}

impl Defined for BuildingDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl BuildingDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: header_name(block, cx),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "BuildingNugget" => {
                if let Some(nugget) = read_nugget(child, cx) {
                    if out.nuggets.iter().any(|held| held.tag == nugget.tag) {
                        cx.block_error(&ctx, child, "a nugget tag not used twice in one building");
                    } else {
                        out.nuggets.push(nugget);
                    }
                }
                true
            }
            _ => false,
        });
        out
    }
}
