use std::cmp::Ordering;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Document, Field};
use crate::value::{parse_float, parse_int, Duration};

pub const DAMAGE_FX_KEYWORD: &str = "DamageFX";

pub const DAMAGE_FX_FIELDS: [&str; 8] = [
    "AmountForMajorFX",
    "MajorFX",
    "MinorFX",
    "ThrottleTime",
    "VeterancyAmountForMajorFX",
    "VeterancyMajorFX",
    "VeterancyMinorFX",
    "VeterancyThrottleTime",
];

pub const VETERANCY_LEVELS: [&str; 4] = ["REGULAR", "VETERAN", "ELITE", "HEROIC"];

const DEFAULT_TOKEN: &str = "DEFAULT";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageFxRow {
    pub amount_for_major_fx: f32,
    pub major_fx: Option<String>,
    pub minor_fx: Option<String>,
    pub throttle: Option<Duration>,
}

type Levels = [DamageFxRow; VETERANCY_LEVELS.len()];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageFxDef {
    default: Levels,
    rows: Vec<(String, Levels)>,
}

impl DamageFxDef {
    #[must_use]
    pub fn row(&self, fx_type: &str, level: usize) -> &DamageFxRow {
        let at = level.min(VETERANCY_LEVELS.len().saturating_sub(1));
        let levels = self
            .rows
            .binary_search_by(|(key, _)| cmp_upper(key, fx_type))
            .ok()
            .and_then(|found| self.rows.get(found))
            .map_or(&self.default, |(_, levels)| levels);
        levels.get(at).unwrap_or(&self.default[0])
    }

    #[must_use]
    pub fn names_row(&self, fx_type: &str) -> bool {
        self.rows
            .binary_search_by(|(key, _)| cmp_upper(key, fx_type))
            .is_ok()
    }

    pub fn fx_lists(&self) -> impl Iterator<Item = &str> {
        self.default
            .iter()
            .chain(self.rows.iter().flat_map(|(_, levels)| levels.iter()))
            .flat_map(|row| [row.major_fx.as_deref(), row.minor_fx.as_deref()])
            .flatten()
    }

    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    fn write(&mut self, fx_type: &str, levels: (usize, usize), set: &impl Fn(&mut DamageFxRow)) {
        let (first, last) = levels;
        if fx_type.eq_ignore_ascii_case(DEFAULT_TOKEN) {
            for row in self
                .default
                .iter_mut()
                .take(last.saturating_add(1))
                .skip(first)
            {
                set(row);
            }
            for (_, existing) in &mut self.rows {
                for row in existing.iter_mut().take(last.saturating_add(1)).skip(first) {
                    set(row);
                }
            }
            return;
        }
        let key = fx_type.to_ascii_uppercase();
        let at = match self
            .rows
            .binary_search_by(|(held, _)| held.as_str().cmp(&key))
        {
            Ok(at) => at,
            Err(at) => {
                self.rows.insert(at, (key, self.default.clone()));
                at
            }
        };
        let Some((_, existing)) = self.rows.get_mut(at) else {
            return;
        };
        for row in existing.iter_mut().take(last.saturating_add(1)).skip(first) {
            set(row);
        }
    }
}

fn cmp_upper(key: &str, token: &str) -> Ordering {
    key.bytes()
        .cmp(token.bytes().map(|b| b.to_ascii_uppercase()))
}

#[must_use]
pub fn read_damage_fx(block: &Block, errors: &ValueErrors) -> DamageFxDef {
    let context = block.name().unwrap_or(DAMAGE_FX_KEYWORD);
    let mut def = DamageFxDef::default();
    for field in block.fields() {
        let Some(kind) = FieldKind::of(&field.keyword) else {
            continue;
        };
        let mut args = field.args.iter().map(String::as_str);
        let levels = if kind.veterancy {
            let Some(level) = args.next().and_then(level_index) else {
                errors.field(context, field, "a veterancy level and a damage-FX type");
                continue;
            };
            (level, level)
        } else {
            (0, VETERANCY_LEVELS.len().saturating_sub(1))
        };
        let Some(fx_type) = args.next() else {
            errors.field(context, field, "a damage-FX type and a value");
            continue;
        };
        let Some(value) = args.next() else {
            errors.field(context, field, "a damage-FX type and a value");
            continue;
        };
        write_value(
            &mut def, kind, fx_type, levels, value, field, context, errors,
        );
    }
    def
}

#[derive(Debug, Clone, Copy)]
struct FieldKind {
    value: ValueKind,
    veterancy: bool,
}

#[derive(Debug, Clone, Copy)]
enum ValueKind {
    Amount,
    Major,
    Minor,
    Throttle,
}

