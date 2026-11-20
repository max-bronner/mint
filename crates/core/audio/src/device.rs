#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl DeviceFormat {
    pub const CD: Self = Self {
        sample_rate: 44_100,
        channels: 2,
    };
}

pub trait Source: Send + 'static {
    fn render(&mut self, out: &mut [f32], format: DeviceFormat);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeviceStats {
    pub wakes: u64,
    pub underruns: u64,
    pub buffer_us: u64,
    pub max_gap_us: u64,
    pub max_render_us: u64,
}

impl DeviceStats {
    #[must_use]
    pub fn summary(&self) -> String {
        #[allow(clippy::cast_precision_loss)]
        let ms = |us: u64| us as f64 / 1000.0;
        format!(
            "device wakes {} underruns {} | buffer {:.1} ms, longest wake gap {:.1} ms, \
             longest mix {:.2} ms",
            self.wakes,
            self.underruns,
            ms(self.buffer_us),
            ms(self.max_gap_us),
            ms(self.max_render_us),
        )
    }
}

pub trait Device: Send {
    fn format(&self) -> DeviceFormat;

    fn describe(&self) -> String;

    fn stats(&self) -> DeviceStats;
}
