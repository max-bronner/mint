use mint_sage_ini::value::{parse_float, parse_int, tagged_value};

const MAX_LINES: usize = 4;

const MAX_STEP_MS: u32 = 100;

const HOLD_AFTER_TYPING_MS: u32 = 2000;

const SLIDE_PX: f32 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaptionStyle {
    pub(crate) colour: [f32; 4],
    pub(crate) position: [f32; 2],
    pub(crate) centered: bool,
    pub(crate) points: f32,
    pub(crate) bold: bool,
    pub(crate) delay_ms: u32,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            colour: [1.0, 1.0, 1.0, 1.0],
            position: [0.0, 0.0],
            centered: false,
            points: 14.0,
            bold: false,
            delay_ms: 16,
        }
    }
}

pub(crate) fn read_style(documents: &[mint_sage_ini::Document]) -> CaptionStyle {
    let mut style = CaptionStyle::default();
    let blocks = documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter(|block| block.keyword.eq_ignore_ascii_case("InGameUI"));
    for block in blocks {
        let args = |keyword: &str| block.field(keyword).map(|field| field.args.as_slice());
        if let Some(args) = args("MilitaryCaptionColor") {
            let channel = |tag: &str| {
                tagged_value(args, tag)
                    .and_then(parse_int)
                    .map(|v| v.clamp(0, 255) as f32 / 255.0)
            };
            if let (Some(r), Some(g), Some(b)) = (channel("R"), channel("G"), channel("B")) {
                style.colour = [r, g, b, channel("A").unwrap_or(1.0)];
            }
        }
        if let Some(args) = args("MilitaryCaptionPosition") {
            let axis = |tag: &str| tagged_value(args, tag).and_then(parse_float);
            if let (Some(x), Some(y)) = (axis("X"), axis("Y")) {
                style.position = [x, y];
            }
        }
        let value = |keyword: &str| block.field(keyword).and_then(mint_sage_ini::Field::value);
        if let Some(centered) =
            value("MilitaryCaptionCentered").and_then(mint_sage_ini::value::parse_bool)
        {
            style.centered = centered;
        }
        if let Some(points) = value("MilitaryCaptionPointSize")
            .and_then(parse_float)
            .filter(|p| p.is_finite() && *p > 0.0)
        {
            style.points = points;
        }
        if let Some(bold) = value("MilitaryCaptionBold").and_then(mint_sage_ini::value::parse_bool)
        {
            style.bold = bold;
        }
        if let Some(delay) = value("MilitaryCaptionDelayMS").and_then(parse_int) {
            style.delay_ms = u32::try_from(delay.max(1)).unwrap_or(u32::MAX);
        }
    }
    style
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Caption {
    text: Vec<char>,
    index: usize,
    lines: Vec<String>,
    staged: Option<char>,
    duration_ms: u32,
    next_ms: u32,
    elapsed_ms: u32,
    finished: bool,
    alpha: i32,
    delay_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Step {
    pub(crate) clicks: u32,
    pub(crate) removed: bool,
}

impl Caption {
    pub(crate) fn new(text: &str, duration_ms: u32, delay_ms: u32) -> Self {
        Self {
            text: text.chars().collect(),
            index: 0,
            lines: vec![String::new()],
            staged: None,
            duration_ms,
            next_ms: 0,
            elapsed_ms: 0,
            finished: false,
            alpha: 255,
            delay_ms: delay_ms.max(1),
        }
    }

    pub(crate) fn full_lines(&self) -> Vec<String> {
        let whole: String = self.text.iter().collect();
        whole
            .split('\n')
            .take(MAX_LINES)
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn fade(&self) -> f32 {
        self.alpha.max(0) as f32 / 255.0
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn staged(&self) -> Option<(char, f32)> {
        let ch = self.staged?;
        let remaining = self.next_ms.checked_sub(self.elapsed_ms)?;
        Some((
            ch,
            (1.0 - remaining as f32 / self.delay_ms as f32).clamp(0.0, 1.0),
        ))
    }

    pub(crate) fn slide_offset(progress: f32) -> f32 {
        (1.0 - progress) * SLIDE_PX
    }

    pub(crate) fn update(&mut self, since_last_ms: u32, menu_up: bool) -> Step {
        let mut step = Step::default();
        if menu_up {
            return step;
        }
        let dt = since_last_ms.min(MAX_STEP_MS);
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt);
        if !self.finished && self.next_ms < self.elapsed_ms {
            self.type_next(&mut step);
        } else if self.finished && self.elapsed_ms > self.duration_ms {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let drop = (dt as f32 * 0.1 + 1.0) as i32;
            if self.alpha - drop < 0 {
                step.removed = true;
            } else {
                self.alpha -= drop;
            }
        }
        step
    }

    fn type_next(&mut self, step: &mut Step) {
        if let Some(staged) = self.staged.take() {
            if let Some(line) = self.lines.last_mut() {
                line.push(staged);
            }
        }
        while let Some(&ch) = self.text.get(self.index) {
            let last = self.index.saturating_add(1) >= self.text.len();
            if ch == ' ' && !last {
                if let Some(line) = self.lines.last_mut() {
                    line.push(ch);
                }
                self.index += 1;
                continue;
            }
            if ch == '\n' {
                if self.lines.len() >= MAX_LINES {
                    self.index = self.text.len();
                } else {
                    self.lines.push(String::new());
                    self.next_ms = self.elapsed_ms.saturating_add(self.delay_ms);
                }
            } else {
                self.staged = Some(ch);
                step.clicks += 1;
                self.next_ms = self.elapsed_ms.saturating_add(self.delay_ms);
            }
            break;
        }
        self.index = self.index.saturating_add(1);
        if self.index >= self.text.len() {
            self.duration_ms = self
                .duration_ms
                .max(self.elapsed_ms.saturating_add(HOLD_AFTER_TYPING_MS));
            self.finished = true;
        }
    }
}

pub(crate) fn origin(style: &CaptionStyle, width: f32, height: f32) -> [f32; 2] {
    [
        (width * style.position[0] + 0.5).floor(),
        (height * style.position[1] + 0.5).floor(),
    ]
}

#[cfg(test)]
mod tests;
