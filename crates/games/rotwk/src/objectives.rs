use mint_vfs::Vfs;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ObjectiveList {
    pub(crate) entries: Vec<(String, bool)>,
    pub(crate) refused: Vec<String>,
}

fn strip_comment(line: &str) -> &str {
    line.split_once(';').map_or(line, |(code, _)| code).trim()
}

pub(crate) fn parse(text: &str) -> ObjectiveList {
    let mut list = ObjectiveList::default();
    let mut inside = false;
    for raw in text.lines() {
        let line = strip_comment(raw);
        if line.is_empty() {
            continue;
        }
        let mut words = line
            .split(|c: char| c.is_whitespace() || c == '=')
            .filter(|w| !w.is_empty());
        let Some(keyword) = words.next() else {
            continue;
        };
        if !inside {
            inside = keyword.eq_ignore_ascii_case("MissionObjectiveList");
            continue;
        }
        if keyword.eq_ignore_ascii_case("End") {
            inside = false;
            continue;
        }
        let bonus = keyword.eq_ignore_ascii_case("BonusMissionObjectiveTag");
        match (
            bonus || keyword.eq_ignore_ascii_case("MissionObjectiveTag"),
            words.next(),
        ) {
            (true, Some(tag)) => list.entries.push((tag.to_owned(), bonus)),
            _ => list.refused.push(line.to_owned()),
        }
    }
    list
}

pub(crate) const MOVIE_ROWS: usize = 12;

pub(crate) fn extern_values(
    objectives: &[crate::rules::map_scripts::Objective],
) -> Vec<(String, String)> {
    let mut shown = objectives.iter().filter(|objective| objective.shown);
    let mut values = Vec::new();
    for row in 1..=MOVIE_ROWS {
        let (tag, status) = shown.next().map_or((String::new(), "00".to_owned()), |o| {
            (
                format!("${}", o.tag),
                format!("{}{}", u8::from(o.completed), u8::from(o.bonus)),
            )
        });
        values.push((format!("Objective{row}"), tag));
        values.push((format!("Objective{row}Status"), status));
    }
    values
}

pub(crate) fn note_newly_shown(
    seen: &mut Vec<bool>,
    objectives: &[crate::rules::map_scripts::Objective],
) -> bool {
    seen.resize(objectives.len(), false);
    let mut newly = false;
    for (was, objective) in seen.iter_mut().zip(objectives) {
        newly |= objective.shown && !*was;
        *was = objective.shown;
    }
    newly
}

pub(crate) fn read_beside(vfs: &Vfs, map_path: &str) -> Option<ObjectiveList> {
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let path = format!("{folder}/map.ini").to_ascii_lowercase();
    let bytes = vfs.read(&path).ok()?;
    Some(parse(&String::from_utf8_lossy(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_and_bonus_tags_share_one_list_in_file_order() {
        let list = parse(
            "; header\nWeather\n  Foo = 1\nEnd\nMissionObjectiveList\n  MissionObjectiveTag = OBJ:One\n  BonusMissionObjectiveTag = OBJ:Two ; extra\n  MissionObjectiveTag OBJ:Three\nEnd\n",
        );
        assert_eq!(
            list.entries,
            [
                ("OBJ:One".to_owned(), false),
                ("OBJ:Two".to_owned(), true),
                ("OBJ:Three".to_owned(), false)
            ]
        );
        assert!(list.refused.is_empty());
    }

    #[test]
    fn the_movie_rows_are_the_shown_entries_in_order_with_completed_then_bonus_status() {
        let entry = |tag: &str, bonus, shown, completed| crate::rules::map_scripts::Objective {
            tag: tag.to_owned(),
            bonus,
            shown,
            completed,
        };
        let values = extern_values(&[
            entry("A", false, false, false),
            entry("B", false, true, true),
            entry("C", true, true, false),
        ]);
        let get = |name: &str| {
            values
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("Objective1"), Some("$B"));
        assert_eq!(get("Objective1Status"), Some("10"));
        assert_eq!(get("Objective2"), Some("$C"));
        assert_eq!(get("Objective2Status"), Some("01"));
        assert_eq!(
            (get("Objective3"), get("Objective3Status")),
            (Some(""), Some("00"))
        );
    }

    #[test]
    fn the_new_objective_flash_rises_on_hidden_to_shown_only() {
        use crate::rules::map_scripts::Objective;
        let entry = |shown| Objective {
            tag: "T".to_owned(),
            bonus: false,
            shown,
            completed: false,
        };
        let mut seen = Vec::new();
        assert!(!note_newly_shown(&mut seen, &[entry(false)]));
        assert!(note_newly_shown(&mut seen, &[entry(true)]));
        assert!(!note_newly_shown(&mut seen, &[entry(true)]));
        assert!(!note_newly_shown(&mut seen, &[entry(false)]));
        assert!(note_newly_shown(&mut seen, &[entry(true)]));
    }

    #[test]
    fn a_line_that_is_not_a_tag_is_refused_rather_than_skipped() {
        let list = parse("MissionObjectiveList\n  MissionObjectiveTag = A\n  Mystery = B\nEnd\n");
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.refused, ["Mystery = B"]);
    }
}
