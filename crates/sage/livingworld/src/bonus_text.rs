use mint_sage_ini::living_world::region::{ConcurrentRegionBonusDef, RegionCampaignDef, RegionDef};

use mint_sage_str::{format_placeholder, Placeholder};

use crate::model::{bonus, Bonus};

const LINES: [(usize, &str); 13] = [
    (bonus::ATTACK, "LW:RegionAttackBonus"),
    (bonus::DEFENSE, "LW:RegionDefenseBonus"),
    (bonus::EXPERIENCE, "LW:RegionExperienceBonus"),
    (bonus::RESOURCE, "LW:RegionBonusResource"),
    (bonus::FERTILE_TERRITORY, "LW:RegionTreasuryBonus"),
    (bonus::LEGENDARY, "LW:RegionLegendaryBonus"),
    (bonus::EXTRA_START_RESOURCES, "LW:RegionExtraResourcesBonus"),
    (bonus::BUILDING_DISCOUNT, "LW:RegionBuildingDiscountBonus"),
    (bonus::FREE_INN_UNITS, "LW:RegionFreeInnUnitsBonus"),
    (bonus::DISCOUNTED_SIEGE_UNITS, "LW:RegionSeigeDiscountBonus"),
    (bonus::DISCOUNTED_HERO_UNITS, "LW:RegionHeroDiscountBonus"),
    (bonus::FREE_BUILDER, "LW:RegionFreeBuildersBonus"),
    (
        bonus::DISCOUNTED_BARRACKS_UNITS,
        "LW:RegionBarracksUnitDiscountBonus",
    ),
];

fn block(values: &[(usize, Option<i32>)]) -> Bonus {
    let mut out = Bonus::default();
    for (field, value) in values {
        out.set(*field, value.unwrap_or(0));
    }
    out
}

#[must_use]
pub fn region_bonus(region: &RegionDef) -> Bonus {
    block(&[
        (bonus::RESOURCE, region.resource_bonus),
        (bonus::ARMY, region.army_bonus),
        (bonus::LEGENDARY, region.legendary_bonus),
        (bonus::ATTACK, region.attack_bonus),
        (bonus::DEFENSE, region.defense_bonus),
        (bonus::EXPERIENCE, region.experience_bonus),
        (bonus::FERTILE_TERRITORY, region.fertile_territory_bonus),
    ])
}

#[must_use]
pub fn territory_bonus(territory: &ConcurrentRegionBonusDef) -> Bonus {
    block(&[
        (bonus::RESOURCE, territory.resource_bonus),
        (bonus::ARMY, territory.army_bonus),
        (bonus::LEGENDARY, territory.legendary_bonus),
        (bonus::ATTACK, territory.attack_bonus),
        (bonus::DEFENSE, territory.defense_bonus),
        (bonus::EXPERIENCE, territory.experience_bonus),
        (
            bonus::EXTRA_START_RESOURCES,
            territory.extra_start_resources_bonus,
        ),
        (bonus::BUILDING_DISCOUNT, territory.building_discount_bonus),
        (bonus::FREE_INN_UNITS, territory.free_inn_units_bonus),
        (
            bonus::DISCOUNTED_SIEGE_UNITS,
            territory.discounted_seige_units_bonus,
        ),
        (
            bonus::DISCOUNTED_HERO_UNITS,
            territory.discounted_hero_units_bonus,
        ),
        (bonus::FREE_BUILDER, territory.free_builder_bonus),
        (
            bonus::DISCOUNTED_BARRACKS_UNITS,
            territory.discounted_barracks_units_bonus,
        ),
    ])
}

#[must_use]
pub fn territory_of<'a>(
    campaign: &'a RegionCampaignDef,
    region: &str,
) -> Option<&'a ConcurrentRegionBonusDef> {
    campaign
        .concurrent_bonuses
        .iter()
        .find(|t| t.regions.0.iter().any(|r| r.eq_ignore_ascii_case(region)))
}

#[must_use]
pub fn territory_name(territory: &ConcurrentRegionBonusDef) -> &str {
    territory
        .territory
        .as_deref()
        .unwrap_or(territory.name.as_str())
}

fn filled(template: &str, value: &str) -> String {
    format_placeholder(template, Placeholder::Str(value))
}

#[must_use]
pub fn bonus_lines(block: &Bonus, text: &dyn Fn(&str) -> String) -> String {
    let lines: Vec<String> = LINES
        .iter()
        .filter(|(field, _)| block.get(*field) > 0)
        .map(|(field, key)| filled(&text(key), &block.get(*field).to_string()))
        .collect();
    if lines.is_empty() {
        text("LW:NoBonus")
    } else {
        lines.join(&text("LW:ListSeparator"))
    }
}

#[must_use]
pub fn territory_text(
    region: &RegionDef,
    territory: Option<&ConcurrentRegionBonusDef>,
    text: &dyn Fn(&str) -> String,
) -> String {
    let mut out = text("LW:TerritoryBonusHeader");
    out.push_str(&bonus_lines(&region_bonus(region), text));
    if region.create_auto_fort == Some(true) {
        out.push_str(&text("LW:FortressMap"));
    }
    let plots = region.building_spot.len();
    let key = if plots == 1 {
        "LW:NumberOfBuildPlotsSingle"
    } else {
        "LW:NumberOfBuildPlotsPlural"
    };
    out.push_str(&filled(&text(key), &plots.to_string()));
    if let Some(territory) = territory {
        out.push_str(&filled(
            &text("LW:TerritoryPartOfRegion"),
            &text(territory_name(territory)),
        ));
        out.push_str(&filled(
            &text("LW:UnifiedRegionBonus"),
            &bonus_lines(&territory_bonus(territory), text),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::living_world::Coord2;

    fn text(key: &str) -> String {
        match key {
            "LW:TerritoryBonusHeader" => "[H]".to_owned(),
            "LW:RegionAttackBonus" => "+%d attack".to_owned(),
            "LW:RegionBonusResource" => "+%d res".to_owned(),
            "LW:ListSeparator" => ", ".to_owned(),
            "LW:FortressMap" => "[fort]".to_owned(),
            "LW:NumberOfBuildPlotsSingle" => "[%d plot]".to_owned(),
            "LW:NumberOfBuildPlotsPlural" => "[%d plots]".to_owned(),
            "LW:NoBonus" => "[none]".to_owned(),
            other => other.to_owned(),
        }
    }

    #[test]
    fn a_region_lists_its_bonuses_fortress_and_single_plot() {
        let region = RegionDef {
            attack_bonus: Some(5),
            resource_bonus: Some(100),
            defense_bonus: Some(0),
            create_auto_fort: Some(true),
            building_spot: vec![Coord2::default()],
            ..RegionDef::default()
        };
        assert_eq!(
            territory_text(&region, None, &text),
            "[H]+5 attack, +100 res[fort][1 plot]"
        );
    }

    #[test]
    fn a_region_without_bonuses_says_so() {
        let region = RegionDef::default();
        assert_eq!(territory_text(&region, None, &text), "[H][none][0 plots]");
    }

    #[test]
    fn a_percent_percent_in_a_bonus_template_is_one_percent() {
        let text = |key: &str| match key {
            "LW:RegionAttackBonus" => "+%ls%% attack".to_owned(),
            other => other.to_owned(),
        };
        let mut block = Bonus::default();
        block.set(bonus::ATTACK, 5);
        assert_eq!(bonus_lines(&block, &text), "+5% attack");
    }
}
