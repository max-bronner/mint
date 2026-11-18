use crate::tree::{Block, Field};

pub const ANIMATION_SOUND_CLIENT_BEHAVIOR: &str = "AnimationSoundClientBehavior";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationSoundEntry {
    pub sound: String,
    pub animations: Vec<String>,
    pub frames: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationSoundBehavior {
    pub max_update_range_cap: Option<u32>,
    pub sounds: Vec<AnimationSoundEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationSoundError {
    pub file: std::sync::Arc<str>,
    pub line: u32,
    pub message: String,
}

impl std::fmt::Display for AnimationSoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

fn attribute_runs<'a>(
    tokens: &'a [String],
    list_keys: &[&str],
) -> Result<Vec<(&'a str, Vec<&'a str>)>, String> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while let Some(token) = tokens.get(index) {
        index = index.saturating_add(1);
        let Some((key, rest)) = token.split_once(':') else {
            return Err(format!("expected KEY:VALUE, found {token:?}"));
        };
        let mut values = Vec::new();
        if !rest.is_empty() {
            values.push(rest);
        } else if let Some(next) = tokens.get(index) {
            if !next.contains(':') {
                values.push(next.as_str());
                index = index.saturating_add(1);
            }
        }
        if list_keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
            while let Some(next) = tokens.get(index) {
                if next.contains(':') {
                    break;
                }
                values.push(next.as_str());
                index = index.saturating_add(1);
            }
        }
        out.push((key, values));
    }
    Ok(out)
}

fn parse_animation_sound_entry(field: &Field) -> Result<AnimationSoundEntry, AnimationSoundError> {
    let error = |message: String| AnimationSoundError {
        file: field.file.clone(),
        line: field.line,
        message,
    };
    let runs = attribute_runs(&field.args, &["Frames"]).map_err(error)?;
    let mut entry = AnimationSoundEntry::default();
    let mut sound = None;
    for (key, values) in runs {
        match key.to_ascii_lowercase().as_str() {
            "sound" => {
                let value = values
                    .first()
                    .ok_or_else(|| error("Sound: with no value".to_owned()))?;
                sound = Some((*value).to_owned());
            }
            "animation" => {
                let value = values
                    .first()
                    .ok_or_else(|| error("Animation: with no value".to_owned()))?;
                entry.animations.push((*value).to_owned());
            }
            "frames" => {
                if values.is_empty() {
                    return Err(error("Frames: with no value".to_owned()));
                }
                for value in values {
                    let frame: u32 = value
                        .parse()
                        .map_err(|_| error(format!("Frames: {value:?} is not an integer")))?;
                    entry.frames.push(frame);
                }
            }
            other => return Err(error(format!("unknown AnimationSound key {other:?}"))),
        }
    }
    entry.sound = sound.ok_or_else(|| error("AnimationSound with no Sound:".to_owned()))?;
    Ok(entry)
}

#[must_use]
pub fn animation_sound_behavior(
    block: &Block,
) -> (AnimationSoundBehavior, Vec<AnimationSoundError>) {
    let mut behavior = AnimationSoundBehavior::default();
    let mut errors = Vec::new();
    for field in block.fields() {
        if field.keyword.eq_ignore_ascii_case("MaxUpdateRangeCap") {
            behavior.max_update_range_cap = field.value().and_then(|v| v.trim().parse().ok());
        } else if field.keyword.eq_ignore_ascii_case("AnimationSound") {
            match parse_animation_sound_entry(field) {
                Ok(entry) => behavior.sounds.push(entry),
                Err(error) => errors.push(error),
            }
        }
    }
    (behavior, errors)
}

#[cfg(test)]
mod tests;
