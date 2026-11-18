use std::collections::BTreeMap;

use super::{tick, Stream};

fn markers(pairs: &[(&str, f32)]) -> Vec<(String, f32)> {
    pairs.iter().map(|(n, l)| ((*n).to_string(), *l)).collect()
}

#[test]
fn fresh_markers_fill_up_to_the_clamped_limit() {
    let mut states = BTreeMap::new();
    let changes = tick(
        &mut states,
        &markers(&[("a", 0.9), ("b", 0.8), ("c", 0.7)]),
        3,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert_eq!(changes.started, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(states.len(), 2);
}

#[test]
fn a_louder_candidate_displaces_the_weakest_only_past_hysteresis() {
    let mut states = BTreeMap::new();
    states.insert(
        "weak".to_string(),
        Stream {
            level: 0.50,
            fading: false,
            timer_ms: 0.0,
        },
    );
    states.insert(
        "strong".to_string(),
        Stream {
            level: 0.80,
            fading: false,
            timer_ms: 0.0,
        },
    );

    let below = tick(
        &mut states,
        &markers(&[("weak", 0.50), ("strong", 0.80), ("new", 0.53)]),
        2,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert!(below.started.is_empty(), "0.53 must not clear 0.50 + 0.05");

    let above = tick(
        &mut states,
        &markers(&[("weak", 0.50), ("strong", 0.80), ("new", 0.56)]),
        2,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert_eq!(above.started, vec!["new".to_string()]);
    assert!(
        states["weak"].fading,
        "the displaced stream fades rather than stopping at once"
    );
}

#[test]
fn duplicate_markers_of_one_stream_keep_only_the_louder_level() {
    let mut states = BTreeMap::new();
    tick(
        &mut states,
        &markers(&[("a", 0.3), ("a", 0.5)]),
        2,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert_eq!(states["a"].level, 0.5);
}

#[test]
fn a_displaced_stream_stops_once_its_fade_crosses_the_floor() {
    let mut states = BTreeMap::new();
    states.insert(
        "weak".to_string(),
        Stream {
            level: 0.50,
            fading: false,
            timer_ms: 0.0,
        },
    );
    tick(
        &mut states,
        &markers(&[("weak", 0.50), ("new", 0.80)]),
        1,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert!(states["weak"].fading);

    let changes = tick(
        &mut states,
        &markers(&[("new", 0.80)]),
        1,
        0.05,
        0.02,
        2_000.0,
        2_000.0,
    );
    assert_eq!(changes.stopped, vec!["weak".to_string()]);
    assert!(!states.contains_key("weak"));
}

#[test]
fn zero_maximum_streams_admits_nothing() {
    let mut states = BTreeMap::new();
    let changes = tick(
        &mut states,
        &markers(&[("a", 0.9)]),
        0,
        0.05,
        0.02,
        2_000.0,
        16.0,
    );
    assert!(changes.started.is_empty());
    assert!(states.is_empty());
}
