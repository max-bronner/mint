use crate::tree::{Block, Node};

use super::grammar::{
    fields, label, labelled, labels_in_any_order, number, walk, Coord2, Cx, Defined, EnumList,
    TokenList, Tokens,
};
use super::players::BuildingType;

fields! {
    pub struct ConnectionDef {
        region: Option<String> = "Region",
        detour_points: Vec<Coord2> = "DetourPoint",
    }
}

fields! {
    pub struct RestrictBuildingsDef {
        buildings: EnumList<BuildingType> = "Buildings",
        number_allowed: Option<i32> = "NumberAllowed",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldToMap {
    pub world: f32,
    pub rts: f32,
}

fields! {
    pub struct RegionDef {
        map_name: Option<String> = "MapName",
        display_name: Option<String> = "DisplayName",
        display_description: Option<String> = "DisplayDescription",
        conquered_notice: Option<String> = "ConqueredNotice",
        movie_name_first_time: Option<String> = "MovieNameFirstTime",
        movie_name_repeat: Option<String> = "MovieNameRepeat",
        skirmish_still_image: Option<String> = "SkirmishStillImage",
        sub_object: Option<String> = "SubObject",
        on_win_act_subroutine: Option<String> = "OnWinActSubroutine",
        region_portrait: Option<String> = "RegionPortrait",
        difficulty_text: Option<String> = "DifficultyText",
        owned_by_player: Option<String> = "OwnedByPlayer",
        fortress_portrait: Option<String> = "FortressPortrait",
        fortress_display_name: Option<String> = "FortressDisplayName",
        fortress_display_description: Option<String> = "FortressDisplayDescription",
        skirmish_voice_track: Option<String> = "SkirmishVoiceTrack",
        skirmish_music_track: Option<String> = "SkirmishMusicTrack",
        strategic_value: Option<i32> = "StrategicValue",
        army_bonus: Option<i32> = "ArmyBonus",
        legendary_bonus: Option<i32> = "LegendaryBonus",
        resource_bonus: Option<i32> = "ResourceBonus",
        attack_bonus: Option<i32> = "AttackBonus",
        defense_bonus: Option<i32> = "DefenseBonus",
        experience_bonus: Option<i32> = "ExperienceBonus",
        fertile_territory_bonus: Option<i32> = "FertileTerritoryBonus",
        display_act_num: Option<i32> = "DisplayActNum",
        cp_limit: Option<i32> = "CPLimit",
        ally_cp_limit: Option<i32> = "AllyCPLimit",
        unpack_camps: Option<bool> = "UnpackCamps",
        custom_center_point: Option<bool> = "CustomCenterPoint",
        end_of_campaign: Option<bool> = "EndOfCampaign",
        map_is_campaign_map: Option<bool> = "MapIsCampaignMap",
        lose_campaign_if_lose_map: Option<bool> = "LoseCampaignIfLoseMap",
        custom_ui_popup_point: Option<bool> = "CustomUIPopupPoint",
        create_auto_fort: Option<bool> = "CreateAutoFort",
        center_point: Option<Coord2> = "CenterPoint",
        ui_popup_point: Option<Coord2> = "UIPopupPoint",
        army_placement_pos: Vec<Coord2> = "ArmyPlacementPos",
        hero_army_spot: Vec<Coord2> = "HeroArmySpot",
        garrison_army_spot: Vec<Coord2> = "GarrisonArmySpot",
        building_spot: Vec<Coord2> = "BuildingSpot",
        delay_carryover_spawning_of: TokenList = "DelayCarryoverSpawningOf",
        on_enter_give_upgrade: TokenList = "OnEnterGiveUpgrade",
        on_victory_remove_upgrade: TokenList = "OnVictoryRemoveUpgrade",
        on_victory_give_upgrade: TokenList = "OnVictoryGiveUpgrade",
        region_bonus: Tokens = "RegionBonus",
    }
    with {
        pub map_match_player_world_to_map: Vec<WorldToMap>,
        pub connections: Vec<ConnectionDef>,
        pub restrict_buildings: Vec<RestrictBuildingsDef>,
    }
}

impl Defined for RegionDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl RegionDef {
    pub fn connected(&self) -> impl Iterator<Item = &str> {
        self.connections
            .iter()
            .filter_map(|connection| connection.region.as_deref())
    }

    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => match field.keyword.as_str() {
                "ConnectsTo" => {
                    for token in &field.args {
                        out.connections.push(ConnectionDef {
                            region: Some(token.clone()),
                            ..ConnectionDef::default()
                        });
                    }
                    true
                }
                "MapMatchPlayerWorldToMap" => {
                    let pair = labelled(&field.args)
                        .and_then(|pairs| labels_in_any_order(&pairs, &["World", "RTS"]))
                        .and_then(|parts| match parts.as_slice() {
                            [Some(world), Some(rts)] => Some(WorldToMap {
                                world: number(world, cx.macros)?,
                                rts: number(rts, cx.macros)?,
                            }),
                            _ => None,
                        });
                    match pair {
                        Some(pair) => out.map_match_player_world_to_map.push(pair),
                        None => cx.errors.field(&ctx, field, "World:<real> RTS:<real>"),
                    }
                    true
                }
                _ => out.read_field(field, cx, &ctx),
            },
            Node::Block(child) if child.keyword == "Connection" => {
                let inner = format!("{ctx}/Connection");
                let mut connection = ConnectionDef::default();
                walk(child, cx, &inner, |n| match n {
                    Node::Field(f) => connection.read_field(f, cx, &inner),
                    _ => false,
                });
                out.connections.push(connection);
                true
            }
            Node::Block(child) if child.keyword == "RestrictBuildings" => {
                let inner = format!("{ctx}/RestrictBuildings");
                let mut restrict = RestrictBuildingsDef::default();
                walk(child, cx, &inner, |n| match n {
                    Node::Field(f) => restrict.read_field(f, cx, &inner),
                    _ => false,
                });
                out.restrict_buildings.push(restrict);
                true
            }
            _ => false,
        });
        out
    }
}

