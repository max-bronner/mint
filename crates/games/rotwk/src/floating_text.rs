use std::collections::HashMap;

use mint_render::Vec3;
use mint_sim::{Entity, PlayerId};

use mint_sage_asset::KindOf;

use crate::game::{ground_at, MatchState, PlayerColors};
use crate::render_state::UnitView;
use crate::rules::{CrateTextKind, IncomeView};
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatingTextTiming {
    pub(crate) timeout: u32,
    pub(crate) rise: f32,
    pub(crate) vanish: f32,
}

impl Default for FloatingTextTiming {
    fn default() -> Self {
        Self {
            timeout: 10,
            rise: 1.0,
            vanish: 0.1,
        }
    }
}

pub(crate) fn read_timing(
    documents: &[mint_sage_ini::Document],
    errors: &mint_sage_ini::ValueErrors,
) -> FloatingTextTiming {
    let mut out = FloatingTextTiming::default();
    let blocks = documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"));
    for block in blocks {
        let fields = mint_sage_ini::read_floating_text_fields(block, errors);
        #[allow(clippy::cast_precision_loss)]
        let hz = mint_sim::SIMULATION_HZ as f32;
        if let Some(ms) = fields.time_out_ms {
            out.timeout = if ms.is_finite() && ms > 0.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let frames = (ms * hz / 1000.0).ceil().min(u32::MAX as f32) as u32;
                frames
            } else {
                FloatingTextTiming::default().timeout
            };
        }
        if let Some(per_second) = fields.move_up_speed.filter(|v| v.is_finite() && *v >= 0.0) {
            out.rise = per_second / hz;
        }
        if let Some(per_second) = fields.vanish_rate.filter(|v| v.is_finite() && *v >= 0.0) {
            out.vanish = per_second / hz;
        }
    }
    out
}

const START_ALPHA: u8 = 230;

const LIFT: f32 = 10.0;

const SCATTER: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    pub(crate) feet: Vec3,
    pub(crate) scatter: Option<[f32; 2]>,
    pub(crate) shown: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FloatingText {
    pub(crate) text: String,
    pub(crate) at: Vec3,
    pub(crate) owner: PlayerId,
    pub(crate) frames: u32,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Label<'a> {
    pub(crate) text: &'a str,
    pub(crate) at: Vec3,
    pub(crate) rise_px: f32,
    pub(crate) colour: [f32; 4],
}

#[derive(Debug, Default)]
pub(crate) struct FloatingTexts {
    pub(crate) texts: Vec<FloatingText>,
    due: HashMap<Entity, (PlayerId, u32, i64)>,
    next: HashMap<Entity, (PlayerId, u32, i64)>,
    scatter: u32,
    pub(crate) rise: f32,
    pub(crate) shown: u64,
}

impl FloatingTexts {
    pub(crate) fn step(
        &mut self,
        timing: &FloatingTextTiming,
        views: &[IncomeView],
        place: impl Fn(Entity) -> Option<Placed>,
        text: impl Fn(i64) -> String,
    ) {
        self.rise = timing.rise;
        self.texts.retain_mut(|floating| {
            floating.frames = floating.frames.saturating_add(1);
            if floating.frames > timing.timeout {
                let past = floating.frames.saturating_sub(timing.timeout);
                #[allow(clippy::cast_precision_loss)]
                let fade = past as f32 * timing.vanish;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let fade = fade.clamp(0.0, 255.0) as u8;
                floating.alpha = floating.alpha.saturating_sub(fade);
            }
            floating.alpha > 0
        });
        self.next.clear();
        for view in views {
            let paid = self
                .due
                .get(&view.structure)
                .is_some_and(|(owner, due_in, _)| {
                    *owner == view.owner && *due_in == 1 && view.due_in == view.interval
                });
            if paid {
                let amount = self
                    .due
                    .get(&view.structure)
                    .map_or(0, |(_, _, amount)| *amount);
                if amount > 0 {
                    if let Some(placed) = place(view.structure).filter(|placed| placed.shown) {
                        let (dx, dy) = match placed.scatter {
                            Some([major, minor]) => (
                                self.next_scatter() * major * SCATTER,
                                self.next_scatter() * minor * SCATTER,
                            ),
                            None => (0.0, 0.0),
                        };
                        self.texts.push(FloatingText {
                            text: text(amount),
                            at: placed.feet + Vec3::new(dx, dy, LIFT),
                            owner: view.owner,
                            frames: 0,
                            alpha: START_ALPHA,
                        });
                        self.shown = self.shown.saturating_add(1);
                    }
                }
            }
            self.next
                .insert(view.structure, (view.owner, view.due_in, view.amount));
        }
        std::mem::swap(&mut self.due, &mut self.next);
    }

    fn next_scatter(&mut self) -> f32 {
        let mut x = if self.scatter == 0 {
            0x9E37_79B9
        } else {
            self.scatter
        };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.scatter = x;
        #[allow(clippy::cast_precision_loss)]
        let unit = (x >> 8) as f32 / (1u32 << 24) as f32;
        unit * 2.0 - 1.0
    }
}

