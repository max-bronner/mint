use mint_render::{Renderer, Vec3};
use mint_sage_asset::KindOf;
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
    for (id, placement, decal) in state.units.state_decals() {
        if obscured.binary_search(&id).is_ok() {
            continue;
        }
        let Some(texture) = state.visuals.ground_texture(
            renderer,
            &state.vfs,
            &state.scene.resolver,
            &decal.texture,
        ) else {
            continue;
        };
        let [forward, _, _, origin] = placement.to_cols();
        let view = crate::visuals::DecalView {
            centre: Vec3::new(
                origin[0],
                origin[1],
                ground_at(&state.scene, origin[0], origin[1]),
            ),
            half_x: decal.size_x / 2.0,
            half_y: decal.size_y / 2.0,
            facing: mint_math::f32::atan2(forward[1], forward[0]),
            texture,
            tint: [1.0; 4],
        };
        match decal.blend {
            mint_sage_ini::DecalBlend::Alpha => decals.push(view),
            mint_sage_ini::DecalBlend::Additive => additive_decals.push(view),
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
