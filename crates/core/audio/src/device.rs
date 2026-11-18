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

pub trait Device: Send {
    fn format(&self) -> DeviceFormat;

    fn describe(&self) -> String;
}
