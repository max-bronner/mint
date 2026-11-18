use std::ops::Range;

use mint_movie::{FramePlanes, FrameSource, PlaneRef, SourceError, StreamInfo};
use mint_sage_vp6::{ChunkKind, Container};
use mint_vp6::{Decoder, Picture, Plane};

#[derive(Debug, thiserror::Error)]
pub enum OpenSourceError {
    #[error("not a readable VP6 movie: {0}")]
    Container(#[from] mint_sage_vp6::Error),
    #[error("the movie has no frames")]
    Empty,
}

struct FrameRange {
    colour: Range<usize>,
    alpha: Option<Range<usize>>,
}

pub struct Vp6Source {
    data: Vec<u8>,
    frames: Vec<FrameRange>,
    info: StreamInfo,
    colour: Decoder,
    alpha: Option<Decoder>,
    next: usize,
    colour_picture: Option<Picture>,
    alpha_picture: Option<Picture>,
}

fn range_of(data: &[u8], chunk: &[u8]) -> Option<Range<usize>> {
    let start = (chunk.as_ptr() as usize).checked_sub(data.as_ptr() as usize)?;
    Some(start..start.checked_add(chunk.len())?)
}

impl Vp6Source {
    pub fn open(data: Vec<u8>) -> Result<Self, OpenSourceError> {
        let container = Container::parse(&data)?;
        let mut frames = Vec::new();
        let mut pending_alpha = None;
        for chunk in container.chunks() {
            let chunk = chunk?;
            let Some(range) = range_of(&data, chunk.data) else {
                continue;
            };
            match chunk.kind {
                ChunkKind::AlphaKey | ChunkKind::AlphaInter => pending_alpha = Some(range),
                ChunkKind::ColourKey | ChunkKind::ColourInter => frames.push(FrameRange {
                    colour: range,
                    alpha: pending_alpha.take(),
                }),
            }
        }
        if frames.is_empty() {
            return Err(OpenSourceError::Empty);
        }
        let header = container.header;
        let info = StreamInfo {
            width: u32::from(header.width),
            height: u32::from(header.height),
            frame_count: u32::try_from(frames.len()).unwrap_or(u32::MAX),
            rate_numerator: header.rate_numerator,
            rate_denominator: header.rate_denominator,
            has_alpha: container.alpha_header.is_some(),
        };
        let alpha = info.has_alpha.then(Decoder::new);
        Ok(Self {
            alpha,
            data,
            frames,
            info,
            colour: Decoder::new(),
            next: 0,
            colour_picture: None,
            alpha_picture: None,
        })
    }
}

fn plane_ref(plane: &Plane) -> Option<PlaneRef<'_>> {
    Some(PlaneRef {
        width: u32::try_from(plane.width()).ok()?,
        height: u32::try_from(plane.height()).ok()?,
        data: plane.data(),
    })
}

impl FrameSource for Vp6Source {
    fn info(&self) -> StreamInfo {
        self.info
    }

    fn advance(&mut self) -> Result<(), SourceError> {
        let index = self.next;
        let frame = self
            .frames
            .get(index)
            .ok_or_else(|| SourceError("end of stream".to_owned()))?;
        let slice = |range: &Range<usize>| {
            self.data
                .get(range.clone())
                .ok_or_else(|| SourceError(format!("frame {index}: chunk out of range")))
        };
        if let (Some(decoder), Some(range)) = (self.alpha.as_mut(), frame.alpha.as_ref()) {
            let picture = decoder
                .decode(slice(range)?)
                .map_err(|e| SourceError(format!("alpha frame {index}: {e}")))?;
            self.alpha_picture = Some(picture.clone());
        }
        let picture = self
            .colour
            .decode(slice(&frame.colour)?)
            .map_err(|e| SourceError(format!("frame {index}: {e}")))?;
        self.colour_picture = Some(picture.clone());
        self.next = index.saturating_add(1);
        Ok(())
    }

    fn planes(&self) -> Option<FramePlanes<'_>> {
        let picture = self.colour_picture.as_ref()?;
        let y = plane_ref(picture.y())?;
        Some(FramePlanes {
            width: self.info.width.min(y.width),
            height: self.info.height.min(y.height),
            bottom_up: true,
            y,
            u: plane_ref(picture.u())?,
            v: plane_ref(picture.v())?,
            alpha: self
                .alpha_picture
                .as_ref()
                .and_then(|picture| plane_ref(picture.y())),
        })
    }

    fn rewind(&mut self) {
        self.next = 0;
    }
}
