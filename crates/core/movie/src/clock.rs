#[must_use]
pub fn frame_at(elapsed_ms: i64, numerator: u32, denominator: u32) -> Option<u64> {
    let period = i128::from(denominator).checked_mul(1000)?;
    if period == 0 || numerator == 0 {
        return None;
    }
    let scaled = i128::from(elapsed_ms).checked_mul(i128::from(numerator))?;
    u64::try_from(scaled / period).ok()
}

#[must_use]
pub fn ms_of_frame(frame: u64, numerator: u32, denominator: u32) -> i64 {
    let period = u128::from(denominator).saturating_mul(1000);
    let rate = u128::from(numerator).max(1);
    let ms = u128::from(frame)
        .saturating_mul(period)
        .saturating_add(rate.saturating_sub(1))
        / rate;
    i64::try_from(ms).unwrap_or(i64::MAX)
}
