use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_apt_player::{InstanceId, Player, Value};

#[derive(Debug, Clone)]
pub struct Palette {
    pub texture_width: u32,
    pub texture_height: u32,
    pub rgba: Vec<u8>,
    pub origin: [u32; 2],
    pub size: [u32; 2],
}

impl Palette {
    fn texel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.texture_width || y >= self.texture_height {
            return None;
        }
        let index = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.texture_width).ok()?)?
            .checked_add(usize::try_from(x).ok()?)?
            .checked_mul(4)?;
        let px = self.rgba.get(index..index.checked_add(4)?)?;
        let [r, g, b, a] = [*px.first()?, *px.get(1)?, *px.get(2)?, *px.get(3)?];
        Some(u32::from_be_bytes([a, r, g, b]))
    }

    #[must_use]
    pub fn sample(&self, cursor: [f32; 2], rect: [f32; 2]) -> Option<u32> {
        let x = scaled(cursor[0], self.size[0], rect[0])?;
        let y = scaled(cursor[1], self.size[1], rect[1])?;
        let x = i64::from(self.origin[0]).checked_add(x)?;
        let y = i64::from(self.origin[1]).checked_add(y)?;
        self.texel(u32::try_from(x).ok()?, u32::try_from(y).ok()?)
    }

    #[must_use]
    pub fn locate(&self, colour: u32, rect: [f32; 2]) -> Option<[f32; 2]> {
        let [w, h] = self.size;
        let mut best: Option<(u64, u32, u32)> = None;
        for row in 0..h {
            for col in 0..w {
                let texel = self.texel(
                    self.origin[0].checked_add(col)?,
                    self.origin[1].checked_add(row)?,
                )?;
                let distance = texel
                    .to_be_bytes()
                    .iter()
                    .zip(colour.to_be_bytes())
                    .map(|(&a, b)| i64::from(a).abs_diff(i64::from(b)).pow(2))
                    .sum::<u64>();
                if best.is_none_or(|(least, _, _)| distance < least) {
                    best = Some((distance, col, row));
                }
            }
        }
        let (_, col, row) = best?;
        Some([place(col, rect[0], w), place(row, rect[1], h)])
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn scaled(cursor: f32, image: u32, rect: f32) -> Option<i64> {
    if rect <= 0.0 || !cursor.is_finite() {
        return None;
    }
    Some((cursor * image as f32 / rect) as i64)
}

#[allow(clippy::cast_precision_loss)]
fn place(texel: u32, rect: f32, image: u32) -> f32 {
    (texel as f32 * rect / image.max(1) as f32 + 0.5).floor()
}

struct Picker {
    instance: InstanceId,
    image: String,
    path: String,
    rect: [f32; 2],
    cursor: Option<[f32; 2]>,
    colour: Option<u32>,
}

#[derive(Default)]
pub struct ColorPickers {
    pickers: BTreeMap<String, Picker>,
}

impl ColorPickers {
    pub fn clear(&mut self) {
        self.pickers.clear();
    }

    pub fn seen(&mut self, player: &mut Player, instance: InstanceId, rect: [f32; 2]) {
        let text = |name: &str| player.own_string(instance, name).map(|s| s.to_string());
        let (Some(id), Some(image)) = (
            text("_componentId").filter(|id| !id.is_empty()),
            text("_imageName"),
        ) else {
            return;
        };
        let path = player
            .own_target_path(instance, "_path")
            .unwrap_or_default();
        if let Some(picker) = self.pickers.get_mut(&id) {
            picker.instance = instance;
            picker.rect = rect;
            return;
        }
        player.register_native_extern(&format!("{id}Cursor"));
        player.register_native_extern(&format!("{id}Color"));
        self.pickers.insert(
            id,
            Picker {
                instance,
                image,
                path,
                rect,
                cursor: None,
                colour: None,
            },
        );
    }

    pub fn service(
        &mut self,
        player: &mut Player,
        palette: &mut dyn FnMut(&str) -> Option<Arc<Palette>>,
    ) {
        for (name, value) in player.take_extern_writes() {
            let Some(text) = text_of(&value) else {
                continue;
            };
            for (id, picker) in &mut self.pickers {
                match name.strip_prefix(id.as_str()) {
                    Some("Color") => {
                        if let Ok(colour) = text.trim().parse::<u32>() {
                            picker.colour = Some(colour);
                        }
                    }
                    Some("Cursor") => {
                        if let Some(cursor) = parse_cursor(&text) {
                            picker.cursor = Some(cursor);
                        }
                    }
                    _ => {}
                }
            }
        }
        for picker in self.pickers.values_mut() {
            if picker.cursor.is_none() && picker.colour.is_none() {
                continue;
            }
            let Some(palette) = palette(&picker.image) else {
                continue;
            };
            if let Some(colour) = picker
                .cursor
                .and_then(|cursor| palette.sample(cursor, picker.rect))
            {
                picker.cursor = None;
                let path = format!("{}.SetColor", picker.path);
                let _ = player.call(
                    picker.instance,
                    &path,
                    vec![Value::str(&colour.to_string())],
                );
            }
            if let Some(colour) = picker.colour.take() {
                if let Some([x, y]) = palette.locate(colour, picker.rect) {
                    let path = format!("{}.SetCursor", picker.path);
                    let _ = player.call(
                        picker.instance,
                        &path,
                        vec![Value::Number(f64::from(x)), Value::Number(f64::from(y))],
                    );
                }
            }
        }
    }

    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.pickers.values().map(|p| p.image.as_str())
    }
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_string()),
        Value::Number(n) => Some(format!("{n}")),
        _ => None,
    }
}

fn parse_cursor(text: &str) -> Option<[f32; 2]> {
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    Some([x, y])
}

#[cfg(test)]
mod tests;