fields! {
    pub struct ConcurrentRegionBonusDef {
        territory: Option<String> = "Territory",
        effect_name: Option<String> = "EffectName",
        regions: Tokens = "Regions",
        unified_eva_event: Option<String> = "UnifiedEvaEvent",
        lost_eva_event: Option<String> = "LostEvaEvent",
        look_at_center: Option<Coord2> = "LookAtCenter",
        look_at_heading: Option<f32> = "LookAtHeading",
        look_at_zoom: Option<f32> = "LookAtZoom",
        legendary_bonus: Option<i32> = "LegendaryBonus",
        resource_bonus: Option<i32> = "ResourceBonus",
        army_bonus: Option<i32> = "ArmyBonus",
        attack_bonus: Option<i32> = "AttackBonus",
        defense_bonus: Option<i32> = "DefenseBonus",
        experience_bonus: Option<i32> = "ExperienceBonus",
        extra_start_resources_bonus: Option<i32> = "ExtraStartResourcesBonus",
        building_discount_bonus: Option<i32> = "BuildingDiscountBonus",
        free_inn_units_bonus: Option<i32> = "FreeInnUnitsBonus",
        discounted_seige_units_bonus: Option<i32> = "DiscountedSeigeUnitsBonus",
        discounted_hero_units_bonus: Option<i32> = "DiscountedHeroUnitsBonus",
        free_builder_bonus: Option<i32> = "FreeBuilderBonus",
        discounted_barracks_units_bonus: Option<i32> = "DiscountedBarracksUnitsBonus",
    }
}

impl ConcurrentRegionBonusDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            _ => false,
        });
        out
    }
}

fields! {
    pub struct RegionCampaignDef {
        region_bonus_army: Option<String> = "RegionBonusArmy",
        region_bonus_resource: Option<String> = "RegionBonusResource",
        region_bonus_legendary: Option<String> = "RegionBonusLegendary",
        region_conquered_sound: Option<String> = "RegionConqueredSound",
        region_effects_manager_name: Option<String> = "RegionEffectsManagerName",
        hero_only_army_command_points: Option<i32> = "HeroOnlyArmyCommandPoints",
        small_army_command_points: Option<i32> = "SmallArmyCommandPoints",
        medium_army_command_points: Option<i32> = "MediumArmyCommandPoints",
        army_carryover_points: Option<i32> = "ArmyCarryoverPoints",
        army_retreat_rounds: Option<i32> = "ArmyRetreatRounds",
        army_placement_pos: Vec<Coord2> = "ArmyPlacementPos",
    }
    with {
        pub regions: Vec<RegionDef>,
        pub concurrent_bonuses: Vec<ConcurrentRegionBonusDef>,
    }
}

impl Defined for RegionCampaignDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl RegionCampaignDef {
    #[must_use]
    pub fn region(&self, name: &str) -> Option<&RegionDef> {
        self.regions
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
    }

    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "Region" => {
                out.regions.push(RegionDef::read(child, cx));
                true
            }
            Node::Block(child) if child.keyword == "ConcurrentRegionBonus" => {
                out.concurrent_bonuses
                    .push(ConcurrentRegionBonusDef::read(child, cx));
                true
            }
            _ => false,
        });
        out
    }
}
