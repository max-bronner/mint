pub(super) const SLOT_ALIGNMENT: u64 = 256;

pub(super) const fn slot_stride(size: u64) -> u64 {
    size.div_ceil(SLOT_ALIGNMENT).saturating_mul(SLOT_ALIGNMENT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PassRegion {
    pub(super) base: u32,
    pub(super) slot_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Regions {
    next: u32,
    capacity: u32,
}

impl Regions {
    pub(super) const fn new(capacity: u32) -> Self {
        Self { next: 0, capacity }
    }

    pub(super) fn claim(&mut self, count: u32) -> Option<u32> {
        let end = self.next.checked_add(count)?;
        if end > self.capacity {
            return None;
        }
        let first = self.next;
        self.next = end;
        Some(first)
    }

    pub(super) fn grown(&self, count: u32) -> u32 {
        self.next.saturating_add(count).saturating_mul(2).max(64)
    }

    pub(super) fn restart(&mut self, capacity: u32) {
        *self = Self::new(capacity);
    }

    pub(super) fn reset(&mut self) {
        self.next = 0;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Tally {
    passes: u32,
    submits: u32,
    flushes: u32,
}

pub(super) struct FrameEncoder {
    encoder: Option<wgpu::CommandEncoder>,
    staging: Option<wgpu::Buffer>,
    staging_capacity: u64,
    staging_next: u64,
    pub(super) instances: Regions,
    pub(super) casters: Regions,
    pub(super) frame_slots: Regions,
    pub(super) light_slots: Regions,
    current: Tally,
    last: Tally,
}

impl FrameEncoder {
    pub(super) const fn new(
        instances: u32,
        casters: u32,
        frame_slots: u32,
        light_slots: u32,
    ) -> Self {
        Self {
            encoder: None,
            staging: None,
            staging_capacity: 0,
            staging_next: 0,
            instances: Regions::new(instances),
            casters: Regions::new(casters),
            frame_slots: Regions::new(frame_slots),
            light_slots: Regions::new(light_slots),
            current: Tally {
                passes: 0,
                submits: 0,
                flushes: 0,
            },
            last: Tally {
                passes: 0,
                submits: 0,
                flushes: 0,
            },
        }
    }

    pub(super) fn take(&mut self, device: &wgpu::Device) -> wgpu::CommandEncoder {
        self.current.passes = self.current.passes.saturating_add(1);
        self.encoder.take().unwrap_or_else(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mint frame"),
            })
        })
    }

    pub(super) fn keep(&mut self, encoder: wgpu::CommandEncoder) {
        self.encoder = Some(encoder);
    }

    pub(super) fn submit(&mut self, queue: &wgpu::Queue, encoder: wgpu::CommandEncoder) {
        queue.submit(std::iter::once(encoder.finish()));
        self.current.submits = self.current.submits.saturating_add(1);
        self.staging_next = 0;
        self.instances.reset();
        self.casters.reset();
        self.frame_slots.reset();
        self.light_slots.reset();
    }

    pub(super) fn submit_open(&mut self, queue: &wgpu::Queue) -> bool {
        match self.encoder.take() {
            Some(encoder) => {
                self.submit(queue, encoder);
                true
            }
            None => false,
        }
    }

    pub(super) fn flush(&mut self, queue: &wgpu::Queue) {
        if self.submit_open(queue) {
            self.current.flushes = self.current.flushes.saturating_add(1);
        }
    }

    pub(super) fn end_frame(&mut self) {
        self.last = std::mem::take(&mut self.current);
    }

    pub(super) const fn submits(&self) -> (u32, u32) {
        (self.last.submits, self.last.flushes)
    }

    pub(super) const fn passes(&self) -> u32 {
        self.last.passes
    }

    pub(super) fn write_buffer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        destination: &wgpu::Buffer,
        offset: u64,
        data: &[u8],
    ) {
        let Some(encoder) = self.encoder.as_mut() else {
            queue.write_buffer(destination, offset, data);
            return;
        };
        let Ok(len) = u64::try_from(data.len()) else {
            return;
        };
        if len == 0 {
            return;
        }
        let mut at = self.staging_next;
        let fits = at
            .checked_add(len)
            .is_some_and(|end| end <= self.staging_capacity);
        if !fits || self.staging.is_none() {
            let capacity = at.saturating_add(len).saturating_mul(2).max(64 * 1024);
            self.staging = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mint ordered writes"),
                size: capacity,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.staging_capacity = capacity;
            at = 0;
        }
        let Some(staging) = &self.staging else {
            return;
        };
        queue.write_buffer(staging, at, data);
        encoder.copy_buffer_to_buffer(staging, at, destination, offset, len);
        self.staging_next = at
            .saturating_add(len)
            .div_ceil(wgpu::COPY_BUFFER_ALIGNMENT)
            .saturating_mul(wgpu::COPY_BUFFER_ALIGNMENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_claimed_in_one_frame_never_overlap() {
        let mut regions = Regions::new(100);
        let passes = [7u32, 0, 31, 1, 40, 21];
        let mut claimed: Vec<(u32, u32)> = Vec::new();
        for count in passes {
            let first = regions.claim(count).expect("fits");
            for &(start, end) in &claimed {
                assert!(
                    first.saturating_add(count) <= start || first >= end,
                    "pass [{first}, +{count}) overlaps [{start}, {end})"
                );
            }
            claimed.push((first, first.saturating_add(count)));
        }
        assert_eq!(regions.claim(1), None, "100 units are all claimed");
    }

    #[test]
    fn a_claim_past_the_end_asks_for_a_buffer_that_holds_the_whole_frame() {
        let mut regions = Regions::new(64);
        assert_eq!(regions.claim(50), Some(0));
        assert_eq!(regions.claim(20), None);
        let capacity = regions.grown(20);
        assert!(capacity >= 70, "the next frame's 70 units fit: {capacity}");
        regions.restart(capacity);
        assert_eq!(regions.claim(20), Some(0), "a new buffer starts empty");
    }

    #[test]
    fn only_a_reset_frees_a_region() {
        let mut regions = Regions::new(10);
        assert_eq!(regions.claim(10), Some(0));
        assert_eq!(regions.claim(1), None);
        regions.reset();
        assert_eq!(regions.claim(10), Some(0));
    }

    #[test]
    fn slots_are_256_byte_aligned() {
        assert_eq!(slot_stride(1040), 1280);
        assert_eq!(slot_stride(112), 256);
        assert_eq!(slot_stride(256), 256);
    }
}
