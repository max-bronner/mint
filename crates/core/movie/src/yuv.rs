pub(crate) const SHADER: &str = include_str!("yuv.wgsl");

fn clamp_byte(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or(u8::MAX)
}

fn channel(sum: i32) -> u8 {
    clamp_byte(sum.saturating_mul(6).saturating_sub(75) / 5)
}

#[must_use]
pub fn convert_reference(y: u8, u: u8, v: u8, alpha_luma: Option<u8>) -> [u8; 4] {
    let (y, u, v) = (i32::from(y), i32::from(u) - 128, i32::from(v) - 128);
    let red = y + (351 * v) / 256;
    let green = y + (-86 * u) / 256 + (-179 * v) / 256;
    let blue = y + (443 * u) / 256;
    let alpha = alpha_luma.map_or(255, |a| clamp_byte((255 * (i32::from(a) - 23)) / 211));
    [channel(red), channel(green), channel(blue), alpha]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_applies_the_scale_and_the_bias_to_known_samples() {
        assert_eq!(convert_reference(128, 128, 128, None), [138, 138, 138, 255]);
        assert_eq!(convert_reference(0, 128, 128, None), [0, 0, 0, 255]);
        assert_eq!(convert_reference(255, 128, 128, None), [255, 255, 255, 255]);
        assert_eq!(convert_reference(100, 128, 255, None), [255, 0, 105, 255]);
        assert_eq!(convert_reference(100, 128, 128, Some(23))[3], 0);
        assert_eq!(convert_reference(100, 128, 128, Some(234))[3], 255);
    }
}
