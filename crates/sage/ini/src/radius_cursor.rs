use std::collections::BTreeMap;

use crate::diagnostic::ValueErrors;
use crate::multiplayer_color::{rgb_field, Rgb8};
use crate::tree::{Block, Document, Field};
use crate::value::{parse_bool, parse_float, parse_int, parse_percent};

pub const IN_GAME_UI_KEYWORD: &str = "InGameUI";

pub const RADIUS_CURSOR_TEMPLATE_KEYWORD: &str = "RadiusCursorTemplate";

pub const WEAPON_SCATTER_FIELD: &str = "RadiusCursorUseWeaponScatterRadius";

pub const CLAIMANT_DECAL_FIELD: &str = "PlaceTerrainResourceClaimantDecal";

pub const CLAIM_DECAL_FIELD: &str = "TerrainResourceClaimDecal";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FloatingTextFields {
    pub time_out_ms: Option<f32>,
    pub move_up_speed: Option<f32>,
    pub vanish_rate: Option<f32>,
}

#[must_use]
pub fn read_floating_text_fields(block: &Block, errors: &ValueErrors) -> FloatingTextFields {
    let real = |keyword: &str| -> Option<f32> {
        let field = block.field(keyword)?;
        let read = field.value().and_then(parse_float);
        if read.is_none() {
            errors.field(IN_GAME_UI_KEYWORD, field, "a number");
        }
        read
    };
    FloatingTextFields {
        time_out_ms: real("FloatingTextTimeOut"),
        move_up_speed: real("FloatingTextMoveUpSpeed"),
        vanish_rate: real("FloatingTextVanishRate"),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadiusCursorTemplateDef {
    pub texture: Option<String>,
    pub style: Vec<String>,
    pub opacity_min: f32,
    pub opacity_max: f32,
    pub opacity_throb_time_ms: u32,
    pub color: Option<Rgb8>,
    pub only_visible_to_owning_player: bool,
}

impl Default for RadiusCursorTemplateDef {
    fn default() -> Self {
        Self {
            texture: None,
            style: vec!["SHADOW_ALPHA_DECAL".to_owned()],
            opacity_min: 1.0,
            opacity_max: 1.0,
            opacity_throb_time_ms: 1000,
            color: None,
            only_visible_to_owning_player: true,
        }
    }
}

#[must_use]
pub fn read_radius_cursor_template(block: &Block) -> RadiusCursorTemplateDef {
    let default = RadiusCursorTemplateDef::default();
    RadiusCursorTemplateDef {
        texture: block
            .field("Texture")
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
            .map(str::to_owned),
        style: block
            .field("Style")
            .map(|field| field.args.clone())
            .unwrap_or(default.style),
        opacity_min: block
            .field("OpacityMin")
            .and_then(Field::value)
            .and_then(parse_percent)
            .unwrap_or(default.opacity_min),
        opacity_max: block
            .field("OpacityMax")
            .and_then(Field::value)
            .and_then(parse_percent)
            .unwrap_or(default.opacity_max),
        opacity_throb_time_ms: block
            .field("OpacityThrobTime")
            .and_then(Field::value)
            .and_then(parse_int)
            .and_then(|ms| u32::try_from(ms).ok())
            .unwrap_or(default.opacity_throb_time_ms),
        color: rgb_field(block, "Color"),
        only_visible_to_owning_player: block
            .field("OnlyVisibleToOwningPlayer")
            .and_then(Field::value)
            .and_then(parse_bool)
            .unwrap_or(default.only_visible_to_owning_player),
    }
}

#[derive(Debug, Clone, Default)]
pub struct RadiusCursorIndex {
    defs: BTreeMap<String, RadiusCursorTemplateDef>,
    weapon_scatter: Option<String>,
    claimant_decal: Option<String>,
    claim_decal: Option<String>,
}

impl RadiusCursorIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq_ignore_ascii_case(IN_GAME_UI_KEYWORD) {
            return;
        }
        if let Some(name) = block
            .field(WEAPON_SCATTER_FIELD)
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
        {
            self.weapon_scatter = Some(name.to_ascii_lowercase());
        }
        if let Some(name) = block
            .field(CLAIMANT_DECAL_FIELD)
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
        {
            self.claimant_decal = Some(name.to_owned());
        }
        if let Some(name) = block
            .field(CLAIM_DECAL_FIELD)
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
        {
            self.claim_decal = Some(name.to_owned());
        }
        for child in block.blocks() {
            if !child
                .keyword
                .eq_ignore_ascii_case(RADIUS_CURSOR_TEMPLATE_KEYWORD)
            {
                continue;
            }
            let Some(name) = child.name() else {
                continue;
            };
            self.defs.insert(
                name.to_ascii_lowercase(),
                read_radius_cursor_template(child),
            );
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RadiusCursorTemplateDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn uses_weapon_scatter_radius(&self, name: &str) -> bool {
        self.weapon_scatter
            .as_deref()
            .is_some_and(|scatter| scatter == name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn claimant_decal(&self) -> Option<&str> {
        self.claimant_decal.as_deref()
    }

    #[must_use]
    pub fn claim_decal(&self) -> Option<&str> {
        self.claim_decal.as_deref()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::BlockTable;
    use crate::Document;

    fn parse(text: &str) -> Document {
        Document::parse("ingameui.ini", text, &BlockTable::corpus_path()).expect("parses")
    }

    #[test]
    fn a_template_is_read_off_its_in_game_ui_parent_with_retail_defaults() {
        let document = parse(
            "InGameUI\n  \
               RadiusCursorUseWeaponScatterRadius = ScatterCursor\n  \
               PlaceTerrainResourceClaimantDecal = PlacementRing\n  \
               TerrainResourceClaimDecal = SelectionRing\n  \
               RadiusCursorTemplate = ScatterCursor\n    \
                 Texture = SCCArrowVolley\n    \
                 Style = SHADOW_ALPHA_DECAL\n    \
                 OpacityMin = 20%\n    \
                 OpacityMax = 60%\n    \
                 OpacityThrobTime = 1500\n    \
                 Color = R:255 G:255 B:128 A:255\n    \
                 OnlyVisibleToOwningPlayer = Yes\n  \
               End\n  \
               RadiusCursorTemplate = Bare\n  \
               End\n\
             End\n",
        );
        let mut index = RadiusCursorIndex::new();
        index.insert_document(&document);
        assert_eq!(index.len(), 2);
        assert_eq!(index.claimant_decal(), Some("PlacementRing"));
        assert_eq!(index.claim_decal(), Some("SelectionRing"));
        let scatter = index.get("scattercursor").expect("named template");
        assert_eq!(scatter.texture.as_deref(), Some("SCCArrowVolley"));
        assert!((scatter.opacity_min - 0.2).abs() < 1e-6);
        assert!((scatter.opacity_max - 0.6).abs() < 1e-6);
        assert_eq!(scatter.opacity_throb_time_ms, 1500);
        assert_eq!(
            scatter.color.map(|c| (c.r, c.g, c.b)),
            Some((255, 255, 128))
        );
        assert!(index.uses_weapon_scatter_radius("SCATTERCURSOR"));
        assert!(!index.uses_weapon_scatter_radius("Bare"));
        let bare = index.get("Bare").expect("bare template");
        assert_eq!(bare.texture, None);
        assert!((bare.opacity_min - 1.0).abs() < 1e-6);
        assert_eq!(bare.opacity_throb_time_ms, 1000);
        assert!(bare.only_visible_to_owning_player);
    }

    #[test]
    fn floating_text_fields_are_read_as_written_and_a_bad_one_is_refused() {
        let document = parse(
            "InGameUI\n  \
               FloatingTextTimeOut = 1000\n  \
               FloatingTextMoveUpSpeed = 2.0\n  \
               FloatingTextVanishRate = sideways\n\
             End\n",
        );
        let errors = ValueErrors::new();
        let fields = read_floating_text_fields(&document.roots()[0], &errors);
        assert_eq!(fields.time_out_ms, Some(1000.0));
        assert_eq!(fields.move_up_speed, Some(2.0));
        assert_eq!(fields.vanish_rate, None, "an unreadable value is not read");
        let refused = errors.take();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].field, "FloatingTextVanishRate");
        assert_eq!(refused[0].value, "sideways");

        let bare = parse("InGameUI\nEnd\n");
        let bare_fields = read_floating_text_fields(&bare.roots()[0], &ValueErrors::new());
        assert_eq!(bare_fields, FloatingTextFields::default());
    }
}
