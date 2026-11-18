use mint_sage_ini::AnimationFxEvent;

pub(super) const MAX_FX_EVENTS_QUEUED: usize = 16;

const FX_EVENT_SKIP_FRAMES: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoneFxCue {
    pub slot: usize,
    pub fx_list: String,
    pub bone: Option<String>,
    pub frame_stop: u32,
}

pub(super) fn queue_fx_events(
    events: &[AnimationFxEvent],
    prev: Option<f32>,
    cur: f32,
    last: f32,
    out: &mut Vec<String>,
    bone_out: &mut Vec<BoneFxCue>,
) {
    for (slot, event) in events.iter().enumerate() {
        if out.len() >= MAX_FX_EVENTS_QUEUED {
            break;
        }
        #[allow(clippy::cast_precision_loss)]
        let target = event.frame as f32;
        if crossed(target, prev, cur, last, event.fire_when_skipped) {
            out.push(event.fx_list.clone());
            if let Some(frame_stop) = event.frame_stop {
                bone_out.push(BoneFxCue {
                    slot,
                    fx_list: event.fx_list.clone(),
                    bone: event.bone.clone(),
                    frame_stop,
                });
            }
        }
    }
}

pub(super) fn crossed(
    target: f32,
    prev: Option<f32>,
    cur: f32,
    last: f32,
    fire_when_skipped: bool,
) -> bool {
    if !cur.is_finite() || !last.is_finite() {
        return false;
    }
    let target = target.clamp(0.0, last.max(0.0));
    match prev {
        None => target <= cur,
        Some(prev) if !prev.is_finite() || prev == cur => false,
        Some(prev) if cur >= prev => {
            target > prev
                && target <= cur
                && (fire_when_skipped || cur - prev <= FX_EVENT_SKIP_FRAMES)
        }
        Some(prev) => {
            let wrapped_delta = last.max(0.0) - prev + cur;
            (target > prev || target <= cur)
                && (fire_when_skipped || wrapped_delta <= FX_EVENT_SKIP_FRAMES)
        }
    }
}
