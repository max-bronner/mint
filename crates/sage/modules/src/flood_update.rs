use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog, Name};

const MAX_FLOOD_MEMBERS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct FloodMember {
    pub template: Name,
    pub control_points: [(f32, f32); 4],
    pub speed: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloodSpec {
    pub angle: f32,
    pub relative: bool,
    pub members: Vec<FloodMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloodUpdate {
    spec: Arc<FloodSpec>,
    spawned: bool,
}

impl Component for FloodUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.spawned);
    }
}

impl ModuleKind for FloodUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn on_attach(&mut self, _now: Frame) {
        self.spawned = false;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if !self.spawned {
            self.spawned = true;
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(ctx.entity, Effect::Flood(Arc::clone(&self.spec)));
            }
        }
        Sleep::Forever
    }
}

fn offset(args: &[&str]) -> Option<(f32, f32)> {
    let mut x = None;
    let mut y = None;
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let Some((tag, value)) = token.split_once(':') else {
            continue;
        };
        let raw = if value.is_empty() {
            tokens.next().copied().unwrap_or("")
        } else {
            value
        };
        let parsed = raw.parse::<f32>().ok().filter(|v| v.is_finite());
        if tag.eq_ignore_ascii_case("X") {
            x = parsed;
        } else if tag.eq_ignore_ascii_case("Y") {
            y = parsed;
        }
    }
    Some((x?, y?))
}

pub fn flood_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FloodUpdate, FieldError> {
    let angle = fields
        .field_numeric("AngleOfFlow")
        .filter(|degrees| degrees.is_finite())
        .unwrap_or(0.0)
        .to_radians();
    let relative = crate::fire_weapon_update::flag(fields, "DirectionIsRelative").unwrap_or(false);
    let mut members = Vec::new();
    let mut refused: Option<String> = None;
    fields.nested("FloodMember", &mut |member| {
        if members.len() >= MAX_FLOOD_MEMBERS || refused.is_some() {
            return;
        }
        let Some(template) = member.field("MemberTemplateName") else {
            refused = Some("a FloodMember names no MemberTemplateName".to_owned());
            return;
        };
        let mut control_points = [(0.0, 0.0); 4];
        for (slot, keyword) in control_points.iter_mut().zip([
            "ControlPointOffsetOne",
            "ControlPointOffsetTwo",
            "ControlPointOffsetThree",
            "ControlPointOffsetFour",
        ]) {
            match offset(&member.field_args(keyword)) {
                Some(point) => *slot = point,
                None => {
                    refused = Some(format!("{keyword} is not X:.. Y:.. Z:.."));
                    return;
                }
            }
        }
        let Some(speed) = member
            .field_numeric("MemberSpeed")
            .filter(|speed| speed.is_finite() && *speed > 0.0)
        else {
            refused = Some("MemberSpeed is not a positive number".to_owned());
            return;
        };
        members.push(FloodMember {
            template: Name::from(template),
            control_points,
            speed,
        });
    });
    if let Some(reason) = refused {
        return Err(FieldError::new(format!("FloodUpdate: {reason}")));
    }
    Ok(FloodUpdate {
        spec: Arc::new(FloodSpec {
            angle,
            relative,
            members,
        }),
        spawned: false,
    })
}
