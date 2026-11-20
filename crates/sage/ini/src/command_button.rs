use std::collections::BTreeMap;

use crate::diagnostic::{ValueError, ValueErrors};
use crate::stances::Stance;
use crate::tree::{Block, Document, Field};
use crate::value::{field_expression, MacroTable};
use crate::value::{is_none, parse_bool, parse_int};

pub const COMMAND_BUTTON_KEYWORD: &str = "CommandButton";

pub const COMMAND_BUTTON_FIELDS: [&str; 41] = [
    "TriggerWhenReady",
    "WeaponSlotToggle1",
    "WeaponSlotToggle2",
    "WeaponSlotToggle3",
    "Command",
    "ShowButton",
    "Options",
    "TextLabel",
    "DescriptLabel",
    "ButtonImage",
    "ButtonBorderType",
    "InPalantir",
    "Radial",
    "SpecialPower",
    "Object",
    "Upgrade",
    "AutoAbility",
    "AutoDelay",
    "AutoAbilityDisallowedOnModelCondition",
    "CreateAHeroUIMinimumLevel",
    "CreateAHeroUIPrerequisiteButtonName",
    "CreateAHeroUIIconImageName",
    "CreateAHeroUIAllowableUpgrades",
    "CreateAHeroUICostIfSelected",
    "CommandRangeStart",
    "CommandRangeCount",
    "NeededUpgrade",
    "NeededUpgradeAny",
    "Science",
    "NeedDamagedTarget",
    "CursorName",
    "InvalidCursorName",
    "RadiusCursorType",
    "PresetRange",
    "UnitSpecificSound",
    "FlagsUsedForToggle",
    "RequiresValidContainer",
    "ShowProductionCount",
    "WeaponSlot",
    "EnableOnModelCondition",
    "DisableOnModelCondition",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandButtonDef {
    pub cursor_name: Option<String>,
    pub invalid_cursor_name: Option<String>,
    pub radius_cursor_type: Option<String>,
    pub command: Option<String>,
    pub options: Vec<String>,
    pub text_label: Option<String>,
    pub descript_label: Option<String>,
    pub button_image: Option<String>,
    pub button_border_type: Option<String>,
    pub in_palantir: Option<bool>,
    pub radial: Option<bool>,
    pub special_power: Option<String>,
    pub object: Option<String>,
    pub upgrade: Option<String>,
    pub auto_ability: Option<bool>,
    pub auto_delay: Option<f32>,
    pub auto_ability_disallowed_on_model_condition: Vec<String>,
    pub create_a_hero_ui_minimum_level: Option<i64>,
    pub create_a_hero_ui_prerequisite_button: Option<String>,
    pub create_a_hero_ui_icon_image: Option<String>,
    pub create_a_hero_ui_allowable_upgrades: Vec<String>,
    pub create_a_hero_ui_cost_if_selected: Option<f32>,
    pub command_range_start: Option<u32>,
    pub command_range_count: Option<u32>,
    pub needed_upgrade: Vec<String>,
    pub needed_upgrade_any: bool,
    pub science: Vec<String>,
    pub need_damaged_target: bool,
    pub show_button: bool,
    pub preset_range: Option<f32>,
    pub stances: Vec<String>,
    pub stance_faces: Vec<ButtonFace>,
    pub toggle_faces: Vec<ButtonFace>,
    pub unit_specific_sound: Vec<String>,
    pub flags_used_for_toggle: Vec<String>,
    pub requires_valid_container: bool,
    pub show_production_count: bool,
    pub weapon_slot: Option<u8>,
    pub enable_on_model_condition: Vec<String>,
    pub disable_on_model_condition: Vec<String>,
    pub trigger_when_ready: bool,
    pub weapon_slot_toggles: [Option<u8>; 3],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ButtonFace {
    pub text_label: Option<String>,
    pub descript_label: Option<String>,
    pub button_image: Option<String>,
}

impl CommandButtonDef {
    #[must_use]
    pub fn stance_face(&self, stance: Stance) -> Option<&ButtonFace> {
        let at = self
            .stances
            .iter()
            .position(|name| Stance::from_name(name) == Some(stance))?;
        self.stance_faces.get(at)
    }

    #[must_use]
    pub fn toggle_face(&self, toggled: bool) -> Option<&ButtonFace> {
        self.toggle_faces.get(usize::from(toggled))
    }

    #[must_use]
    pub fn unit_specific_sound(&self, state: usize) -> Option<&str> {
        self.unit_specific_sound
            .get(state)
            .or_else(|| self.unit_specific_sound.first())
            .map(String::as_str)
    }

    pub fn needs_met(&self, held: impl Fn(&str) -> bool) -> bool {
        let gated = self
            .options
            .iter()
            .any(|option| option.eq_ignore_ascii_case("NEED_UPGRADE"));
        if !gated || self.needed_upgrade.is_empty() {
            return true;
        }
        if self.needed_upgrade_any {
            self.needed_upgrade.iter().any(|name| held(name))
        } else {
            self.needed_upgrade.iter().all(|name| held(name))
        }
    }
}

fn ref_value(block: &Block, keyword: &str) -> Option<String> {
    let value = block.field(keyword).and_then(Field::value)?;
    (!is_none(value)).then(|| value.to_owned())
}

fn stance_faces(block: &Block, stances: usize) -> Vec<ButtonFace> {
    let nth = |keyword: &str, at: usize| {
        block
            .field(keyword)
            .and_then(|field| field.args.get(at))
            .cloned()
    };
    (0..stances)
        .map(|at| ButtonFace {
            text_label: nth("TextLabel", at),
            descript_label: nth("DescriptLabel", at),
            button_image: nth("ButtonImage", at),
        })
        .collect()
}

fn scalar<T>(
    block: &Block,
    keyword: &str,
    errors: &ValueErrors,
    expected: &'static str,
    parser: impl FnOnce(&str) -> Option<T>,
) -> Option<T> {
    let field = block.field(keyword)?;
    let read = field.value().and_then(parser);
    if read.is_none() {
        errors.field(name_of(block), field, expected);
    }
    read
}

fn name_of(block: &Block) -> &str {
    block.name().unwrap_or(COMMAND_BUTTON_KEYWORD)
}

#[must_use]
pub fn read_command_button(
    block: &Block,
    errors: &ValueErrors,
    macros: &MacroTable,
) -> CommandButtonDef {
    let stances: Vec<String> = block
        .field("Stances")
        .map(|f| f.args.clone())
        .unwrap_or_default();
    let parallel = ["TextLabel", "DescriptLabel", "ButtonImage"]
        .iter()
        .filter_map(|keyword| block.field(keyword).map(|field| field.args.len()))
        .max()
        .unwrap_or(0);
    let toggle_faces = if stances.is_empty() && parallel > 1 {
        stance_faces(block, parallel)
    } else {
        Vec::new()
    };
    CommandButtonDef {
        stance_faces: stance_faces(block, stances.len()),
        toggle_faces,
        stances,
        cursor_name: ref_value(block, "CursorName"),
        invalid_cursor_name: ref_value(block, "InvalidCursorName"),
        radius_cursor_type: ref_value(block, "RadiusCursorType"),
        command: ref_value(block, "Command"),
        options: block
            .field("Options")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        text_label: block
            .field("TextLabel")
            .and_then(Field::value)
            .map(str::to_owned),
        descript_label: block
            .field("DescriptLabel")
            .and_then(Field::value)
            .map(str::to_owned),
        button_image: block
            .field("ButtonImage")
            .and_then(Field::value)
            .map(str::to_owned),
        button_border_type: ref_value(block, "ButtonBorderType"),
        in_palantir: scalar(block, "InPalantir", errors, "Yes or No", parse_bool),
        radial: scalar(block, "Radial", errors, "Yes or No", parse_bool),
        special_power: block
            .field("SpecialPower")
            .and_then(Field::value)
            .map(str::to_owned),
        object: block
            .field("Object")
            .and_then(Field::value)
            .map(str::to_owned),
        upgrade: block
            .field("Upgrade")
            .and_then(Field::value)
            .map(str::to_owned),
        auto_ability: scalar(block, "AutoAbility", errors, "Yes or No", parse_bool),
        auto_delay: scalar(
            block,
            "AutoDelay",
            errors,
            "a number",
            crate::value::parse_float,
        ),
        auto_ability_disallowed_on_model_condition: block
            .field("AutoAbilityDisallowedOnModelCondition")
            .map(|f| f.args.iter().filter(|a| !is_none(a)).cloned().collect())
            .unwrap_or_default(),
        create_a_hero_ui_minimum_level: scalar(
            block,
            "CreateAHeroUIMinimumLevel",
            errors,
            "an integer",
            parse_int,
        ),
        create_a_hero_ui_prerequisite_button: ref_value(
            block,
            "CreateAHeroUIPrerequisiteButtonName",
        ),
        create_a_hero_ui_icon_image: ref_value(block, "CreateAHeroUIIconImageName"),
        create_a_hero_ui_allowable_upgrades: names(block, "CreateAHeroUIAllowableUpgrades"),
        create_a_hero_ui_cost_if_selected: block.field("CreateAHeroUICostIfSelected").and_then(
            |field| {
                let read = field_expression(field, macros);
                if read.is_none() {
                    errors.field(name_of(block), field, "a number");
                }
                read
            },
        ),
        command_range_start: scalar(
            block,
            "CommandRangeStart",
            errors,
            "a non-negative integer",
            |v| parse_int(v).and_then(|n| u32::try_from(n).ok()),
        ),
        command_range_count: scalar(
            block,
            "CommandRangeCount",
            errors,
            "a non-negative integer",
            |v| parse_int(v).and_then(|n| u32::try_from(n).ok()),
        ),
        needed_upgrade: block
            .field("NeededUpgrade")
            .map(|f| f.args.iter().filter(|a| !is_none(a)).cloned().collect())
            .unwrap_or_default(),
        needed_upgrade_any: scalar(block, "NeededUpgradeAny", errors, "Yes or No", parse_bool)
            .unwrap_or(false),
        science: block
            .field("Science")
            .map(|f| f.args.iter().filter(|a| !is_none(a)).cloned().collect())
            .unwrap_or_default(),
        need_damaged_target: scalar(block, "NeedDamagedTarget", errors, "Yes or No", parse_bool)
            .unwrap_or(false),
        show_button: scalar(block, "ShowButton", errors, "Yes or No", parse_bool).unwrap_or(true),
        preset_range: block.field("PresetRange").and_then(|field| {
            let read = crate::value::field_expression(field, macros);
            if read.is_none() {
                errors.field(name_of(block), field, "a number, macro or arithmetic call");
            }
            read.filter(|range| range.is_finite() && *range > 0.0)
        }),
        unit_specific_sound: names(block, "UnitSpecificSound"),
        flags_used_for_toggle: names(block, "FlagsUsedForToggle"),
        requires_valid_container: scalar(
            block,
            "RequiresValidContainer",
            errors,
            "Yes or No",
            parse_bool,
        )
        .unwrap_or(false),
        show_production_count: scalar(
            block,
            "ShowProductionCount",
            errors,
            "Yes or No",
            parse_bool,
        )
        .unwrap_or(false),
        weapon_slot: scalar(
            block,
            "WeaponSlot",
            errors,
            "a weapon slot, PRIMARY to QUINARY",
            weapon_slot,
        ),
        enable_on_model_condition: names(block, "EnableOnModelCondition"),
        disable_on_model_condition: names(block, "DisableOnModelCondition"),
        trigger_when_ready: scalar(block, "TriggerWhenReady", errors, "Yes or No", parse_bool)
            .unwrap_or(false),
        weapon_slot_toggles: [
            "WeaponSlotToggle1",
            "WeaponSlotToggle2",
            "WeaponSlotToggle3",
        ]
        .map(|keyword| {
            scalar(
                block,
                keyword,
                errors,
                "a weapon slot, PRIMARY to QUINARY",
                weapon_slot,
            )
        }),
    }
}

pub(crate) fn weapon_slot(value: &str) -> Option<u8> {
    ["PRIMARY", "SECONDARY", "TERTIARY", "QUATERNARY", "QUINARY"]
        .iter()
        .position(|name| value.eq_ignore_ascii_case(name))
        .and_then(|at| u8::try_from(at).ok())
}

fn names(block: &Block, keyword: &str) -> Vec<String> {
    block
        .field(keyword)
        .map(|f| f.args.iter().filter(|a| !is_none(a)).cloned().collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default)]
pub struct CommandButtonIndex {
    defs: BTreeMap<String, CommandButtonDef>,
    definitions: usize,
    errors: Vec<ValueError>,
}

impl CommandButtonIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, macros: &MacroTable) {
        for block in document.roots() {
            self.insert_block(block, macros);
        }
    }

    pub fn insert_block(&mut self, block: &Block, macros: &MacroTable) {
        if !block.keyword.eq(COMMAND_BUTTON_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        let errors = ValueErrors::new();
        let def = read_command_button(block, &errors, macros);
        self.errors.append(&mut errors.take());
        self.defs.insert(name.to_ascii_lowercase(), def);
    }

    #[must_use]
    pub fn errors(&self) -> &[ValueError] {
        &self.errors
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&CommandButtonDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &CommandButtonDef)> {
        self.defs.iter().map(|(name, def)| (name.as_str(), def))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_command_button_reads_its_top_fields() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_PlaceBeacon\n  \
                Command = PLACE_BEACON\n  \
                Options = NEED_TARGET_POS OK_FOR_MULTI_SELECT\n  \
                TextLabel = CONTROLBAR:RadialBack\n  \
                InPalantir = Yes\n  \
                CreateAHeroUIMinimumLevel = 3\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert_eq!(def.command.as_deref(), Some("PLACE_BEACON"));
        assert_eq!(def.options, vec!["NEED_TARGET_POS", "OK_FOR_MULTI_SELECT"]);
        assert_eq!(def.in_palantir, Some(true));
        assert_eq!(def.create_a_hero_ui_minimum_level, Some(3));
    }

    #[test]
    fn command_range_start_and_count_are_read_for_a_paging_button() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_SelectUpgradesMenFortress\n  \
                Command = PUSH_VISIBLE_COMMAND_RANGE\n  \
                Radial = Yes\n  \
                CommandRangeStart = 7\n  \
                CommandRangeCount = 7\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert_eq!(def.command.as_deref(), Some("PUSH_VISIBLE_COMMAND_RANGE"));
        assert_eq!(def.command_range_start, Some(7));
        assert_eq!(def.command_range_count, Some(7));
    }

    #[test]
    fn auto_delay_and_disallowed_model_condition_are_read_as_written() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_SpecialAbilityTheodenKingsFavour\n  \
                Command = SPECIAL_POWER\n  \
                AutoAbility = Yes\n  \
                AutoDelay = 30.0\n  \
                AutoAbilityDisallowedOnModelCondition = MOUNTED\n  \
                PresetRange = 150.0\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert_eq!(def.auto_ability, Some(true));
        assert_eq!(def.auto_delay, Some(30.0));
        assert_eq!(
            def.auto_ability_disallowed_on_model_condition,
            vec!["MOUNTED"]
        );
        assert_eq!(def.preset_range, Some(150.0));
    }

    #[test]
    fn needed_upgrade_reads_every_name() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_Slot
                  Command = REVIVE
                  Options = HIDE_WHILE_DISABLED NEED_UPGRADE
                  NeededUpgrade = Upgrade_A Upgrade_B
            End
