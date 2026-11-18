use mint_sage_ini::{value::parse_percent, Document};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Handicap {
    pub(crate) damage: f32,
    pub(crate) health: f32,
    pub(crate) spell_damage: f32,
    pub(crate) recharge: f32,
    pub(crate) build_time: f32,
}

impl Default for Handicap {
    fn default() -> Self {
        Self {
            damage: 1.0,
            health: 1.0,
            spell_damage: 1.0,
            recharge: 1.0,
            build_time: 1.0,
        }
    }
}

impl Handicap {
    pub(crate) fn damage_to(&self, victim: &Self, amount: f32) -> f32 {
        amount * self.damage / victim.health.max(0.01)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HandicapTable {
    by_percent: Vec<(u8, Handicap)>,
}

impl HandicapTable {
    pub(crate) fn read(documents: &[Document], game_data: &mint_sage_ini::Block) -> Self {
        let mut table = Self::default();
        for document in documents {
            for block in document.roots() {
                if !block.keyword.eq_ignore_ascii_case("ModifierList") {
                    continue;
                }
                let Some(percent) = block
                    .name()
                    .and_then(|name| suffix_percent(name, "HandicapPercent"))
                else {
                    continue;
                };
                let mut set: Vec<(String, f32)> = Vec::new();
                for field in block.fields_named("Modifier") {
                    if let (Some(kind), Some(value)) = (
                        field.args.first(),
                        field.args.get(1).and_then(|v| parse_percent(v)),
                    ) {
                        set.push((kind.to_ascii_uppercase(), value));
                    }
                }
                table.update(percent, |entry| {
                    for (kind, value) in set {
                        match kind.as_str() {
                            "DAMAGE_MULT" => entry.damage = value,
                            "HEALTH_MULT" => entry.health = value,
                            "SPELL_DAMAGE" => entry.spell_damage = value,
                            "RECHARGE_TIME" => entry.recharge = value,
                            _ => {}
                        }
                    }
                });
            }
        }
        for field in game_data.fields() {
            let (Some(percent), Some(value)) = (
                suffix_percent(&field.keyword, "HandicapBuildSpeed"),
                field.value().and_then(parse_percent),
            ) else {
                continue;
            };
            table.update(percent, |entry| entry.build_time = value);
        }
        table.by_percent.sort_by_key(|(p, _)| *p);
        table
    }

    fn update(&mut self, percent: u8, change: impl FnOnce(&mut Handicap)) {
        match self.by_percent.iter_mut().find(|(p, _)| *p == percent) {
            Some((_, handicap)) => change(handicap),
            None => {
                let mut handicap = Handicap::default();
                change(&mut handicap);
                self.by_percent.push((percent, handicap));
            }
        }
    }

    pub(crate) fn get(&self, percent: u8) -> Handicap {
        self.by_percent
            .iter()
            .find(|(p, _)| *p == percent)
            .map_or_else(Handicap::default, |(_, h)| *h)
    }

    pub(crate) fn len(&self) -> usize {
        self.by_percent.len()
    }
}

fn suffix_percent(name: &str, prefix: &str) -> Option<u8> {
    let head = name.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    name.get(prefix.len()..)?
        .parse::<u8>()
        .ok()
        .filter(|p| (1..=100).contains(p))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mint_sage_ini::table::BlockTable;

    const TEXT: &str = "ModifierList HandicapPercent5
  Category = LEVEL
  Modifier = DAMAGE_MULT 95%
  Modifier = SPELL_DAMAGE 95%
  Modifier = HEALTH_MULT 90%
  Modifier = RECHARGE_TIME 105%
  Duration = 0
End

ModifierList SomethingElse
  Modifier = DAMAGE_MULT 50%
End

GameData
  HandicapBuildSpeed5 = 105%
  HandicapBuildSpeed100 = 200%
End
";

    fn table() -> HandicapTable {
        let document = Document::parse("synthetic.ini", TEXT, &BlockTable::corpus_path()).unwrap();
        let documents = [document];
        let game_data = mint_sage_ini::game_data::merged_game_data(&documents);
        HandicapTable::read(&documents, &game_data)
    }

    #[test]
    fn a_handicap_reads_its_modifiers_and_build_speed() {
        let t = table();
        let five = t.get(5);
        assert_eq!(five.damage, 0.95);
        assert_eq!(five.spell_damage, 0.95);
        assert_eq!(five.health, 0.90);
        assert_eq!(five.recharge, 1.05);
        assert_eq!(five.build_time, 1.05);
        assert_eq!(t.get(100).build_time, 2.0);
        assert_eq!(t.get(100).damage, 1.0, "only its build speed is declared");
        assert_eq!(t.get(0), Handicap::default());
        assert_eq!(t.get(50), Handicap::default(), "not declared");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn damage_is_the_attackers_multiplier_over_the_victims_health() {
        let t = table();
        let (weak, none) = (t.get(5), Handicap::default());
        assert!((weak.damage_to(&none, 100.0) - 95.0).abs() < 1e-3);
        assert!((none.damage_to(&weak, 90.0) - 100.0).abs() < 1e-3);
        assert!((none.damage_to(&none, 7.0) - 7.0).abs() < 1e-6);
        let nothing = Handicap {
            health: 0.0,
            ..Handicap::default()
        };
        assert!(none.damage_to(&nothing, 1.0).is_finite());
    }

    #[test]
    fn a_percent_suffix_reads_one_to_a_hundred() {
        assert_eq!(
            suffix_percent("HandicapPercent95", "HandicapPercent"),
            Some(95)
        );
        assert_eq!(
            suffix_percent("handicappercent5", "HandicapPercent"),
            Some(5)
        );
        assert_eq!(suffix_percent("HandicapPercent0", "HandicapPercent"), None);
        assert_eq!(
            suffix_percent("HandicapPercent101", "HandicapPercent"),
            None
        );
        assert_eq!(suffix_percent("Handicap", "HandicapPercent"), None);
        assert_eq!(suffix_percent("HandicapPercentX", "HandicapPercent"), None);
    }
}