pub(crate) fn float_deposits(
    state: &mut MatchState,
    timing: &FloatingTextTiming,
    units: &[UnitView],
) {
    let simulation = &state.simulation;
    let rules = simulation.rules();
    let world = simulation.state();
    let views = rules.income_views(world);
    let strings = &state.strings;
    let scene = &state.scene;
    let kinds = &state.kinds;
    let viewer = state.shroud_viewer;
    let place = |structure: mint_sim::Entity| {
        let unit = units.iter().find(|unit| unit.entity == structure)?;
        let (x, y) = (unit.position.x, unit.position.y);
        let scatter = unit.kind_of.contains(KindOf::STRUCTURE).then(|| {
            let geometry = unit
                .kind
                .and_then(|kind| kinds.get(Kind(kind)))
                .map(|entry| &entry.record.geometry);
            geometry.map_or([unit.radius; 2], geometry_radii)
        });
        Some(Placed {
            feet: Vec3::new(x, y, ground_at(scene, x, y)),
            scatter,
            shown: rules
                .ids()
                .is_none_or(|ids| !rules.stealthed_from(world, ids, viewer, structure)),
        })
    };
    let text = |amount: i64| {
        mint_sage_str::format_placeholder(
            &strings.fetch("GUI:AddCash"),
            mint_sage_str::Placeholder::Int(amount),
        )
    };
    state.floating.step(timing, &views, place, text);
    let Some(ran) = simulation.frame().get().checked_sub(1) else {
        return;
    };
    for shown in rules.crate_texts(world, mint_sim::Frame::new(ran)) {
        let (label, value) = match shown.kind {
            CrateTextKind::Cash(amount) => ("GUI:AddCash", amount),
            CrateTextKind::Rank => ("GUI:GainRank", 1),
        };
        let (x, y) = (shown.at.x, shown.at.y);
        state.floating.texts.push(FloatingText {
            text: mint_sage_str::format_placeholder(
                &strings.fetch(label),
                mint_sage_str::Placeholder::Int(value),
            ),
            at: Vec3::new(x, y, ground_at(scene, x, y) + LIFT),
            owner: shown.player,
            frames: 0,
            alpha: START_ALPHA,
        });
        state.floating.shown = state.floating.shown.saturating_add(1);
    }
}

fn geometry_radii(geometry: &mint_sage_modules::Geometry) -> [f32; 2] {
    let major = geometry.major_radius.unwrap_or(0.0);
    let is_box = geometry
        .shape
        .as_deref()
        .is_some_and(|shape| shape.eq_ignore_ascii_case("BOX"));
    let minor = if is_box {
        geometry.minor_radius.unwrap_or(0.0)
    } else {
        major
    };
    [major, minor]
}

pub(crate) fn deposit_labels<'a>(
    floating: &'a FloatingTexts,
    colours: &PlayerColors,
    clear: impl Fn(Vec3) -> bool,
) -> Vec<Label<'a>> {
    floating
        .texts
        .iter()
        .filter(|text| clear(text.at))
        .map(|text| {
            let house = colours.of_or_white(text.owner);
            #[allow(clippy::cast_precision_loss)]
            let rise_px = text.frames as f32 * floating.rise;
            Label {
                text: &text.text,
                at: text.at,
                rise_px,
                colour: [house[0], house[1], house[2], f32::from(text.alpha) / 255.0],
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn farm() -> Entity {
        mint_sim::World::new().spawn().unwrap()
    }

    fn at_origin(_: Entity) -> Option<Placed> {
        Some(Placed {
            feet: Vec3::ZERO,
            scatter: None,
            shown: true,
        })
    }

    fn view(structure: Entity, due_in: u32) -> IncomeView {
        IncomeView {
            structure,
            owner: PlayerId::new(0),
            due_in,
            interval: 180,
            amount: 25,
        }
    }

    #[test]
    fn a_zero_timeout_takes_the_default_and_a_bad_value_is_refused() {
        let table = mint_sage_ini::BlockTable::corpus_path();
        let document = mint_sage_ini::Document::parse(
            "ingameui.ini",
            "InGameUI\n  \
               FloatingTextTimeOut = 0\n  \
               FloatingTextMoveUpSpeed = sideways\n\
             End\n",
            &table,
        )
        .unwrap();
        let errors = mint_sage_ini::ValueErrors::new();
        let timing = read_timing(std::slice::from_ref(&document), &errors);
        assert_eq!(
            timing.timeout,
            FloatingTextTiming::default().timeout,
            "0 ms is not a 0-frame timeout"
        );
        let refused = errors.take();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].field, "FloatingTextMoveUpSpeed");
    }

    #[test]
    fn a_payment_rises_holds_then_fades_away() {
        let timing = FloatingTextTiming {
            timeout: 30,
            rise: 1.0,
            vanish: 0.1,
        };
        let structure = farm();
        let place = at_origin;
        let text = |amount: i64| format!("+{amount}");
        let mut texts = FloatingTexts::default();
        texts.step(&timing, &[view(structure, 2)], place, text);
        texts.step(&timing, &[view(structure, 1)], place, text);
        assert!(texts.texts.is_empty(), "nothing paid yet");
        texts.step(&timing, &[view(structure, 180)], place, text);
        assert_eq!(texts.texts.len(), 1);
        assert_eq!(texts.texts[0].text, "+25");
        assert_eq!(texts.texts[0].at, Vec3::new(0.0, 0.0, LIFT));
        let mut lived = 0;
        for due_in in (1..180).rev() {
            texts.step(&timing, &[view(structure, due_in)], place, text);
            if texts.texts.is_empty() {
                break;
            }
            lived += 1;
            if lived <= 30 {
                assert_eq!(texts.texts[0].alpha, START_ALPHA, "held until the timeout");
            }
        }
        assert_eq!(lived, 30 + 71);
        assert_eq!(texts.shown, 1);
    }

    #[test]
    fn a_restarted_interval_under_a_new_owner_is_not_a_payment() {
        let timing = FloatingTextTiming::default();
        let structure = farm();
        let place = at_origin;
        let text = |amount: i64| format!("+{amount}");
        let mut texts = FloatingTexts::default();
        texts.step(&timing, &[view(structure, 1)], place, text);
        let captured = IncomeView {
            owner: PlayerId::new(1),
            ..view(structure, 180)
        };
        texts.step(&timing, &[captured], place, text);
        assert!(texts.texts.is_empty());
    }
}
