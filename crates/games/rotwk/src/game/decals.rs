use mint_render::{Renderer, Vec3};
use mint_sage_asset::KindOf;
use mint_sage_ini::DecalShader;
use mint_sage_scene::dynamic_decal::{DecalTimeline, FiredDecal};
use mint_sim::PlayerId;

use super::claim_outline::{self, ClaimCircle};
use super::{footprint_bounds, ground_at, placement_at, MatchState};
use crate::render_state::UnitView;
use crate::unit::Kind;

const ILLEGAL_PLACEMENT_BIB_TEXTURE: &str = "TBRedBib";

pub(super) fn push_ground_decals(
    renderer: &mut Renderer,
    state: &mut MatchState,
    units: &[UnitView],
    selector: PlayerId,
) {
    let RankDecals {
        mut decals,
        merge_masks,
        merged,
    } = rank_decals(renderer, state, units, selector);
    for cursor in [state.visuals.radius_cursor, state.visuals.claim_ring]
        .into_iter()
        .flatten()
    {
        if let Some(texture) = cursor.texture {
            decals.push(crate::visuals::DecalView {
                centre: cursor.centre,
                half_x: cursor.radius,
                half_y: cursor.radius,
                facing: 0.0,
                texture,
                tint: cursor.tint,
            });
        }
    }
    let rules = state.simulation.rules();
    let placing = state.visuals.claim_ring.is_some();
    let claims = selection_claims(
        units,
        selector,
        |owner| rules.allied(selector, owner),
        placing,
        |unit| {
            let record = &state.kinds.get(Kind(unit.kind?))?.record;
            record
                .claim_radius
                .filter(|r| r.is_finite() && *r > 0.0 && record.claim_decal_visible)
        },
    );
    let plans = if claims.is_empty() && !placing {
        Vec::new()
    } else {
        rules.shown_plan_claims(state.simulation.state(), selector)
    };
    let circles: Vec<_> = claims
        .iter()
        .map(|(unit, radius)| (unit.owner, unit.position.x, unit.position.y, *radius))
        .chain(
            plans
                .iter()
                .map(|(owner, centre, radius)| (*owner, centre.x, centre.y, *radius)),
        )
        .map(|(owner, x, y, radius)| {
            let [r, g, b, _] = state.player_colors.of_or_white(owner);
            ClaimCircle {
                centre: [x, y],
                radius,
                colour: [r, g, b],
            }
        })
        .collect();
    if circles.len() != state.visuals.selection_claims {
        crate::diag::diag!(
            "claims: the selection shows {} claim decal(s), {} of them plans",
            circles.len(),
            plans.len()
        );
        state.visuals.selection_claims = circles.len();
    }
    if circles != state.visuals.claim_outline.0 {
        let segments = claim_outline::outline(&circles);
        state.visuals.claim_outline = (circles, segments);
    }
    let mut additive_decals = Vec::new();
    if !state.visuals.claim_outline.1.is_empty() {
        if let Some(texture) = state.visuals.ground_texture(
            renderer,
            &state.vfs,
            &state.scene.resolver,
            claim_outline::SEGMENT_TEXTURE,
        ) {
            for segment in &state.visuals.claim_outline.1 {
                let [x, y] = segment.centre;
                let [r, g, b] = segment.colour;
                additive_decals.push(crate::visuals::DecalView {
                    centre: Vec3::new(x, y, ground_at(&state.scene, x, y)),
                    half_x: segment.side / 2.0,
                    half_y: segment.side / 2.0,
                    facing: segment.facing,
                    texture,
                    tint: [r, g, b, 1.0],
                });
            }
        }
    }
    let obscured = obscured_ids(units);
    let now = state.visuals.fading_decals.now();
    let shadow_decals = *state
        .visuals
        .shadow_decals
        .get_or_insert_with(|| crate::scene::static_lod_shadow_decals(&state.vfs));
    let mut shown = Vec::new();
    for (id, placement, decal) in state.units.state_decals() {
        if obscured.binary_search(&id).is_ok() || !(shadow_decals || decal.override_lod_visibility)
        {
            continue;
        }
        let [forward, _, _, origin] = placement.to_cols();
        shown.push(ShownDecal {
            id,
            decal: decal.clone(),
            centre: Vec3::new(
                origin[0],
                origin[1],
                ground_at(&state.scene, origin[0], origin[1]),
            ),
            facing: mint_math::f32::atan2(forward[1], forward[0]),
        });
    }
    let shadows = state.visuals.fading_decals.shadows(now, shown);
    for (shown, opacity) in shadows {
        let Some(texture) = state.visuals.ground_texture(
            renderer,
            &state.vfs,
            &state.scene.resolver,
            &shown.decal.texture,
        ) else {
            continue;
        };
        let additive = shown.decal.blend == mint_sage_ini::DecalBlend::Additive;
        let view = crate::visuals::DecalView {
            centre: shown.centre,
            half_x: shown.decal.size_x / 2.0,
            half_y: shown.decal.size_y / 2.0,
            facing: shown.facing,
            texture,
            tint: opacity_tint([1.0; 3], opacity, additive),
        };
        if additive {
            additive_decals.push(view);
        } else {
            decals.push(view);
        }
    }
    let fired = state.visuals.fading_decals.fired(now);
    for (fired, opacity) in fired {
        let decal = &fired.decal;
        let Some(texture) = state.visuals.ground_texture(
            renderer,
            &state.vfs,
            &state.scene.resolver,
            &decal.texture,
        ) else {
            continue;
        };
        let (x, y) = (fired.centre.x, fired.centre.y);
        let colour = decal.color.map(|channel| channel.clamp(0.0, 255.0) / 255.0);
        let view = crate::visuals::DecalView {
            centre: Vec3::new(x, y, ground_at(&state.scene, x, y)),
            half_x: decal.size / 2.0,
            half_y: decal.size / 2.0,
            facing: fired.facing,
            texture,
            tint: opacity_tint(colour, opacity, decal.shader != DecalShader::Alpha),
        };
        match decal.shader {
            DecalShader::Alpha => decals.push(view),
            DecalShader::Additive => additive_decals.push(view),
            DecalShader::Subtract => {}
        }
    }
    for item in &state.visuals.formation_preview.clone() {
        if let Some(texture) =
            state
                .visuals
                .ground_texture(renderer, &state.vfs, &state.scene.resolver, &item.texture)
        {
            decals.push(crate::visuals::DecalView {
                centre: Vec3::new(
                    item.centre.x,
                    item.centre.y,
                    ground_at(&state.scene, item.centre.x, item.centre.y),
                ),
                half_x: item.half_width,
                half_y: item.half_height,
                facing: item.facing,
                texture,
                tint: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }
    for piece in &state.visuals.formation_arrow.clone() {
        if let Some(texture) = state.visuals.ground_texture(
            renderer,
            &state.vfs,
            &state.scene.resolver,
            &piece.texture,
        ) {
            decals.push(crate::visuals::DecalView {
                centre: Vec3::new(
                    piece.centre.x,
                    piece.centre.y,
                    ground_at(&state.scene, piece.centre.x, piece.centre.y),
                ),
                half_x: piece.size_x / 2.0,
                half_y: piece.size_y / 2.0,
                facing: piece.angle,
                texture,
                tint: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }
    if let Some(ghost) = state.visuals.placement_ghost.filter(|g| g.code != 0) {
        let bounds = state
            .kinds
            .get(Kind(ghost.kind))
            .and_then(|entry| footprint_bounds(&crate::construction::footprint_of(&entry.record)));
        if let Some((centre, half)) = bounds {
            if let Some(texture) = state.visuals.ground_texture(
                renderer,
                &state.vfs,
                &state.scene.resolver,
                ILLEGAL_PLACEMENT_BIB_TEXTURE,
            ) {
                let at = placement_at(ghost.x, ghost.y, 0.0, ghost.facing)
                    .transform_point(Vec3::new(centre[0], centre[1], 0.0));
                decals.push(crate::visuals::DecalView {
                    centre: Vec3::new(at.x, at.y, ground_at(&state.scene, at.x, at.y)),
                    half_x: half[0],
                    half_y: half[1],
                    facing: ghost.facing,
                    texture,
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
            }
        }
    }
    state.visuals.push_decals(&decals, &mut state.scene.drawn);
    state.visuals.push_decals_as(
        &additive_decals,
        mint_render::Blend::Additive,
        &mut state.scene.drawn,
    );
    state
        .visuals
        .push_merge_masks(&merge_masks, &mut state.scene.drawn);
    state.visuals.push_decals_as(
        &merged,
        mint_render::Blend::MergeAlphaBlend,
        &mut state.scene.drawn,
    );
}

fn opacity_tint(colour: [f32; 3], opacity: i64, additive: bool) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)]
    let alpha = opacity.clamp(0, 255) as f32 / 255.0;
    let [r, g, b] = colour;
    if additive {
        [r * alpha, g * alpha, b * alpha, 1.0]
    } else {
        [r, g, b, alpha]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShownDecal {
    id: u32,
    decal: mint_sage_ini::StateDecal,
    centre: Vec3,
    facing: f32,
}

#[derive(Debug, Default)]
pub(crate) struct FadingDecals {
    seconds: f64,
    shown: Vec<(ShownDecal, DecalTimeline)>,
    leaving: Vec<(ShownDecal, DecalTimeline)>,
    fired: Vec<(FiredDecal, DecalTimeline)>,
}

const MAX_FIRED_DECALS: usize = 512;

impl FadingDecals {
    pub(crate) fn set_clock(&mut self, seconds: f32) {
        if seconds.is_finite() {
            self.seconds = self.seconds.max(f64::from(seconds));
        }
    }

    pub(crate) fn now(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (self.seconds * 30.0).clamp(0.0, f64::from(u32::MAX)) as u32;
        frames
    }

    pub(crate) fn fire(&mut self, now: u32, decals: impl IntoIterator<Item = FiredDecal>) {
        for decal in decals {
            if self.fired.len() >= MAX_FIRED_DECALS {
                self.fired.remove(0);
            }
            let timeline = DecalTimeline::fired(now, &decal.decal);
            self.fired.push((decal, timeline));
        }
    }

    fn fired(&mut self, now: u32) -> Vec<(FiredDecal, i64)> {
        self.fired
            .retain(|(_, timeline)| timeline.opacity_at(now).is_some());
        self.fired
            .iter()
            .filter_map(|(decal, timeline)| Some((decal.clone(), timeline.opacity_at(now)?)))
            .collect()
    }

    fn shadows(&mut self, now: u32, shown: Vec<ShownDecal>) -> Vec<(ShownDecal, i64)> {
        let same = |a: &ShownDecal, b: &ShownDecal| a.id == b.id && a.decal == b.decal;
        let mut next = Vec::with_capacity(shown.len());
        for decal in shown {
            let timeline = self
                .shown
                .iter()
                .find(|(held, _)| same(held, &decal))
                .map_or_else(
                    || DecalTimeline::shadow(now, &decal.decal.opacity),
                    |(_, timeline)| *timeline,
                );
            next.push((decal, timeline));
        }
        for (held, timeline) in std::mem::take(&mut self.shown) {
            if next.iter().any(|(decal, _)| same(decal, &held)) {
                continue;
            }
            if let Some(leaving) = timeline.removed(now, held.decal.opacity.dynamic) {
                self.leaving.push((held, leaving));
            }
        }
        self.shown = next;
        self.leaving
            .retain(|(_, timeline)| timeline.opacity_at(now).is_some());
        self.shown
            .iter()
            .chain(&self.leaving)
            .filter_map(|(decal, timeline)| Some((decal.clone(), timeline.opacity_at(now)?)))
            .collect()
    }
}

fn obscured_ids(units: &[UnitView]) -> Vec<u32> {
    let mut ids: Vec<u32> = units
        .iter()
        .filter(|unit| unit.visibility <= mint_sage_shroud::ObjectVisibility::Fogged)
        .map(|unit| unit.entity.index())
        .collect();
    ids.sort_unstable();
    ids
}

struct RankDecals {
    decals: Vec<crate::visuals::DecalView>,
    merge_masks: Vec<crate::visuals::DecalView>,
    merged: Vec<crate::visuals::DecalView>,
}

fn selection_claims(
    units: &[UnitView],
    selector: PlayerId,
    allied: impl Fn(PlayerId) -> bool,
    placing: bool,
    claim: impl Fn(&UnitView) -> Option<f32>,
) -> Vec<(&UnitView, f32)> {
    let claims: Vec<_> = units
        .iter()
        .filter(|unit| {
            (unit.owner == selector || allied(unit.owner))
                && !unit.dying
                && unit.visibility != mint_sage_shroud::ObjectVisibility::Fogged
        })
        .filter_map(|unit| claim(unit).map(|radius| (unit, radius)))
        .collect();
    if placing || claims.iter().any(|(unit, _)| unit.selected) {
        claims
    } else {
        Vec::new()
    }
}

fn gets_selection_decal(kind_of: KindOf, own: bool, is_horde: bool) -> bool {
    own && !is_horde && !kind_of.contains(KindOf::STRUCTURE)
}

fn rank_decals(
    renderer: &mut Renderer,
    state: &mut MatchState,
    units: &[UnitView],
    selector: PlayerId,
) -> RankDecals {
    let mut out = RankDecals {
        decals: Vec::new(),
        merge_masks: Vec::new(),
        merged: Vec::new(),
    };
    let Some(ids) = state.simulation.rules().ids() else {
        return out;
    };
    let world = state.simulation.state();
    let rules = state.simulation.rules();
    let frame = state.simulation.frame().get();
    type Held<'a> = (
        &'a mint_sage_modules::SelectionDecal,
        Option<mint_render::TextureHandle>,
        Option<mint_render::TextureHandle>,
    );
    let mut textures: Vec<Held> = Vec::new();
    for unit in units.iter().filter(|unit| {
        unit.selected && unit.visibility != mint_sage_shroud::ObjectVisibility::Fogged
    }) {
        let own = unit.owner == selector;
        let is_horde = world.contains(ids.horde, unit.entity);
        if !gets_selection_decal(unit.kind_of, own, is_horde) {
            continue;
        }
        let Some((_, decal)) = rules.rank_decal(world, unit.entity) else {
            continue;
        };
        let Some(half) = crate::visuals::selection_decal_width(decal, 1) else {
            continue;
        };
        let merges = decal.merges();
        let (texture, mask) = match textures
            .iter()
            .find(|(held, ..)| std::ptr::eq(*held, decal))
        {
            Some((_, texture, mask)) => (*texture, *mask),
            None => {
                let mut load = |name: &str, cut_out: Option<&str>| {
                    state.visuals.decal_texture(
                        renderer,
                        &state.vfs,
                        &state.scene.resolver,
                        name,
                        cut_out,
                    )
                };
                let (texture, mask) = if merges {
                    (
                        load(&decal.texture, None),
                        decal.texture2.as_deref().and_then(|name| load(name, None)),
                    )
                } else {
                    (load(&decal.texture, decal.texture2.as_deref()), None)
                };
                textures.push((decal, texture, mask));
                (texture, mask)
            }
        };
        let Some(texture) = texture else {
            continue;
        };
        let house = state.player_colors.of_or_white(unit.owner);
        let (x, y) = (unit.position.x, unit.position.y);
        let centre = Vec3::new(x, y, ground_at(&state.scene, x, y));
        if let Some(mask) = mask {
            out.merge_masks.push(crate::visuals::DecalView {
                centre,
                half_x: half,
                half_y: half,
                facing: 0.0,
                texture: mask,
                tint: [1.0; 4],
            });
        }
        let drawn = if merges {
            &mut out.merged
        } else {
            &mut out.decals
        };
        drawn.push(crate::visuals::DecalView {
            centre,
            half_x: half,
            half_y: half,
            facing: 0.0,
            texture,
            tint: [
                house[0],
                house[1],
                house[2],
                crate::visuals::throb_opacity(
                    decal.opacity,
                    decal.throb_ms,
                    frame,
                    mint_sim::SIMULATION_HZ,
                ),
            ],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pick::tests::view;

    fn claimant(index: u32, selected: bool) -> UnitView {
        UnitView {
            selected,
            ..view(index, 0.0, 0.0, KindOf::STRUCTURE, 0)
        }
    }

    #[test]
    fn only_fogged_and_shrouded_drawables_lose_their_state_decals() {
        use mint_sage_shroud::ObjectVisibility::{Clear, Fogged, PartialClear, Shrouded};
        let units: Vec<UnitView> = [Clear, Shrouded, PartialClear, Fogged]
            .into_iter()
            .zip(0u32..)
            .map(|(visibility, index)| UnitView {
                visibility,
                ..claimant(index, false)
            })
            .collect();
        let mut expected = vec![units[1].entity.index(), units[3].entity.index()];
        expected.sort_unstable();
        assert_eq!(obscured_ids(&units), expected);
    }

    #[test]
    fn only_an_own_non_horde_non_structure_object_gets_a_selection_decal() {
        assert!(gets_selection_decal(KindOf::NONE, true, false));
        assert!(!gets_selection_decal(KindOf::STRUCTURE, true, false));
        assert!(!gets_selection_decal(KindOf::NONE, true, true));
        assert!(!gets_selection_decal(KindOf::NONE, false, false));
    }

    #[test]
    fn a_selected_visible_claimant_shows_every_visible_claim_its_owner_holds() {
        let units = [
            claimant(0, true),
            claimant(1, false),
            claimant(2, false),
            UnitView {
                owner: PlayerId::new(1),
                ..claimant(3, false)
            },
        ];
        let visible = |unit: &UnitView| (unit.entity != units[2].entity).then_some(80.0);
        let shown: Vec<_> = selection_claims(&units, PlayerId::new(0), |_| false, false, visible)
            .into_iter()
            .map(|(unit, _)| unit.entity)
            .collect();
        assert_eq!(shown, vec![units[0].entity, units[1].entity]);
    }

    #[test]
    fn a_selection_without_a_visible_claimant_shows_no_claims() {
        let units = [claimant(0, true), claimant(1, false)];
        let visible = |unit: &UnitView| (unit.entity == units[1].entity).then_some(80.0);
        assert!(selection_claims(&units, PlayerId::new(0), |_| false, false, visible).is_empty());
    }

    #[test]
    fn placing_a_claimant_shows_every_visible_claim_with_nothing_selected() {
        let units = [claimant(0, false), claimant(1, false)];
        let visible = |_: &UnitView| Some(80.0);
        let shown: Vec<_> = selection_claims(&units, PlayerId::new(0), |_| false, true, visible)
            .into_iter()
            .map(|(unit, _)| unit.entity)
            .collect();
        assert_eq!(shown, vec![units[0].entity, units[1].entity]);
    }
}
