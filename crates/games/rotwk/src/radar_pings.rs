use crate::radar_events::RadarEventRing;

pub(crate) trait RadarPingHost {
    type Handle;

    fn create(&mut self, clip: &str) -> Self::Handle;
    fn move_ping(&mut self, ping: &Self::Handle, pixel: (f32, f32));
    fn fade(&mut self, ping: &Self::Handle);
}

const MOVE_THRESHOLD_SQ: f32 = 0.5 * 0.5;

struct Tracked<H> {
    serial: u32,
    handle: H,
    last_pixel: (f32, f32),
    faded: bool,
}

pub(crate) struct RadarPingDriver<H> {
    tracked: Vec<Tracked<H>>,
}

impl<H> Default for RadarPingDriver<H> {
    fn default() -> Self {
        Self {
            tracked: Vec::new(),
        }
    }
}

impl<H> RadarPingDriver<H> {
    pub(crate) fn drive<Host: RadarPingHost<Handle = H>>(
        &mut self,
        host: &mut Host,
        ring: &RadarEventRing,
        now: f32,
        mut to_pixel: impl FnMut((f32, f32)) -> Option<(f32, f32)>,
    ) {
        for event in ring.drawable(now) {
            let Some(clip) = event.kind.ping_clip() else {
                continue;
            };
            if self.tracked.iter().any(|t| t.serial == event.serial) {
                continue;
            }
            let handle = host.create(clip);
            let pixel = to_pixel(event.at).unwrap_or((0.0, 0.0));
            host.move_ping(&handle, pixel);
            self.tracked.push(Tracked {
                serial: event.serial,
                handle,
                last_pixel: pixel,
                faded: false,
            });
        }
        self.tracked.retain_mut(|tracked| {
            let Some(event) = ring.drawable(now).find(|e| e.serial == tracked.serial) else {
                return false;
            };
            if let Some(pixel) = to_pixel(event.at) {
                let dx = pixel.0 - tracked.last_pixel.0;
                let dy = pixel.1 - tracked.last_pixel.1;
                if dx * dx + dy * dy >= MOVE_THRESHOLD_SQ {
                    host.move_ping(&tracked.handle, pixel);
                    tracked.last_pixel = pixel;
                }
            }
            if event.fading(now) && !tracked.faded {
                host.fade(&tracked.handle);
                tracked.faded = true;
            }
            true
        });
    }
}

#[cfg(test)]
mod tests;
