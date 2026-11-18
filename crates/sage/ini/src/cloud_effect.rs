use crate::multiplayer_color::{rgb_field, Rgb8};
use crate::tree::{Block, Document};

pub const CLOUD_EFFECT_KEYWORD: &str = "CloudEffect";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloudDarkeningSetting {
    pub factor: Rgb8,
    pub factor_rain: Rgb8,
}

impl CloudDarkeningSetting {
    fn apply(&mut self, block: &Block) {
        if let Some(value) = rgb_field(block, "DarkeningFactor") {
            self.factor = value;
        }
        if let Some(value) = rgb_field(block, "DarkeningFactorRain") {
            self.factor_rain = value;
        }
    }
}

#[must_use]
pub fn cloud_darkening(documents: &[Document]) -> CloudDarkeningSetting {
    let mut setting = CloudDarkeningSetting::default();
    for document in documents {
        for block in document.roots_named(CLOUD_EFFECT_KEYWORD) {
            setting.apply(block);
        }
    }
    setting
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{BlockTable, Document};

    const GLOBAL: &str = "\
CloudEffect
  DarkeningFactor = R:60 G:60 B:90
  DarkeningFactorRain = R:30 G:30 B:45
End
";

    #[test]
    fn a_later_documents_block_overrides_field_by_field() {
        let global = Document::parse("global.ini", GLOBAL, &BlockTable::corpus_path()).unwrap();
        let map = Document::parse(
            "map.ini",
            "CloudEffect\n  DarkeningFactor = R:10 G:10 B:10\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let setting = cloud_darkening(&[global, map]);
        assert_eq!(
            setting.factor,
            Rgb8 {
                r: 10,
                g: 10,
                b: 10
            }
        );
        assert_eq!(
            setting.factor_rain,
            Rgb8 {
                r: 30,
                g: 30,
                b: 45
            }
        );
    }

    #[test]
    fn no_block_at_all_darkens_nothing() {
        let setting = cloud_darkening(&[]);
        assert_eq!(setting.factor, Rgb8::default());
        assert_eq!(setting.factor_rain, Rgb8::default());
    }
}
