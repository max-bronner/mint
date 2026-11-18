use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectAnimation {
    Off,
    Hold,
    Play,
}

impl ObjectAnimation {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ObjectAnimation::Off => "off",
            ObjectAnimation::Hold => "hold",
            ObjectAnimation::Play => "play",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "off" => Some(ObjectAnimation::Off),
            "hold" => Some(ObjectAnimation::Hold),
            "play" => Some(ObjectAnimation::Play),
            _ => None,
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            ObjectAnimation::Off => ObjectAnimation::Hold,
            ObjectAnimation::Hold => ObjectAnimation::Play,
            ObjectAnimation::Play => ObjectAnimation::Off,
        }
    }

    pub(crate) fn on(self) -> bool {
        self != ObjectAnimation::Off
    }

    pub(crate) fn completion(self) -> Completion {
        match self {
            ObjectAnimation::Hold => Completion::Hold,
            _ => Completion::Restart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ObjectMaterial {
    Off,
    Step(u32),
    #[default]
    Scroll,
}

pub(crate) const GRID_STEP_MS: u32 = 33;

impl ObjectMaterial {
    pub(crate) fn label(self) -> String {
        match self {
            ObjectMaterial::Off => "off".to_owned(),
            ObjectMaterial::Step(ticks) => format!("step:{ticks}"),
            ObjectMaterial::Scroll => "scroll".to_owned(),
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(ticks) = text.strip_prefix("step") {
            let ticks = ticks.trim_start_matches(':').trim();
            return match ticks {
                "" => Some(ObjectMaterial::Step(0)),
                _ => ticks.parse().ok().map(ObjectMaterial::Step),
            };
        }
        match text {
            "off" => Some(ObjectMaterial::Off),
            "scroll" | "on" => Some(ObjectMaterial::Scroll),
            _ => None,
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            ObjectMaterial::Off => ObjectMaterial::Step(0),
            ObjectMaterial::Step(_) => ObjectMaterial::Scroll,
            ObjectMaterial::Scroll => ObjectMaterial::Off,
        }
    }

    pub(crate) fn stepped(self) -> Self {
        match self {
            ObjectMaterial::Step(ticks) => ObjectMaterial::Step(ticks.saturating_add(1)),
            _ => ObjectMaterial::Step(0),
        }
    }

    pub(crate) fn seconds(self, seconds: f32) -> f32 {
        match self {
            ObjectMaterial::Off => 0.0,
            ObjectMaterial::Step(ticks) => {
                #[allow(clippy::cast_precision_loss)]
                let ms = ticks as f32 * GRID_STEP_MS as f32 + 0.5;
                ms / 1000.0
            }
            ObjectMaterial::Scroll => seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SwayMode {
    Off,
    Gentle,
    #[default]
    Generals,
    Strong,
}

impl SwayMode {
    pub(crate) fn breeze(self) -> Option<mint_sage_asset::Breeze> {
        match self {
            SwayMode::Off => None,
            SwayMode::Gentle => Some(mint_sage_asset::Breeze::GENTLE),
            SwayMode::Generals => Some(mint_sage_asset::Breeze::GENERALS),
            SwayMode::Strong => Some(mint_sage_asset::Breeze::STRONG),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            SwayMode::Off => "off",
            SwayMode::Gentle => "gentle",
            SwayMode::Generals => "generals",
            SwayMode::Strong => "strong",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "off" => Some(SwayMode::Off),
            "gentle" => Some(SwayMode::Gentle),
            "generals" | "on" | "default" => Some(SwayMode::Generals),
            "strong" => Some(SwayMode::Strong),
            _ => None,
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            SwayMode::Off => SwayMode::Gentle,
            SwayMode::Gentle => SwayMode::Generals,
            SwayMode::Generals => SwayMode::Strong,
            SwayMode::Strong => SwayMode::Off,
        }
    }

    pub(crate) fn report(self) -> String {
        let Some(breeze) = self.breeze() else {
            return format!("vegetation sway = {} (,, --sway)", self.label());
        };
        format!(
            "vegetation sway = {} direction={:.4}rad ({:.1}deg) intensity={:.4}rad ({:.2}deg) \
             lean={:.4}rad ({:.2}deg) period={:.1}s randomness={:.2} (,, --sway)",
            self.label(),
            breeze.direction,
            breeze.direction.to_degrees(),
            breeze.intensity,
            breeze.intensity.to_degrees(),
            breeze.lean,
            breeze.lean.to_degrees(),
            breeze.period,
            breeze.randomness
        )
    }
}

pub(crate) fn live_material_seconds(material: ObjectMaterial, held: f32) -> f32 {
    match material {
        ObjectMaterial::Off => held,
        other => other.seconds(held),
    }
}

pub(crate) fn grid_cells_at(seconds: f32) -> String {
    let at = |fps: f32, log2_width: u32| {
        let args = mint_sage_w3d::MapperArgs {
            fps,
            log2_width,
            ..mint_sage_w3d::MapperArgs::default()
        };
        let offset =
            mint_sage_asset::sample_uv_transform(mint_sage_w3d::UvMapperType::Grid, &args, seconds)
                .offset;
        let width = 1u32 << log2_width;
        #[allow(clippy::cast_precision_loss)]
        let scale = width as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = (
            (offset[0] * scale).round() as u32,
            (offset[1] * scale).round() as u32,
        );
        format!("{width}x{width} col {} row {}", cell.0, cell.1)
    };
    format!("FPS=15 {}; FPS=30 {}", at(15.0, 2), at(30.0, 3))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectAction {
    Toggle,
    ToggleModels,
}

pub(crate) fn pan_slot(key: Key) -> Option<usize> {
    match key {
        Key::ArrowUp => Some(0),
        Key::ArrowDown => Some(1),
        Key::ArrowLeft => Some(2),
        Key::ArrowRight => Some(3),
        _ => None,
    }
}
