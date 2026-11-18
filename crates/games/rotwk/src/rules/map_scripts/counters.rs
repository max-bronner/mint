use crate::game_data::RETAIL_LOGIC_HZ;

pub(super) const TICKS_PER_SCRIPT_FRAME: u64 = (mint_sim::SIMULATION_HZ / RETAIL_LOGIC_HZ) as u64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SideKey {
    pub(super) side: String,
    pub(super) name: String,
}

impl SideKey {
    pub(super) fn qualify(raw: &str, current_side: &str) -> Self {
        match raw.split_once('/') {
            Some((side, name)) => Self {
                side: side.to_owned(),
                name: name.to_owned(),
            },
            None => Self {
                side: current_side.to_owned(),
                name: raw.to_owned(),
            },
        }
    }
}

pub(super) fn seconds_to_script_frames(seconds: f32) -> i32 {
    let frames_per_ms = RETAIL_LOGIC_HZ as f32 / 1000.0;
    let product = f64::from(seconds.max(0.0)) * f64::from(frames_per_ms);
    #[allow(clippy::cast_possible_truncation)]
    let frames = (product * 1000.0).ceil() as i32;
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_slash_and_side_names_resolve_to_one_key_per_side() {
        let own = SideKey::qualify("X", "PlyrA");
        assert_eq!((own.side.as_str(), own.name.as_str()), ("PlyrA", "X"));
        assert_eq!(own, SideKey::qualify("PlyrA/X", "PlyrB"));
        assert_ne!(own, SideKey::qualify("X", "PlyrB"));
        let neutral = SideKey::qualify("/X", "PlyrA");
        assert_eq!((neutral.side.as_str(), neutral.name.as_str()), ("", "X"));
        let nested = SideKey::qualify("A/B/C", "PlyrA");
        assert_eq!((nested.side.as_str(), nested.name.as_str()), ("A", "B/C"));
    }

    #[test]
    fn seconds_convert_with_the_retail_f32_expression_not_ceil_of_five_times() {
        let mut differ = 0;
        for tenth in 1..=3000_i32 {
            let seconds = tenth as f32 / 10.0;
            let exact = (tenth * 5 + 9) / 10;
            let frames = seconds_to_script_frames(seconds);
            assert!(
                frames == exact || frames == exact + 1,
                "{seconds}: {frames}"
            );
            differ += i32::from(frames != exact);
        }
        assert_eq!(differ, 332);
        assert_eq!(seconds_to_script_frames(0.6), 4);
        assert_eq!(seconds_to_script_frames(1.2), 7);
        assert_eq!(seconds_to_script_frames(2.4), 13);
        assert_eq!(seconds_to_script_frames(1.0), 5);
    }
}