",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert_eq!(def.needed_upgrade, vec!["Upgrade_A", "Upgrade_B"]);
    }

    #[test]
    fn none_clears_a_reference_field_rather_than_naming_one() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Empty\n  Command = NONE\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let def = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert_eq!(def.command, None);
    }

    #[test]
    fn needed_upgrade_any_turns_all_into_any() {
        let parse = |text: &str| {
            let doc = crate::tree::Document::parse(
                "commandbutton.ini",
                text,
                &BlockTable::priority_blocks_path(),
            )
            .unwrap();
            read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new())
        };
        let any = parse(
            "CommandButton A\n  Command = PLAYER_UPGRADE\n  Options = NEED_UPGRADE\n  \
             NeededUpgrade = Upgrade_Forge Upgrade_Range\n  NeededUpgradeAny = Yes\nEnd\n",
        );
        assert!(any.needed_upgrade_any);
        let only_range = |name: &str| name.eq_ignore_ascii_case("upgrade_range");
        assert!(any.needs_met(only_range));
        assert!(!any.needs_met(|_| false));
        let all = parse(
            "CommandButton B\n  Command = PLAYER_UPGRADE\n  Options = NEED_UPGRADE\n  \
             NeededUpgrade = Upgrade_Forge Upgrade_Range\nEnd\n",
        );
        assert!(!all.needs_met(only_range));
        assert!(all.needs_met(|_| true));
        let ungated = parse(
            "CommandButton C\n  Command = PLAYER_UPGRADE\n  NeededUpgrade = Upgrade_Forge\nEnd\n",
        );
        assert!(ungated.needs_met(|_| false), "no NEED_UPGRADE option");
    }

    #[test]
    fn a_toggle_buttons_sound_is_the_entry_for_its_state() {
        let parse = |text: &str| {
            let doc = crate::tree::Document::parse(
                "commandbutton.ini",
                text,
                &BlockTable::priority_blocks_path(),
            )
            .unwrap();
            read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new())
        };
        let toggle = parse(
            "CommandButton T
  Command = SPECIAL_POWER
               UnitSpecificSound = MountSound DismountSound ; trailing
               FlagsUsedForToggle = MOUNTED
End
",
        );
        let single = parse(
            "CommandButton S
  UnitSpecificSound = OnlySound
End
",
        );
        let silent = parse(
            "CommandButton N
  Command = SELL
End
",
        );
        for (button, state, sound) in [
            (&toggle, 0, Some("MountSound")),
            (&toggle, 1, Some("DismountSound")),
            (&single, 0, Some("OnlySound")),
            (&single, 1, Some("OnlySound")),
            (&silent, 0, None),
        ] {
            assert_eq!(button.unit_specific_sound(state), sound, "state {state}");
        }
        assert_eq!(toggle.flags_used_for_toggle, vec!["MOUNTED"]);
    }

    #[test]
    fn a_wrong_typed_bool_is_refused_and_an_absent_one_defaults() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_MadeUpExit\n  \
                RequiresValidContainer = Maybe\n\
            End\n\
            CommandButton Command_MadeUpBare\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let errors = ValueErrors::new();
        let bad = read_command_button(&doc.roots()[0], &errors, &MacroTable::new());
        assert!(!bad.requires_valid_container);
        let refused = errors.take();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].context, "Command_MadeUpExit");
        assert_eq!(refused[0].field, "RequiresValidContainer");
        assert_eq!(refused[0].value, "Maybe");

        let bare = read_command_button(&doc.roots()[1], &ValueErrors::new(), &MacroTable::new());
        assert!(
            !bare.requires_valid_container,
            "absent stays the default, not an error"
        );
    }

    #[test]
    fn show_production_count_is_read_as_written() {
        let doc = crate::tree::Document::parse(
            "commandbutton.ini",
            "CommandButton Command_ConstructUnit\n  \
                Command = UNIT_BUILD\n  \
                ShowProductionCount = Yes\n\
            End\n\
            CommandButton Command_Revive\n  \
                Command = REVIVE\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let build = read_command_button(&doc.roots()[0], &ValueErrors::new(), &MacroTable::new());
        assert!(build.show_production_count);
        let revive = read_command_button(&doc.roots()[1], &ValueErrors::new(), &MacroTable::new());
        assert!(!revive.show_production_count);
    }
}
