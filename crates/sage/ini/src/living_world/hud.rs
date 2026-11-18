use crate::tree::{Block, Node};

use super::grammar::{fields, label, walk, ColorInt, Cx, FontSpec, Table};

fields! {
    pub struct ArmyDetailsPanel {
        back_button_tooltip: Option<String> = "BackButtonTooltip",
    }
}

fields! {
    pub struct BuildQueueDetailsPanel {
        progress_overlay_color: Option<ColorInt> = "ProgressOverlayColor",
        back_button_tooltip: Option<String> = "BackButtonTooltip",
    }
}

fields! {
    pub struct RegionDetailsPanelStructuresPage {
        progress_overlay_color: Option<ColorInt> = "ProgressOverlayColor",
    }
}

fields! {
    pub struct BattleResolver {
        starting_auto_resolve_battle_message_duration: Option<f32> = "StartingAutoResolveBattleMessageDuration",
        all_enemies_retreated_message_duration: Option<f32> = "AllEnemiesRetreatedMessageDuration",
    }
}

fields! {
    pub struct Checklist {
        item_font: Option<FontSpec> = "ItemFont",
        item_normal_color: Option<ColorInt> = "ItemNormalColor",
        item_important_color: Option<ColorInt> = "ItemImportantColor",
        item_new_color: Option<ColorInt> = "ItemNewColor",
        item_vertical_spacing: Option<f32> = "ItemVerticalSpacing",
    }
}

fields! {
    pub struct DynamicAutoResolveDialog {
        battle_rounds_per_step: Option<i32> = "BattleRoundsPerStep",
    }
}

fields! {
    pub struct RegionDisplay {
        build_plots_title: Option<String> = "BuildPlotsTitle",
        build_plots_help: Option<String> = "BuildPlotsHelp",
        armory_points_title: Option<String> = "ArmoryPointsTitle",
        armory_points_help: Option<String> = "ArmoryPointsHelp",
        command_points_title: Option<String> = "CommandPointsTitle",
        command_points_help: Option<String> = "CommandPointsHelp",
    }
}

fields! {
    pub struct StatsDisplay {
        command_points_row_title: Option<String> = "CommandPointsRowTitle",
        command_points_row_help: Option<String> = "CommandPointsRowHelp",
        attack_bonus_row_title: Option<String> = "AttackBonusRowTitle",
        attack_bonus_row_help: Option<String> = "AttackBonusRowHelp",
        defense_bonus_row_title: Option<String> = "DefenseBonusRowTitle",
        defense_bonus_row_help: Option<String> = "DefenseBonusRowHelp",
        experience_bonus_row_title: Option<String> = "ExperienceBonusRowTitle",
        experience_bonus_row_help: Option<String> = "ExperienceBonusRowHelp",
        resource_multiplier_row_title: Option<String> = "ResourceMultiplierRowTitle",
        resource_multiplier_row_help: Option<String> = "ResourceMultiplierRowHelp",
        power_points_row_title: Option<String> = "PowerPointsRowTitle",
        power_points_row_help: Option<String> = "PowerPointsRowHelp",
        treasury_income_row_title: Option<String> = "TreasuryIncomeRowTitle",
        treasury_income_row_help: Option<String> = "TreasuryIncomeRowHelp",
    }
}

fields! {
    pub struct TypeImages {
        building_fortress: Option<String> = "BuildingFortress",
        building_armory: Option<String> = "BuildingArmory",
        building_resource: Option<String> = "BuildingResource",
        building_barracks: Option<String> = "BuildingBarracks",
        unit_soldier: Option<String> = "UnitSoldier",
        unit_archer: Option<String> = "UnitArcher",
        unit_pikemen: Option<String> = "UnitPikemen",
        unit_cavalry: Option<String> = "UnitCavalry",
        unit_monster: Option<String> = "UnitMonster",
        unit_hero: Option<String> = "UnitHero",
        unit_fortress: Option<String> = "UnitFortress",
        unit_siege: Option<String> = "UnitSiege",
        unit_support: Option<String> = "UnitSupport",
    }
}

fields! {
    pub struct ImageButton {
        image: Option<String> = "Image",
        title: Option<String> = "Title",
        help: Option<String> = "Help",
    }
}

