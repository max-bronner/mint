use mint_sage_livingworld::screen_queries::PlayerStats;

const TREASURY: (usize, &str) = (0, "STRATEGICHUD:StatsTreasury");
const INCOME: (usize, &str) = (6, "STRATEGICHUD:StatsTreasuryIncome");
const BONUS: &str = "STRATEGICHUD:StatsBonus";
const MULTIPLIER: (usize, &str) = (4, "STRATEGICHUD:StatsResourceMultiplier");
const POWER_POINTS: (usize, &str) = (5, "STRATEGICHUD:StatsPowerPoints");

#[derive(Debug, Default)]
pub(super) struct Stats {
    clip: Option<String>,
    shown: Option<PlayerStats>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct StatsChanges {
    pub(super) texts: Vec<(String, String)>,
    pub(super) flash: Option<String>,
}

impl Stats {
    pub(super) fn loaded(&mut self, clip: &str) {
        self.clip = Some(clip.to_owned());
        self.shown = None;
    }

    pub(super) fn changes(
        &mut self,
        now: PlayerStats,
        label: impl Fn(&str) -> Option<String>,
    ) -> StatsChanges {
        let mut out = StatsChanges::default();
        let Some(clip) = self.clip.as_deref() else {
            return out;
        };
        let was = self.shown.replace(now);
        let mut row = |(index, text): (usize, String)| {
            out.texts.push((format!("{clip}.{index}_Text"), text));
        };
        let int = |label_key: &str, value: i32| {
            label(label_key)
                .map(|t| {
                    mint_sage_str::format_placeholder(
                        &t,
                        mint_sage_str::Placeholder::Int(i64::from(value)),
                    )
                })
                .unwrap_or_default()
        };
        let money = (now.treasury, now.income);
        if was.map(|w| (w.treasury, w.income)) != Some(money) {
            row((TREASURY.0, int(TREASURY.1, now.treasury)));
            row((INCOME.0, int(INCOME.1, now.income)));
        }
        for (i, value) in now.bonuses.iter().enumerate() {
            if was.map(|w| w.bonuses.get(i).copied()) != Some(Some(*value)) {
                row((i + 1, int(BONUS, *value)));
            }
        }
        if was.map(|w| w.resource_multiplier) != Some(now.resource_multiplier) {
            let text = label(MULTIPLIER.1)
                .map(|t| t.replacen("%g", &format_g(f64::from(now.resource_multiplier)), 1))
                .unwrap_or_default();
            row((MULTIPLIER.0, text));
        }
        if was.map(|w| w.power_points) != Some(now.power_points) {
            let text = label(POWER_POINTS.1)
                .map(|_| now.power_points.to_string())
                .unwrap_or_default();
            row((POWER_POINTS.0, text));
        }
        if was.map(|w| w.treasury) != Some(now.treasury) {
            let below = clip
                .strip_prefix("_level")
                .and_then(|rest| rest.split_once('.'))
                .map_or(clip, |(_, path)| path);
            out.flash = Some(below.to_owned());
        }
        out
    }
}

fn format_g(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return format!("{value}");
    }
    let exponent = format!("{value:.5e}");
    let Some((mantissa, power)) = exponent.split_once('e') else {
        return exponent;
    };
    let power: i32 = power.parse().unwrap_or(0);
    let trim = |s: String| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s
        }
    };
    if (-4..6).contains(&power) {
        let decimals = usize::try_from(5 - power).unwrap_or(0);
        trim(format!("{value:.decimals$}"))
    } else {
        let sign = if power < 0 { '-' } else { '+' };
        format!(
            "{}e{sign}{:02}",
            trim(mantissa.to_owned()),
            power.unsigned_abs()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> PlayerStats {
        PlayerStats {
            treasury: 1500,
            income: 300,
            bonuses: [10, 0, 25],
            resource_multiplier: 1.1,
            power_points: 4,
        }
    }

    fn label(key: &str) -> Option<String> {
        Some(
            match key {
                "STRATEGICHUD:StatsTreasury" | "STRATEGICHUD:StatsPowerPoints" => "%d",
                "STRATEGICHUD:StatsTreasuryIncome" => "+%d",
                "STRATEGICHUD:StatsBonus" => "+%d%%",
                "STRATEGICHUD:StatsResourceMultiplier" => "x%g",
                _ => return None,
            }
            .to_owned(),
        )
    }

    #[test]
    fn every_row_is_written_once_then_only_the_rows_that_change() {
        let mut panel = Stats::default();
        assert_eq!(panel.changes(stats(), label), StatsChanges::default());
        panel.loaded("_level0.main.stats");
        let first = panel.changes(stats(), label);
        let text = |key: &str| {
            first
                .texts
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, t)| t.as_str())
        };
        assert_eq!(text("_level0.main.stats.0_Text"), Some("1500"));
        assert_eq!(text("_level0.main.stats.6_Text"), Some("+300"));
        assert_eq!(text("_level0.main.stats.1_Text"), Some("+10%"));
        assert_eq!(text("_level0.main.stats.3_Text"), Some("+25%"));
        assert_eq!(text("_level0.main.stats.4_Text"), Some("x1.1"));
        assert_eq!(text("_level0.main.stats.5_Text"), Some("4"));
        assert_eq!(first.texts.len(), 7);
        assert_eq!(first.flash.as_deref(), Some("main.stats"));
        let mut later = stats();
        later.bonuses[1] = 5;
        let next = panel.changes(later, label);
        assert_eq!(
            next.texts,
            vec![("_level0.main.stats.2_Text".to_owned(), "+5%".to_owned())]
        );
        assert_eq!(next.flash, None);
    }

    #[test]
    fn percent_g_keeps_six_significant_digits_without_trailing_zeros() {
        assert_eq!(format_g(1.0), "1");
        assert_eq!(format_g(f64::from(1.1f32)), "1.1");
        assert_eq!(format_g(f64::from(0.9f32)), "0.9");
        assert_eq!(format_g(1.25), "1.25");
        assert_eq!(format_g(1_234_567.0), "1.23457e+06");
    }
}
