pub const RALLY_FALLBACK_X: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkOnAnchor {
    pub start: [f32; 3],
    pub rally: [f32; 3],
    pub angle: f32,
}

#[must_use]
pub fn walk_on_anchor(start: Option<[f32; 3]>, rally: Option<[f32; 3]>) -> WalkOnAnchor {
    let (start, rally) = match (start, rally) {
        (Some(start), Some(rally)) => (start, rally),
        (Some([x, y, z]), None) => ([x + RALLY_FALLBACK_X, y, z], [x, y, z]),
        (None, _) => ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
    };
    let angle = mint_math::f32::atan2(rally[1] - start[1], rally[0] - start[0]);
    WalkOnAnchor {
        start,
        rally,
        angle,
    }
}