impl FieldKind {
    fn of(keyword: &str) -> Option<Self> {
        let (value, veterancy) = match keyword {
            "AmountForMajorFX" => (ValueKind::Amount, false),
            "MajorFX" => (ValueKind::Major, false),
            "MinorFX" => (ValueKind::Minor, false),
            "ThrottleTime" => (ValueKind::Throttle, false),
            "VeterancyAmountForMajorFX" => (ValueKind::Amount, true),
            "VeterancyMajorFX" => (ValueKind::Major, true),
            "VeterancyMinorFX" => (ValueKind::Minor, true),
            "VeterancyThrottleTime" => (ValueKind::Throttle, true),
            _ => return None,
        };
        Some(Self { value, veterancy })
    }
}

#[allow(clippy::too_many_arguments)]
fn write_value(
    def: &mut DamageFxDef,
    kind: FieldKind,
    fx_type: &str,
    levels: (usize, usize),
    value: &str,
    field: &Field,
    context: &str,
    errors: &ValueErrors,
) {
    match kind.value {
        ValueKind::Amount => match parse_float(value) {
            Some(amount) => def.write(fx_type, levels, &|row| row.amount_for_major_fx = amount),
            None => errors.value(context, field, value, "a number"),
        },
        ValueKind::Major => def.write(fx_type, levels, &|row| {
            row.major_fx = Some(value.to_owned());
        }),
        ValueKind::Minor => def.write(fx_type, levels, &|row| {
            row.minor_fx = Some(value.to_owned());
        }),
        ValueKind::Throttle => match parse_int(value) {
            Some(raw) => {
                #[allow(clippy::cast_precision_loss)]
                let milliseconds = raw as f32;
                def.write(fx_type, levels, &|row| {
                    row.throttle = Some(Duration::Milliseconds(milliseconds));
                });
            }
            None => errors.value(context, field, value, "a duration in milliseconds"),
        },
    }
}

fn level_index(token: &str) -> Option<usize> {
    VETERANCY_LEVELS
        .iter()
        .position(|name| token.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Default)]
pub struct DamageFxIndex {
    defs: Vec<(String, DamageFxDef)>,
    definitions: usize,
}

impl DamageFxIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document, errors: &ValueErrors) {
        for block in document.roots() {
            self.insert_block(block, errors);
        }
    }

    pub fn insert_block(&mut self, block: &Block, errors: &ValueErrors) {
        if !block.keyword.eq(DAMAGE_FX_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        let def = read_damage_fx(block, errors);
        let key = name.to_ascii_uppercase();
        match self.defs.binary_search_by(|(held, _)| held.cmp(&key)) {
            Ok(at) => {
                if let Some(slot) = self.defs.get_mut(at) {
                    slot.1 = def;
                }
            }
            Err(at) => self.defs.insert(at, (key, def)),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&DamageFxDef> {
        self.defs
            .binary_search_by(|(held, _)| cmp_upper(held, name))
            .ok()
            .and_then(|at| self.defs.get(at))
            .map(|(_, def)| def)
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

    pub fn fx_lists(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().flat_map(|(_, def)| def.fx_lists())
    }

    #[must_use]
    pub fn any_names_row(&self, fx_type: &str) -> bool {
        self.defs.iter().any(|(_, def)| def.names_row(fx_type))
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

    fn def(body: &str) -> DamageFxDef {
        let doc = crate::tree::Document::parse(
            "damagefx.ini",
            &format!("DamageFX TestFX\n{body}End\n"),
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        read_damage_fx(&doc.roots()[0], &ValueErrors::new())
    }

    #[test]
    fn a_default_line_overwrites_the_rows_written_before_it() {
        let parsed = def("  MajorFX = SWORD_SLASH FX_Sword\n  \
                          MajorFX = DEFAULT FX_Any\n  \
                          MajorFX = BIG_ROCK FX_Rock\n");
        assert_eq!(
            parsed.row("SWORD_SLASH", 0).major_fx.as_deref(),
            Some("FX_Any")
        );
        assert_eq!(
            parsed.row("BIG_ROCK", 0).major_fx.as_deref(),
            Some("FX_Rock")
        );
        assert_eq!(parsed.row("FLAME", 0).major_fx.as_deref(), Some("FX_Any"));
    }

    #[test]
    fn a_veterancy_line_writes_one_level_only() {
        let parsed = def("  MajorFX = DEFAULT FX_Any\n  \
                          VeterancyMajorFX = ELITE SWORD_SLASH FX_Elite\n");
        assert_eq!(
            parsed.row("SWORD_SLASH", 2).major_fx.as_deref(),
            Some("FX_Elite")
        );
        assert_eq!(
            parsed.row("SWORD_SLASH", 0).major_fx.as_deref(),
            Some("FX_Any")
        );
    }

    #[test]
    fn a_throttle_that_is_not_a_number_is_refused() {
        let doc = crate::tree::Document::parse(
            "damagefx.ini",
            "DamageFX TestFX\n  ThrottleTime = DEFAULT soon\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let errors = ValueErrors::new();
        let parsed = read_damage_fx(&doc.roots()[0], &errors);
        assert!(parsed.row("SWORD_SLASH", 0).throttle.is_none());
        assert_eq!(errors.take().len(), 1);
    }
}
