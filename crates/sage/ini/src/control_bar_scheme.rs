use crate::tree::{Block, Document};
use crate::value::{parse_int, tagged_value};

pub const CONTROL_BAR_SCHEME_KEYWORD: &str = "ControlBarScheme";

pub const DEFAULT_SCHEME: &str = "Default";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Argb8 {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Argb8 {
    #[must_use]
    pub fn packed(self) -> u32 {
        u32::from_be_bytes([self.a, self.r, self.g, self.b])
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlBarSchemeDef {
    pub name: String,
    pub side: Option<String>,
    pub screen_width: Option<i32>,
    pub build_up_clock_color: Option<Argb8>,
}

fn argb_field(block: &Block, keyword: &str) -> Option<Argb8> {
    let args = &block.field(keyword)?.args;
    let channel = |tag: &str| -> Option<u8> {
        let raw = parse_int(tagged_value(args, tag)?)?;
        Some(u8::try_from(raw.clamp(0, 255)).unwrap_or(u8::MAX))
    };
    Some(Argb8 {
        a: channel("A")?,
        r: channel("R")?,
        g: channel("G")?,
        b: channel("B")?,
    })
}

#[must_use]
pub fn read_control_bar_scheme(block: &Block) -> ControlBarSchemeDef {
    ControlBarSchemeDef {
        name: block.name().unwrap_or_default().to_owned(),
        side: block
            .field("Side")
            .and_then(crate::tree::Field::value)
            .map(str::to_owned),
        screen_width: block
            .field("ScreenCreationRes")
            .and_then(|field| tagged_value(&field.args, "X"))
            .and_then(parse_int)
            .and_then(|x| i32::try_from(x).ok()),
        build_up_clock_color: argb_field(block, "BuildUpClockColor"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ControlBarSchemeIndex {
    schemes: Vec<ControlBarSchemeDef>,
}

impl ControlBarSchemeIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            if block.keyword.eq(CONTROL_BAR_SCHEME_KEYWORD) && block.name().is_some() {
                self.schemes.push(read_control_bar_scheme(block));
            }
        }
    }

    #[must_use]
    pub fn for_side(&self, side: &str) -> Option<&ControlBarSchemeDef> {
        let mut best: Option<&ControlBarSchemeDef> = None;
        for scheme in self.schemes.iter().filter(|s| {
            s.side
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(side))
        }) {
            let wider = best.is_none_or(|held| {
                scheme.screen_width.unwrap_or(0) > held.screen_width.unwrap_or(0)
            });
            if wider {
                best = Some(scheme);
            }
        }
        best.or_else(|| {
            self.schemes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(DEFAULT_SCHEME))
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_side_takes_its_widest_scheme_and_otherwise_the_default() {
        let text = "
ControlBarScheme Default
  Side = Default
  BuildUpClockColor = R:1 G:2 B:3 A:4
End
ControlBarScheme Small
  Side = Elves
  ScreenCreationRes = X:800 Y:600
  BuildUpClockColor = R:0 G:0 B:0 A:192
End
ControlBarScheme Wide
  Side = Elves
  ScreenCreationRes = X:1024 Y:768
End
";
        let document = Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap();
        let mut index = ControlBarSchemeIndex::new();
        index.insert_document(&document);
        assert_eq!(index.for_side("elves").unwrap().name, "Wide");
        let default = index.for_side("Men").unwrap();
        assert_eq!(default.name, "Default");
        assert_eq!(
            default.build_up_clock_color.map(Argb8::packed),
            Some(0x0401_0203)
        );
    }
}
