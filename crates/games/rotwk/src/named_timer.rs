use mint_sage_ini::value::{parse_float, parse_int, tagged_value};

use crate::rules::map_scripts::TimerView;

const SCRIPT_FRAMES_PER_SECOND: i32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimerStyle {
    pub(crate) position: [f32; 2],
    pub(crate) normal: [f32; 4],
    pub(crate) ready: [f32; 4],
    pub(crate) points: f32,
}

impl Default for TimerStyle {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            normal: [1.0; 4],
            ready: [1.0; 4],
            points: 10.0,
        }
    }
}

pub(crate) fn read_style(documents: &[mint_sage_ini::Document]) -> TimerStyle {
    let mut style = TimerStyle::default();
    let blocks = documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"));
    for block in blocks {
        let colour = |keyword: &str| {
            let args = block.field(keyword)?.args.as_slice();
            let channel = |tag: &str| {
                tagged_value(args, tag)
                    .and_then(parse_int)
                    .map(|v| v.clamp(0, 255) as f32 / 255.0)
            };
            Some([channel("R")?, channel("G")?, channel("B")?, 1.0])
        };
        if let Some(c) = colour("NamedTimerCountdownNormalColor") {
            style.normal = c;
        }
        if let Some(c) = colour("NamedTimerCountdownReadyColor") {
            style.ready = c;
        }
        if let Some(args) = block
            .field("NamedTimerCountdownPosition")
            .map(|field| field.args.as_slice())
        {
            let axis = |tag: &str| tagged_value(args, tag).and_then(parse_float);
            if let (Some(x), Some(y)) = (axis("X"), axis("Y")) {
                style.position = [x, y];
            }
        }
        if let Some(points) = block
            .field("NamedTimerCountdownNormalPointSize")
            .and_then(mint_sage_ini::Field::value)
            .and_then(parse_float)
            .filter(|p| p.is_finite() && *p > 0.0)
        {
            style.points = points;
        }
    }
    style
}

pub(crate) fn line_text(label: &str, timer: &TimerView) -> String {
    if timer.countdown {
        let seconds = timer.value.max(0) / SCRIPT_FRAMES_PER_SECOND;
        format!("{label} {}:{:02}", seconds / 60, seconds % 60)
    } else {
        format!("{label} {}", timer.value)
    }
}

pub(crate) fn is_ready(timer: &TimerView) -> bool {
    timer.countdown && timer.value.max(0) / SCRIPT_FRAMES_PER_SECOND == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(countdown: bool, value: i32) -> TimerView {
        TimerView {
            label: String::new(),
            countdown,
            value,
        }
    }

    #[test]
    fn a_countdown_shows_minutes_and_whole_seconds_of_script_frames() {
        assert_eq!(line_text("T", &timer(true, 5 * 75)), "T 1:15");
        assert_eq!(line_text("T", &timer(true, 5 * 9)), "T 0:09");
        assert_eq!(line_text("N", &timer(false, 42)), "N 42");
        assert!(is_ready(&timer(true, 4)) && !is_ready(&timer(false, 0)));
    }
}
