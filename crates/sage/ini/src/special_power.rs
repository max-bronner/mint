use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::{parse_float_or_macro, parse_int, MacroTable};

pub const SPECIAL_POWER_KEYWORD: &str = "SpecialPower";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpecialPowerDef {
    pub reload_time_ms: Option<u32>,
    pub shared_synced_timer: bool,
    pub required_sciences: Vec<String>,
    pub max_cast_range: Option<f32>,
    pub radius_cursor_radius: Option<f32>,
    pub unit_cost: Option<u32>,
    pub object_filter: Vec<String>,
    pub forbidden_object_filter: Vec<String>,
    pub forbidden_object_range: Option<f32>,
    pub flags: SpecialPowerFlags,
    pub power_enum: Option<String>,
    pub initiate_sound: Option<String>,
    pub initiate_at_location_sound: Option<String>,
    pub prevent_activation_conditions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpecialPowerFlags {
    pub needs_target: bool,
    pub limit_distance: bool,
    pub needs_object_filter: bool,
    pub no_forbidden_objects: bool,
    pub pathable_only: bool,
    pub water_ok: bool,
    pub respect_recharge_time_discount: bool,
}

impl SpecialPowerFlags {
    fn parse(field: Option<&Field>) -> Self {
        let mut flags = Self::default();
        let Some(field) = field else {
            return flags;
        };
        for token in &field.args {
            match token.to_ascii_uppercase().as_str() {
                "NEEDS_TARGET" => flags.needs_target = true,
                "LIMIT_DISTANCE" => flags.limit_distance = true,
                "NEEDS_OBJECT_FILTER" => flags.needs_object_filter = true,
                "NO_FORBIDDEN_OBJECTS" => flags.no_forbidden_objects = true,
                "PATHABLE_ONLY" => flags.pathable_only = true,
                "WATER_OK" => flags.water_ok = true,
                "RESPECT_RECHARGE_TIME_DISCOUNT" => flags.respect_recharge_time_discount = true,
                _ => {}
            }
        }
        flags
    }
}

fn filter_tokens(field: Option<&Field>, macros: &MacroTable) -> Vec<String> {
    let Some(field) = field else {
        return Vec::new();
    };
    if let [single] = field.args.as_slice() {
        if let Some(expanded) = macros.tokens(single) {
            return expanded.to_vec();
        }
    }
    field.args.clone()
}

fn reload_time(block: &Block, macros: &MacroTable) -> Option<u32> {
    let raw = block.field("ReloadTime").and_then(Field::value)?;
    let ms = parse_float_or_macro(raw, macros)?;
    if !(0.0..=86_400_000.0).contains(&ms) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms as u32;
    Some(whole)
}

#[must_use]
pub fn read_special_power(block: &Block, macros: &MacroTable) -> SpecialPowerDef {
    SpecialPowerDef {
        reload_time_ms: reload_time(block, macros),
        shared_synced_timer: block
            .field("SharedSyncedTimer")
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        required_sciences: block
            .field("RequiredSciences")
            .map(|field| {
                field
                    .args
                    .iter()
                    .filter(|name| !crate::value::is_none(name))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        max_cast_range: block
            .field("MaxCastRange")
            .and_then(Field::value)
            .and_then(|raw| parse_float_or_macro(raw, macros)),
        radius_cursor_radius: block
            .field("RadiusCursorRadius")
            .and_then(Field::value)
            .and_then(|raw| parse_float_or_macro(raw, macros)),
        unit_cost: block
            .field("UnitCost")
            .and_then(Field::value)
            .and_then(parse_int)
            .and_then(|value| u32::try_from(value).ok()),
        object_filter: filter_tokens(block.field("ObjectFilter"), macros),
        forbidden_object_filter: filter_tokens(block.field("ForbiddenObjectFilter"), macros),
        forbidden_object_range: block
            .field("ForbiddenObjectRange")
            .and_then(Field::value)
            .and_then(|raw| parse_float_or_macro(raw, macros)),
        flags: SpecialPowerFlags::parse(block.field("Flags")),
        power_enum: block
            .field("Enum")
            .and_then(Field::value)
            .map(str::to_owned),
        initiate_sound: block
            .field("InitiateSound")
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
            .map(str::to_owned),
        initiate_at_location_sound: block
            .field("InitiateAtLocationSound")
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
            .map(str::to_owned),
        prevent_activation_conditions: block
            .field("PreventActivationConditions")
            .map(|field| {
                field
                    .args
                    .iter()
                    .filter(|name| !crate::value::is_none(name))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpecialPowerIndex {
    defs: BTreeMap<String, SpecialPowerDef>,
    definitions: usize,
    unresolved_reload_times: usize,
}

impl SpecialPowerIndex {
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
        if !block.keyword.eq(SPECIAL_POWER_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        let def = read_special_power(block, macros);
        if def.reload_time_ms.is_none() && block.field("ReloadTime").is_some() {
            self.unresolved_reload_times = self.unresolved_reload_times.saturating_add(1);
        }
        self.defs.insert(name.to_ascii_lowercase(), def);
    }

    #[must_use]
    pub const fn unresolved_reload_times(&self) -> usize {
        self.unresolved_reload_times
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SpecialPowerDef> {
        self.defs.get(&name.to_ascii_lowercase())
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_special_power_reads_reload_time_and_shared_synced_timer() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SuperweaponSpawnOrcs\n  \
                Enum = SPECIAL_SPAWN_ORCS\n  \
                ReloadTime = 20000\n  \
                SharedSyncedTimer = Yes\n\
            End\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_special_power(&doc.roots()[0], &MacroTable::new());
        assert_eq!(def.reload_time_ms, Some(20_000));
        assert!(def.shared_synced_timer);
    }

    #[test]
    fn a_power_with_no_reload_time_field_reads_as_none_not_zero() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SpecialAbilityGrabPassenger\n  Enum = SPECIAL_GRAB_PASSENGER\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let def = read_special_power(&doc.roots()[0], &MacroTable::new());
        assert_eq!(def.reload_time_ms, None);
        assert!(!def.shared_synced_timer);
        assert!(def.required_sciences.is_empty());
    }

    #[test]
    fn required_sciences_read_as_written() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SpellBookRallyingCall\n  \
                RequiredSciences = SCIENCE_RallyingCall SCIENCE_RallyingCallMP\n\
             End\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let def = read_special_power(&doc.roots()[0], &MacroTable::new());
        assert_eq!(
            def.required_sciences,
            vec!["SCIENCE_RallyingCall", "SCIENCE_RallyingCallMP"]
        );
    }

    #[test]
    fn the_index_looks_up_case_insensitively_and_counts_definitions() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SpecialAbilityHornOfGondor\n  ReloadTime = 60000\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let mut index = SpecialPowerIndex::new();
        index.insert_document(&doc, &MacroTable::new());
        assert_eq!(index.definitions(), 1);
        assert_eq!(index.len(), 1);
        assert_eq!(
            index
                .get("specialabilityhornofgondor")
                .and_then(|def| def.reload_time_ms),
            Some(60_000)
        );
        assert!(index.get("nope").is_none());
    }

    #[test]
    fn a_macro_valued_reload_time_resolves_and_an_unknown_one_is_counted() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "#define SPELL_RECHARGE_TIME_TIER_1 180000\n\
             SpecialPower SpellBookHeal\n  ReloadTime = SPELL_RECHARGE_TIME_TIER_1\nEnd\n\
             SpecialPower SpellBookOdd\n  ReloadTime = NOT_DEFINED\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let mut index = SpecialPowerIndex::new();
        index.insert_document(&doc, &macros);
        assert_eq!(
            index.get("SpellBookHeal").and_then(|d| d.reload_time_ms),
            Some(180_000)
        );
        assert_eq!(
            index.get("SpellBookOdd").and_then(|d| d.reload_time_ms),
            None
        );
        assert_eq!(index.unresolved_reload_times(), 1);
    }

    #[test]
    fn needs_target_is_the_seventh_flag() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SpecialAbilitySummon\n  Flags = NEEDS_TARGET WATER_OK\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let def = read_special_power(&doc.roots()[0], &MacroTable::new());
        assert!(def.flags.needs_target);
        assert!(def.flags.water_ok);
    }

    #[test]
    fn max_cast_range_and_unit_cost_read_as_written() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "SpecialPower SpellBookSummon\n  \
                Flags = LIMIT_DISTANCE\n  \
                MaxCastRange = 200\n\
             End\n\
             SpecialPower SpecialAbilityNecroWellOfSouls\n  \
                UnitCost = 4\n\
             End\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let def = read_special_power(&doc.roots()[0], &MacroTable::new());
        assert_eq!(def.max_cast_range, Some(200.0));
        assert!(def.flags.limit_distance);
        let def = read_special_power(&doc.roots()[1], &MacroTable::new());
        assert_eq!(def.unit_cost, Some(4));
    }

    #[test]
    fn object_filter_reads_inline_tokens_or_a_whole_list_macro() {
        let doc = crate::tree::Document::parse(
            "specialpower.ini",
            "#define POISON_OBJECTFILTER ANY +INFANTRY -HERO\n\
             SpecialPower SpecialAbilityGrab\n  \
                ObjectFilter = NONE +INFANTRY -HERO\n\
             End\n\
             SpecialPower SpecialAbilityPoison\n  \
                Flags = NEEDS_OBJECT_FILTER NO_FORBIDDEN_OBJECTS\n  \
                ObjectFilter = POISON_OBJECTFILTER\n  \
                ForbiddenObjectFilter = ANY +STRUCTURE\n  \
                ForbiddenObjectRange = 5\n\
             End\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let def = read_special_power(&doc.roots()[0], &macros);
        assert_eq!(def.object_filter, vec!["NONE", "+INFANTRY", "-HERO"]);
        let def = read_special_power(&doc.roots()[1], &macros);
        assert_eq!(def.object_filter, vec!["ANY", "+INFANTRY", "-HERO"]);
        assert_eq!(def.forbidden_object_filter, vec!["ANY", "+STRUCTURE"]);
        assert_eq!(def.forbidden_object_range, Some(5.0));
        assert!(def.flags.needs_object_filter);
        assert!(def.flags.no_forbidden_objects);
    }
}
