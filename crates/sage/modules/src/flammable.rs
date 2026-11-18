use mint_sim::{FieldError, LogicFrames, ModuleFieldSource};

use crate::fire_weapon_update::milliseconds;

#[derive(Debug, Clone, PartialEq)]
pub struct Flammable {
    pub damage_type: Option<String>,
    pub flame_damage_limit: f32,
    pub flame_damage_expiration: LogicFrames,
    pub aflame_duration: Option<LogicFrames>,
    pub burned_delay: LogicFrames,
    pub set_burned_status: bool,
    pub aflame_damage_delay: LogicFrames,
    pub aflame_damage_amount: f32,
    pub swap_model_when_aflame: bool,
    pub swap_model_when_quenched: bool,
    pub swap_texture_when_aflame: bool,
    pub swap_texture_when_quenched: bool,
    pub burn_contained: bool,
}

pub fn flammable_from_fields(fields: &dyn ModuleFieldSource) -> Result<Flammable, FieldError> {
    let aflame_duration_ms = fields.field_numeric("AflameDuration");
    Ok(Flammable {
        damage_type: fields.field("DamageType").map(str::to_owned),
        flame_damage_limit: fields
            .field_numeric("FlameDamageLimit")
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0),
        flame_damage_expiration: milliseconds(fields, "FlameDamageExpiration"),
        aflame_duration: aflame_duration_ms
            .filter(|ms| ms.is_finite() && *ms > 0.0)
            .map(milliseconds_unchecked),
        burned_delay: milliseconds(fields, "BurnedDelay"),
        set_burned_status: flag(fields, "SetBurnedStatus")?,
        aflame_damage_delay: milliseconds(fields, "AflameDamageDelay"),
        aflame_damage_amount: fields
            .field_numeric("AflameDamageAmount")
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0),
        swap_model_when_aflame: flag(fields, "SwapModelWhenAflame")?,
        swap_model_when_quenched: flag(fields, "SwapModelWhenQuenched")?,
        swap_texture_when_aflame: flag(fields, "SwapTextureWhenAflame")?,
        swap_texture_when_quenched: flag(fields, "SwapTextureWhenQuenhed")?,
        burn_contained: flag(fields, "BurnContained")?,
    })
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn milliseconds_unchecked(ms: f32) -> LogicFrames {
    const MAX_MILLISECONDS: f32 = 86_400_000.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms.min(MAX_MILLISECONDS) as u32;
    crate::special_ability_update::retail_duration_ticks(whole)
}

#[derive(Debug, Clone, PartialEq)]
pub struct FireSpread {
    pub try_range: f32,
    pub min_spread_delay: LogicFrames,
    pub max_spread_delay: LogicFrames,
}

#[must_use]
pub fn fire_spread_from_fields(fields: &dyn ModuleFieldSource) -> FireSpread {
    let min = milliseconds(fields, "MinSpreadDelay");
    let max = milliseconds(fields, "MaxSpreadDelay");
    FireSpread {
        try_range: fields
            .field_numeric("SpreadTryRange")
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0),
        min_spread_delay: min,
        max_spread_delay: if max.get() > min.get() { max } else { min },
    }
}
