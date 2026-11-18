use mint_audio::Listener;
use mint_sage_ini::audio::MicrophoneDef;

#[must_use]
pub fn place(def: &MicrophoneDef, eye: [f32; 3], look_at: [f32; 3], right: [f32; 3]) -> Listener {
    let to_ground = sub(look_at, eye);
    let length = norm(to_ground);
    let mut position = look_at;
    if length > f32::EPSILON {
        let mut along = def
            .preferred_fraction_camera_to_ground
            .map_or(length, |fraction| fraction * length);
        if let Some(max) = def.max_distance_to_camera {
            along = along.min(max);
        }
        if let Some(min) = def.min_distance_to_camera {
            along = along.max(min);
        }
        let along = along.clamp(0.0, length);
        let clamped = add(eye, scale(to_ground, along / length));
        let pull = def.pull_towards_look_at.unwrap_or(0.0).clamp(0.0, 1.0);
        position = add(clamped, scale(sub(look_at, clamped), pull));
    }
    Listener {
        position,
        right,
        gain: zoom_gain(def, norm(sub(position, eye))),
    }
}

fn zoom_gain(def: &MicrophoneDef, camera_distance: f32) -> f32 {
    let amount = def.zoom_sound_volume_amount.unwrap_or(0.0).clamp(0.0, 1.0);
    if amount <= 0.0 {
        return 1.0;
    }
    let min = def.zoom_min_distance.unwrap_or(0.0);
    let max = def.zoom_max_distance.unwrap_or(0.0);
    if camera_distance < min {
        1.0
    } else if camera_distance < max {
        1.0 - (camera_distance - min) / (max - min) * amount
    } else {
        1.0 - amount
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn norm(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_audio::mixer::spatial_level;
    use mint_audio::{Rolloff, Spatial};

    fn shipped() -> MicrophoneDef {
        MicrophoneDef {
            preferred_fraction_camera_to_ground: Some(0.86),
            min_distance_to_camera: Some(100.0),
            max_distance_to_camera: Some(300.0),
            pull_towards_look_at: Some(0.6),
            zoom_min_distance: Some(130.0),
            zoom_max_distance: Some(425.0),
            zoom_sound_volume_amount: Some(0.2),
        }
    }

    fn opening_listener() -> Listener {
        let behind = 300.0 / 0.767_327;
        place(
            &shipped(),
            [0.0, -behind, 300.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        )
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 2e-3 * b.abs().max(1.0)
    }

    #[test]
    fn the_microphone_sits_near_the_focus_with_the_zoom_gain_of_its_camera_distance() {
        let listener = opening_listener();
        assert!(
            near(listener.position[1], -61.18),
            "{:?}",
            listener.position
        );
        assert!(near(listener.position[2], 46.95), "{:?}", listener.position);
        assert!(near(listener.gain, 0.8063), "{}", listener.gain);
    }

    #[test]
    fn on_and_off_screen_levels_follow_min_over_distance_and_the_min_volume_floor() {
        let listener = opening_listener();
        let cases = [
            ([0.0, 0.0, 0.0], 1.0, 1.15),
            ([240.0, 0.0, 0.0], 0.6347, 1.05),
            ([0.0, 340.0, 0.0], 0.3961, 1.05),
            ([0.0, 700.0, 0.0], 0.0, 1.05),
        ];
        for (at, effect, voice) in cases {
            let d = norm(sub(at, listener.position));
            let level = |gain: f32, min_gain: f32| {
                let spatial = Spatial {
                    position: at,
                    min_range: 160.0,
                    max_range: 640.0,
                    min_gain,
                };
                spatial_level(Rolloff::InverseClamped, gain, &spatial, d)
            };
            let heard = level(1.0, 0.0);
            assert!(near(heard, effect), "effect at {at:?}: {heard}");
            let heard = level(1.15, 1.05);
            assert!(near(heard, voice), "voice at {at:?}: {heard}");
        }
    }
}
