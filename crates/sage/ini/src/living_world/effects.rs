use crate::tree::{Block, Node};

use super::campaign::ControlPointDef;
use super::grammar::{fields, label, walk, Cx, Defined, Millis, Rgb};

fields! {
    pub struct ColorIntensityPoint {
        intensity: Option<f32> = "Intensity",
        ease_in: Option<f32> = "EaseIn",
        ease_out: Option<f32> = "EaseOut",
        time: Option<Millis> = "Time",
    }
}

fields! {
    pub struct EffectDef {
        geometry: Vec<String> = "Geometry",
        load_in_shell: Option<bool> = "LoadInShell",
        opacity: Option<f32> = "Opacity",
    }
    with {
        pub color_intensity_control_points: Vec<ColorIntensityPoint>,
        pub control_points: Vec<ControlPointDef>,
    }
}

impl EffectDef {
    fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.keyword.clone(),
            ..Self::default()
        };
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "ColorIntensityControlPoint" => {
                out.color_intensity_control_points
                    .push(ColorIntensityPoint::read_plain_block(child, cx));
                true
            }
            Node::Block(child) if child.keyword == "ControlPoint" => {
                out.control_points
                    .push(ControlPointDef::read_plain_block(child, cx));
                true
            }
            _ => false,
        });
        out
    }
}

pub const EFFECT_NAMES: [&str; 9] = [
    "BordersEffect",
    "MouseoverEffectFlareup",
    "MouseoutEffectFlareup",
    "ConqueredEffectEvenglow",
    "ConqueredEffectFlareup",
    "FilledOwnershipEffect",
    "HomeRegionHighlight",
    "RegionSelectionEffect",
    "UnifiedEffect",
];

fields! {
    pub struct RegionEffectsDef {
        region_object: Option<String> = "RegionObject",
        neutral_region_color: Option<Rgb> = "NeutralRegionColor",
        region_border_color: Option<Rgb> = "RegionBorderColor",
        shell_start_position_color: Option<Rgb> = "ShellStartPositionColor",
        should_show_region_borders: Option<bool> = "ShouldShowRegionBorders",
    }
    with {
        pub effects: Vec<EffectDef>,
    }
}

impl Defined for RegionEffectsDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl RegionEffectsDef {
    #[must_use]
    pub fn effect(&self, name: &str) -> Option<&EffectDef> {
        self.effects.iter().find(|effect| effect.name == name)
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
            Node::Block(child) if EFFECT_NAMES.contains(&child.keyword.as_str()) => {
                out.effects.push(EffectDef::read(child, cx));
                true
            }
            _ => false,
        });
        out
    }
}
