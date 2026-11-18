use mint_sage_ini::{
    read_award_system, read_create_a_hero_system, read_thing_stats, CommandButtonIndex,
    CommandSetIndex, Document, MacroTable, UpgradeIndex, ValueErrors, AWARD_SYSTEM_KEYWORD,
    CREATE_A_HERO_SYSTEM_KEYWORD,
};
use mint_sage_modules::TemplateRecords;

use super::powers::NO_POWER_BUTTON;
use super::system::{float_to_u32, CahSystem, PowerButton};

const HERO_TEMPLATE: &str = "CreateAHero";

#[must_use]
pub(crate) fn load_system(
    documents: &[Document],
    macros: &MacroTable,
    upgrades: &UpgradeIndex,
    buttons: &CommandButtonIndex,
    sets: &CommandSetIndex,
    templates: &TemplateRecords,
) -> Option<CahSystem> {
    let errors = ValueErrors::new();
    let block = documents
        .iter()
        .find_map(|d| d.roots_named(CREATE_A_HERO_SYSTEM_KEYWORD).next())?;
    let def = read_create_a_hero_system(block, &errors, macros);
    let awards = documents
        .iter()
        .flat_map(|d| d.roots_named(AWARD_SYSTEM_KEYWORD))
        .flat_map(|block| read_award_system(block, &errors))
        .collect();
    let thing_stats = documents
        .iter()
        .flat_map(|d| d.roots_named(AWARD_SYSTEM_KEYWORD))
        .flat_map(|block| read_thing_stats(block, &errors))
        .collect();
    let power_buttons = buttons
        .iter()
        .filter(|(name, def)| {
            !def.create_a_hero_ui_allowable_upgrades.is_empty() || *name == NO_POWER_BUTTON
        })
        .map(|(name, def)| PowerButton::new(name, def))
        .collect();
    let template = def
        .command_set_template
        .as_deref()
        .and_then(|name| sets.get(name));
    let template_icons = template.map_or_else(Vec::new, |set| {
        (1u32..)
            .map_while(|slot| set.slot(slot))
            .map(|name| buttons.get(name).and_then(|b| b.button_image.clone()))
            .collect()
    });
    let base_cost = templates
        .get(HERO_TEMPLATE)
        .and_then(|record| record.build_cost)
        .map_or(0, float_to_u32);
    Some(
        CahSystem::new(&def, upgrades, power_buttons, awards, template)
            .with_base_cost(base_cost)
            .with_thing_stats(thing_stats)
            .with_template_icons(template_icons),
    )
}
