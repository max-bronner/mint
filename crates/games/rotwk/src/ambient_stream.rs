use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stream {
    pub(crate) level: f32,
    pub(crate) fading: bool,
    pub(crate) timer_ms: f32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Changes {
    pub(crate) started: Vec<String>,
    pub(crate) fading: Vec<(String, u32)>,
    pub(crate) stopped: Vec<String>,
}

pub(crate) fn tick(
    states: &mut BTreeMap<String, Stream>,
    markers: &[(String, f32)],
    max_streams: u32,
    hysteresis: f32,
    min_sample_volume: f32,
    fade_ms: f32,
    dt_ms: f32,
) -> Changes {
    let max_streams = max_streams.min(2);
    let mut changes = Changes::default();

    for stream in states.values_mut() {
        stream.level = 0.0;
    }
    let mut candidates: Vec<(String, f32)> = Vec::new();
    for (name, loudness) in markers {
        if *loudness < min_sample_volume {
            continue;
        }
        if let Some(stream) = states.get_mut(name) {
            stream.level = stream.level.max(*loudness);
            continue;
        }
        match candidates.iter_mut().find(|(n, _)| n == name) {
            Some((_, level)) => *level = level.max(*loudness),
            None => candidates.push((name.clone(), *loudness)),
        }
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    candidates.truncate(2);

    let playing = states.values().filter(|stream| !stream.fading).count();
    let mut capacity =
        i32::try_from(max_streams).unwrap_or(i32::MAX) - i32::try_from(playing).unwrap_or(i32::MAX);
    let mut unslotted = 0usize;
    loop {
        let weakest = states
            .iter()
            .filter(|(_, stream)| !stream.fading)
            .map(|(name, stream)| {
                let bonus = if stream.level > 0.0 { hysteresis } else { 0.0 };
                (name.clone(), stream.level + bonus)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((weakest_name, weakest_score)) = weakest else {
            break;
        };
        let louder_candidate = candidates
            .get(unslotted)
            .is_some_and(|(_, level)| *level > weakest_score);
        if capacity < 0 || louder_candidate {
            if let Some(stream) = states.get_mut(&weakest_name) {
                stream.fading = true;
                let ms = fade_ms_to_floor(stream.level, min_sample_volume, fade_ms);
                changes.fading.push((weakest_name, ms));
            }
            capacity += 1;
            if louder_candidate {
                unslotted += 1;
            }
        } else {
            break;
        }
    }

    let free = u32::try_from(capacity).unwrap_or(0);
    for (name, level) in candidates.into_iter().take(free as usize) {
        states.insert(
            name.clone(),
            Stream {
                level,
                fading: false,
                timer_ms: 0.0,
            },
        );
        changes.started.push(name);
    }

    let mut stopped = Vec::new();
    for (name, stream) in states.iter_mut() {
        if stream.fading {
            stream.timer_ms += dt_ms;
            let factor = fade_factor(stream.timer_ms, fade_ms);
            if stream.level * factor < min_sample_volume {
                stopped.push(name.clone());
            }
        } else {
            stream.timer_ms = (stream.timer_ms - dt_ms).max(0.0);
        }
    }
    for name in &stopped {
        states.remove(name);
    }
    changes.stopped = stopped;
    changes
}

pub(crate) fn fade_factor(timer_ms: f32, fade_ms: f32) -> f32 {
    if fade_ms <= 0.0 {
        return 0.0;
    }
    (1.0 - timer_ms / fade_ms).clamp(0.0, 1.0)
}

fn fade_ms_to_floor(level: f32, min_sample_volume: f32, fade_ms: f32) -> u32 {
    if level <= 0.0 || fade_ms <= 0.0 {
        return 0;
    }
    let floor_factor = (min_sample_volume / level).clamp(0.0, 1.0);
    let ms = fade_ms * (1.0 - floor_factor);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (ms.max(0.0) as u32).min(fade_ms as u32)
}

#[cfg(test)]
mod tests;
