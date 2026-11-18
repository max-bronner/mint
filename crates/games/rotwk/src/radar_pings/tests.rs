use super::*;
use crate::radar_events::{RadarEventRing, RadarEventType};

#[derive(Default)]
struct RecordingHost {
    created: u32,
    moved: u32,
    faded: u32,
    next_handle: u32,
}

impl RadarPingHost for RecordingHost {
    type Handle = u32;

    fn create(&mut self, _clip: &str) -> Self::Handle {
        self.created += 1;
        self.next_handle += 1;
        self.next_handle
    }

    fn move_ping(&mut self, _ping: &Self::Handle, _pixel: (f32, f32)) {
        self.moved += 1;
    }

    fn fade(&mut self, _ping: &Self::Handle) {
        self.faded += 1;
    }
}

#[test]
fn a_static_pingable_event_creates_once_and_moves_once() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::UnderAttack, (100.0, 200.0), 0.0);
    let mut driver = RadarPingDriver::default();
    let mut host = RecordingHost::default();
    for frame in 0_i16..10 {
        driver.drive(&mut host, &ring, f32::from(frame) * 0.1, Some);
    }
    assert_eq!(host.created, 1);
    assert_eq!(host.moved, 1, "no further move - the event never moved");
    assert_eq!(host.faded, 0);
}

#[test]
fn fade_fires_once_at_the_fade_point() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::UnderAttack, (0.0, 0.0), 0.0);
    let mut driver = RadarPingDriver::default();
    let mut host = RecordingHost::default();
    driver.drive(&mut host, &ring, 0.0, Some);
    driver.drive(&mut host, &ring, 3.4, Some);
    assert_eq!(host.faded, 0, "not fading yet");
    driver.drive(&mut host, &ring, 3.6, Some);
    assert_eq!(host.faded, 1);
    driver.drive(&mut host, &ring, 3.9, Some);
    assert_eq!(host.faded, 1, "not called twice");
}

#[test]
fn a_type_with_no_ping_clip_is_never_created() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::Infiltration, (0.0, 0.0), 0.0);
    let mut driver = RadarPingDriver::default();
    let mut host = RecordingHost::default();
    driver.drive(&mut host, &ring, 0.0, Some);
    assert_eq!(host.created, 0);
}

#[test]
fn a_move_past_the_threshold_calls_move_again() {
    let mut ring = RadarEventRing::default();
    ring.create(RadarEventType::UnderAttack, (0.0, 0.0), 0.0);
    let mut driver = RadarPingDriver::default();
    let mut host = RecordingHost::default();
    driver.drive(&mut host, &ring, 0.0, |_| Some((0.0, 0.0)));
    assert_eq!(host.moved, 1);
    driver.drive(&mut host, &ring, 0.1, |_| Some((0.6, 0.0)));
    assert_eq!(host.moved, 2, "0.6 px - past the threshold");
    driver.drive(&mut host, &ring, 0.2, |_| Some((0.8, 0.0)));
    assert_eq!(host.moved, 2, "0.2 px further - under the threshold");
}
