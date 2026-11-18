#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub rate_numerator: u32,
    pub rate_denominator: u32,
    pub has_alpha: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PlaneRef<'a> {
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct FramePlanes<'a> {
    pub width: u32,
    pub height: u32,
    pub bottom_up: bool,
    pub y: PlaneRef<'a>,
    pub u: PlaneRef<'a>,
    pub v: PlaneRef<'a>,
    pub alpha: Option<PlaneRef<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SourceError(pub String);

pub trait FrameSource {
    fn info(&self) -> StreamInfo;

    fn advance(&mut self) -> Result<(), SourceError>;

    fn planes(&self) -> Option<FramePlanes<'_>>;

    fn rewind(&mut self);
}
