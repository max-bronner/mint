use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Field};
use crate::value::{field_expression, is_none, parse_float, parse_int, tagged_value, MacroTable};

pub const CREATE_A_HERO_SYSTEM_KEYWORD: &str = "CreateAHeroSystem";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlingDef {
    pub upgrade: Option<String>,
    pub name_tag: Option<String>,
    pub description_tag: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlingType {
    Attribute,
    Appearance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlingBinderDef {
    pub group: Option<String>,
    pub label_tag: Option<String>,
    pub description_tag: Option<String>,
    pub ui_slot: Option<u32>,
    pub bling_type: Option<BlingType>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlingChoices {
    pub upgrades: Vec<String>,
    pub at: Vec<bool>,
}

fn bling_choices(field: &Field) -> BlingChoices {
    let mut choices = BlingChoices::default();
    for name in field.args.iter().filter(|name| !is_none(name)) {
        match name.strip_prefix('@').filter(|bare| !bare.is_empty()) {
            Some(bare) => {
                choices.upgrades.push(bare.to_owned());
                choices.at.push(true);
            }
            None => {
                choices.upgrades.push(name.clone());
                choices.at.push(false);
            }
        }
    }
    choices
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeDef {
    pub group: Option<String>,
    pub min_upgrade: Option<String>,
    pub max_upgrade: Option<String>,
    pub default_upgrade: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewPreset {
    pub pitch: Option<f32>,
    pub zoom: Option<f32>,
    pub floor: Option<f32>,
    pub dist: Option<f32>,
    pub shift: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewInfoDef {
    pub far: ViewPreset,
    pub near: ViewPreset,
    pub close_up: ViewPreset,
    pub portrait: ViewPreset,
    pub normal_cam: Option<f32>,
    pub camera_angle: Option<f32>,
    pub map_location: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubClassDef {
    pub upgrade: Option<String>,
    pub name_tag: Option<String>,
    pub description_tag: Option<String>,
    pub icon_image: Option<String>,
    pub button_image: Option<String>,
    pub default_faction: Option<String>,
    pub usable_factions: Vec<String>,
    pub bling_upgrades: Vec<BlingChoices>,
    pub default_primary_color: Option<u32>,
    pub default_secondary_color: Option<u32>,
    pub default_tertiary_color: Option<u32>,
    pub spendable_attribute_points: Option<u32>,
    pub attributes: Vec<AttributeDef>,
    pub view_info: Option<ViewInfoDef>,
    pub awards: Vec<String>,
    pub stats: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassDef {
    pub upgrade: Option<String>,
    pub name_tag: Option<String>,
    pub description_tag: Option<String>,
    pub powers_desc_tag: Option<String>,
    pub icon_image: Option<String>,
    pub subclasses: Vec<SubClassDef>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateAHeroSystemDef {
    pub map_mode_upgrade: Option<String>,
    pub game_mode_upgrade: Option<String>,
    pub can_build_upgrade: Option<String>,
    pub command_set_template: Option<String>,
    pub stat_names: Vec<(String, String)>,
    pub weapon_group: Option<String>,
    pub special_anim_percent_chance: Option<f32>,
    pub selected_cheer_anim: Option<String>,
    pub examine_weapon_anim: Option<String>,
    pub examine_self_anim: Option<String>,
    pub examine_anim_tweak_value: Option<i32>,
    pub model_swap_fade_down: Option<f32>,
    pub model_swap_fade_up: Option<f32>,
    pub hero_revival_discount: Option<f32>,
    pub special_power_discount_per_level: Option<f32>,
    pub blings: Vec<BlingDef>,
    pub binders: Vec<BlingBinderDef>,
    pub classes: Vec<ClassDef>,
}

fn text(block: &Block, keyword: &str) -> Option<String> {
    let value = block.field(keyword).and_then(Field::value)?;
    (!is_none(value)).then(|| value.to_owned())
}

fn words(block: &Block, keyword: &str) -> Vec<String> {
    block
        .fields_named(keyword)
        .flat_map(|field| field.args.iter())
        .filter(|name| !is_none(name))
        .cloned()
        .collect()
}

fn number<T>(
    block: &Block,
    keyword: &str,
    errors: &ValueErrors,
    expected: &'static str,
    parser: impl FnOnce(&Field) -> Option<T>,
) -> Option<T> {
    let field = block.field(keyword)?;
    let read = parser(field);
    if read.is_none() {
        errors.field(
            block.name().unwrap_or(block.keyword.as_str()),
            field,
            expected,
        );
    }
    read
}

fn real(block: &Block, keyword: &str, errors: &ValueErrors, macros: &MacroTable) -> Option<f32> {
    number(block, keyword, errors, "a number", |field| {
        field_expression(field, macros)
    })
}

fn int(block: &Block, keyword: &str, errors: &ValueErrors) -> Option<i32> {
    number(block, keyword, errors, "an integer", |field| {
        field
            .value()
            .and_then(parse_int)
            .and_then(|n| i32::try_from(n).ok())
    })
}

fn count(block: &Block, keyword: &str, errors: &ValueErrors) -> Option<u32> {
    number(block, keyword, errors, "a non-negative integer", |field| {
        field
            .value()
            .and_then(parse_int)
            .and_then(|n| u32::try_from(n).ok())
    })
}

fn colour(block: &Block, keyword: &str, errors: &ValueErrors) -> Option<u32> {
    number(block, keyword, errors, "R:n G:n B:n [A:n]", |field| {
        let part = |tag| {
            tagged_value(&field.args, tag)
                .and_then(parse_int)
                .and_then(|n| u8::try_from(n).ok())
        };
        let alpha = match tagged_value(&field.args, "A") {
            Some(_) => part("A")?,
            None => 255,
        };
        Some(
            u32::from(alpha) << 24
                | u32::from(part("R")?) << 16
                | u32::from(part("G")?) << 8
                | u32::from(part("B")?),
        )
    })
}

fn preset(block: &Block, prefix: &str, errors: &ValueErrors) -> ViewPreset {
    let read = |suffix: &str| {
        let keyword = format!("{prefix}{suffix}");
        number(block, &keyword, errors, "a number", |field| {
            field.value().and_then(parse_float)
        })
    };
    ViewPreset {
        pitch: read("Pitch"),
        zoom: read("Zoom"),
        floor: read("Floor"),
        dist: read("Dist"),
        shift: read("Shift"),
    }
}

fn view_info(block: &Block, errors: &ValueErrors) -> ViewInfoDef {
    let float = |keyword| {
        number(block, keyword, errors, "a number", |field| {
            field.value().and_then(parse_float)
        })
    };
    ViewInfoDef {
        far: preset(block, "Far", errors),
        near: preset(block, "Near", errors),
        close_up: preset(block, "CloseUp", errors),
        portrait: preset(block, "Portrait", errors),
        normal_cam: float("NormalCam"),
        camera_angle: float("CameraAngle"),
        map_location: count(block, "MapLocation", errors),
    }
}

fn attribute(block: &Block) -> AttributeDef {
    AttributeDef {
        group: text(block, "GroupName"),
        min_upgrade: text(block, "MinValueUpgrade"),
        max_upgrade: text(block, "MaxValueUpgrade"),
        default_upgrade: text(block, "DefaultValueUpgrade"),
    }
}

fn subclass(block: &Block, errors: &ValueErrors) -> SubClassDef {
    SubClassDef {
        upgrade: text(block, "UpgradeName"),
        name_tag: text(block, "NameTag"),
        description_tag: text(block, "DescriptionTag"),
        icon_image: text(block, "IconImage"),
        button_image: text(block, "ButtonImage"),
        default_faction: text(block, "DefaultFaction"),
        usable_factions: words(block, "UsableFactions"),
        bling_upgrades: block
            .fields_named("BlingUpgrades")
            .map(bling_choices)
            .collect(),
        default_primary_color: colour(block, "DefaultPrimaryColor", errors),
        default_secondary_color: colour(block, "DefaultSecondaryColor", errors),
        default_tertiary_color: colour(block, "DefaultTertiaryColor", errors),
        spendable_attribute_points: count(block, "SpendableAttributePoints", errors),
        attributes: block.blocks_named("Attribute").map(attribute).collect(),
        view_info: block
            .blocks_named("ViewInfo")
            .next()
            .map(|view| view_info(view, errors)),
        awards: words(block, "Awards"),
        stats: words(block, "Stats"),
    }
}

fn class(block: &Block, errors: &ValueErrors) -> ClassDef {
    ClassDef {
        upgrade: text(block, "UpgradeName"),
        name_tag: text(block, "NameTag"),
        description_tag: text(block, "DescriptionTag"),
        powers_desc_tag: text(block, "PowersDescTag"),
        icon_image: text(block, "IconImage"),
        subclasses: block
            .blocks_named("SubClass")
            .map(|sub| subclass(sub, errors))
            .collect(),
    }
}

fn binder(block: &Block, errors: &ValueErrors) -> BlingBinderDef {
    let bling_type = number(
        block,
        "BlingType",
        errors,
        "ATTRIBUTE or APPEARANCE",
        |field| match field.value()? {
            v if v.eq_ignore_ascii_case("ATTRIBUTE") => Some(BlingType::Attribute),
            v if v.eq_ignore_ascii_case("APPEARANCE") => Some(BlingType::Appearance),
            _ => None,
        },
    );
    BlingBinderDef {
        group: text(block, "GroupName"),
        label_tag: text(block, "LabelTag"),
        description_tag: text(block, "DescriptionTag"),
        ui_slot: count(block, "UISlot", errors),
        bling_type,
    }
}

fn bling(block: &Block) -> BlingDef {
    BlingDef {
        upgrade: text(block, "BlingUpgradeName"),
        name_tag: text(block, "NameTag"),
        description_tag: text(block, "DescriptionTag"),
        group: text(block, "GroupName"),
    }
}

#[must_use]
pub fn read_create_a_hero_system(
    block: &Block,
    errors: &ValueErrors,
    macros: &MacroTable,
) -> CreateAHeroSystemDef {
    CreateAHeroSystemDef {
        map_mode_upgrade: text(block, "CreateAHeroMapModeUpgradeName"),
        game_mode_upgrade: text(block, "CreateAHeroGameModeUpgradeName"),
        can_build_upgrade: text(block, "CanBuildCreateAHeroUpgradeName"),
        command_set_template: text(block, "CommandSetTemplate"),
        stat_names: block
            .fields()
            .filter(|field| field.keyword.ends_with("StatName"))
            .filter_map(|field| Some((field.keyword.clone(), field.value()?.to_owned())))
            .collect(),
        weapon_group: text(block, "WeaponGroupName"),
        special_anim_percent_chance: real(block, "SpecialAnimPercentChance", errors, macros),
        selected_cheer_anim: text(block, "SelectedCheerAninName"),
        examine_weapon_anim: text(block, "ExamineWeaponAninName"),
        examine_self_anim: text(block, "ExamineSelfAninName"),
        examine_anim_tweak_value: int(block, "ExamineAnimTweakValue", errors),
        model_swap_fade_down: real(block, "ModelSwapFadeDown", errors, macros),
        model_swap_fade_up: real(block, "ModelSwapFadeUp", errors, macros),
        hero_revival_discount: real(block, "HeroRevivalDiscount", errors, macros),
        special_power_discount_per_level: real(
            block,
            "SpecialPowerDiscountPerLevel",
            errors,
            macros,
        ),
        blings: block.blocks_named("CreateAHeroBling").map(bling).collect(),
        binders: block
            .blocks_named("CreateAHeroBlingBinder")
            .map(|b| binder(b, errors))
            .collect(),
        classes: block
            .blocks_named("CreateAHeroClass")
            .map(|c| class(c, errors))
            .collect(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::tree::Document;

    fn read(text: &str) -> (CreateAHeroSystemDef, ValueErrors) {
        let doc = Document::parse("cah.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let errors = ValueErrors::new();
        let block = doc
            .roots_named(CREATE_A_HERO_SYSTEM_KEYWORD)
            .next()
            .unwrap();
        (read_create_a_hero_system(block, &errors, &macros), errors)
    }

    const SAMPLE: &str = "CreateAHeroSystem\n\
        HeroRevivalDiscount = 75\n\
        StratigicDefeatStatName = STAT_A\n\
        SkirmishVictoryStatName = STAT_B\n\
        CreateAHeroBling\n  GroupName = G_Hat\n  BlingUpgradeName = Upgrade_Hat1\nEnd\n\
        CreateAHeroBlingBinder\n  GroupName = G_Hat\n  UISlot = 2\n  BlingType = APPEARANCE\nEnd\n\
        CreateAHeroClass\n  UpgradeName = Upgrade_ClassA\n\
          SubClass\n    UpgradeName = Upgrade_SubA0\n    UsableFactions = Men Elves\n\
            BlingUpgrades = Upgrade_Hat1 @Upgrade_Hat2\n    BlingUpgrades = Upgrade_Boot1\n\
            Awards = AwardOne\n    Awards = AwardTwo\n\
            DefaultPrimaryColor = R:10 G:20 B:30 A:128\n\
            DefaultSecondaryColor = R:1 G:2 B:3\n\
            Attribute\n      GroupName = G_Armor\n      MinValueUpgrade = Upgrade_Arm1\n    End\n\
            ViewInfo\n      FarPitch = -0.5\n      PortraitDist = 49.0\n      MapLocation = 6\n    End\n\
          End\n\
        End\nEnd\n";

    #[test]
    fn the_whole_tree_is_read_with_repeated_lines_kept_in_order() {
        let (system, errors) = read(SAMPLE);
        assert!(errors.is_empty());
        assert_eq!(system.hero_revival_discount, Some(75.0));
        assert_eq!(system.stat_names.len(), 2);
        assert_eq!(system.blings[0].upgrade.as_deref(), Some("Upgrade_Hat1"));
        assert_eq!(system.binders[0].bling_type, Some(BlingType::Appearance));
        assert_eq!(system.binders[0].ui_slot, Some(2));
        let sub = &system.classes[0].subclasses[0];
        assert_eq!(sub.usable_factions, ["Men", "Elves"]);
        assert_eq!(sub.bling_upgrades.len(), 2);
        assert_eq!(
            sub.bling_upgrades[0].upgrades,
            ["Upgrade_Hat1", "Upgrade_Hat2"]
        );
        assert_eq!(sub.bling_upgrades[0].at, [false, true]);
        assert_eq!(sub.bling_upgrades[1].at, [false]);
        assert_eq!(sub.awards, ["AwardOne", "AwardTwo"]);
        assert_eq!(sub.default_primary_color, Some(0x800A_141E));
        assert_eq!(sub.default_secondary_color, Some(0xFF01_0203));
        assert_eq!(
            sub.attributes[0].min_upgrade.as_deref(),
            Some("Upgrade_Arm1")
        );
        let view = sub.view_info.unwrap();
        assert_eq!(view.far.pitch, Some(-0.5));
        assert_eq!(view.portrait.dist, Some(49.0));
        assert_eq!(view.map_location, Some(6));
    }

    #[test]
    fn an_unreadable_value_is_an_error_not_a_default() {
        let (_, errors) = read(
            "CreateAHeroSystem\n  HeroRevivalDiscount = lots\n\
             CreateAHeroBlingBinder\n  BlingType = SHINY\n  UISlot = -1\nEnd\nEnd\n",
        );
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn the_examine_tweak_is_an_integer() {
        let tweak = |value: &str| {
            let (system, errors) = read(&format!(
                "CreateAHeroSystem
  ExamineAnimTweakValue = {value}
End
"
            ));
            (system.examine_anim_tweak_value, errors.len())
        };
        assert_eq!(tweak("-3"), (Some(-3), 0));
        assert_eq!(tweak("x"), (None, 1));
    }
}
