pub(crate) const IMAGE: &str = "TitleScreen";

const RING: [f32; 4] = [0.467_773_44, 0.841_145_8, 0.530_273_44, 0.924_479_2];

pub(crate) fn ring_rect(window: (f32, f32)) -> [f32; 4] {
    let (w, h) = window;
    [RING[0] * w, RING[1] * h, RING[2] * w, RING[3] * h]
}