fields! {
    pub struct TitledButton {
        title: Option<String> = "Title",
        help: Option<String> = "Help",
    }
}

fields! {
    pub struct StrategicHudDef {}
    with {
        pub army_details_panel: Option<ArmyDetailsPanel>,
        pub build_queue_details_panel: Option<BuildQueueDetailsPanel>,
        pub region_details_panel_structures_page: Option<RegionDetailsPanelStructuresPage>,
        pub battle_resolver: Option<BattleResolver>,
        pub checklist: Option<Checklist>,
        pub dynamic_auto_resolve_dialog: Option<DynamicAutoResolveDialog>,
        pub region_display: Option<RegionDisplay>,
        pub stats_display: Option<StatsDisplay>,
        pub type_images: Option<TypeImages>,
        pub cancel_army_member_move_button: Option<ImageButton>,
        pub cancel_army_move_button: Option<ImageButton>,
        pub destroy_building_button: Option<ImageButton>,
        pub cancel_building_construction_button: Option<ImageButton>,
        pub disband_army_member_button: Option<ImageButton>,
        pub toggle_selection_details_button: Option<ImageButton>,
        pub upgrade_unit_button: Option<ImageButton>,
        pub options_button: Option<TitledButton>,
        pub objectives_button: Option<TitledButton>,
    }
}

fn element<T: Table + Default>(slot: &mut Option<T>, block: &Block, cx: &Cx) {
    let ctx = label(block);
    let target = slot.get_or_insert_with(T::default);
    walk(block, cx, &ctx, |node| match node {
        Node::Field(field) => target.read_table(field, cx, &ctx),
        _ => false,
    });
}

impl StrategicHudDef {
    pub const ELEMENTS: [&'static str; 18] = [
        "ArmyDetailsPanel",
        "BuildQueueDetailsPanel",
        "RegionDetailsPanelStructuresPage",
        "BattleResolver",
        "Checklist",
        "DynamicAutoResolveDialog",
        "RegionDisplay",
        "StatsDisplay",
        "TypeImages",
        "CancelArmyMemberMoveButton",
        "CancelArmyMoveButton",
        "DestroyBuildingButton",
        "CancelBuildingConstructionButton",
        "DisbandArmyMemberButton",
        "ToggleSelectionDetailsButton",
        "UpgradeUnitButton",
        "OptionsButton",
        "ObjectivesButton",
    ];

    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self::default();
        walk(block, cx, &ctx, |node| {
            let Node::Block(child) = node else {
                return false;
            };
            match child.keyword.as_str() {
                "ArmyDetailsPanel" => element(&mut out.army_details_panel, child, cx),
                "BuildQueueDetailsPanel" => element(&mut out.build_queue_details_panel, child, cx),
                "RegionDetailsPanelStructuresPage" => {
                    element(&mut out.region_details_panel_structures_page, child, cx);
                }
                "BattleResolver" => element(&mut out.battle_resolver, child, cx),
                "Checklist" => element(&mut out.checklist, child, cx),
                "DynamicAutoResolveDialog" => {
                    element(&mut out.dynamic_auto_resolve_dialog, child, cx)
                }
                "RegionDisplay" => element(&mut out.region_display, child, cx),
                "StatsDisplay" => element(&mut out.stats_display, child, cx),
                "TypeImages" => element(&mut out.type_images, child, cx),
                "CancelArmyMemberMoveButton" => {
                    element(&mut out.cancel_army_member_move_button, child, cx)
                }
                "CancelArmyMoveButton" => element(&mut out.cancel_army_move_button, child, cx),
                "DestroyBuildingButton" => element(&mut out.destroy_building_button, child, cx),
                "CancelBuildingConstructionButton" => {
                    element(&mut out.cancel_building_construction_button, child, cx);
                }
                "DisbandArmyMemberButton" => {
                    element(&mut out.disband_army_member_button, child, cx)
                }
                "ToggleSelectionDetailsButton" => {
                    element(&mut out.toggle_selection_details_button, child, cx);
                }
                "UpgradeUnitButton" => element(&mut out.upgrade_unit_button, child, cx),
                "OptionsButton" => element(&mut out.options_button, child, cx),
                "ObjectivesButton" => element(&mut out.objectives_button, child, cx),
                _ => return false,
            }
            true
        });
        out
    }
}
