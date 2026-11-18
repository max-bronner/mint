use super::{prefix, set, Filter, Route, Site, BLACK_BOX, CATEGORIES};

#[test]
fn a_category_list_admits_what_it_names_and_an_exclusion_wins_over_all() {
    let listed = Filter::parse("power, stance");
    assert_eq!(
        ["power", "stance", "order"].map(|c| listed.admits(c)),
        [true, true, false]
    );
    let all_but = Filter::parse("all,-order");
    assert_eq!(["power", "order"].map(|c| all_but.admits(c)), [true, false]);
}

#[test]
fn a_category_no_line_carries_is_reported() {
    assert_eq!(
        Filter::parse("power,stnace,-ordr").unknown(),
        ["stnace", "ordr"]
    );
}

const NOT_LITERAL: &[&str] = &["misc", "production", "report", "stats"];

#[test]
fn categories_are_the_prefixes_in_the_sources() {
    let mut found: Vec<String> = NOT_LITERAL.iter().map(|&c| c.to_owned()).collect();
    let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for call in text.split("diag!(").skip(1) {
                    let Some(literal) = call.trim_start().strip_prefix('"') else {
                        continue;
                    };
                    if let Some(category) = prefix(literal) {
                        found.push(category.to_owned());
                    }
                }
            }
        }
    }
    found.sort();
    found.dedup();
    assert_eq!(
        found, CATEGORIES,
        "a new diag! prefix belongs in CATEGORIES"
    );
}

#[test]
fn every_black_box_category_is_one_a_line_carries() {
    let strays: Vec<&str> = BLACK_BOX
        .iter()
        .copied()
        .filter(|name| !CATEGORIES.contains(name))
        .collect();
    assert!(strays.is_empty(), "{strays:?}");
}

#[test]
fn a_call_site_follows_a_filter_change_and_keeps_the_black_box_unprinted() {
    static SHOT: Site = Site::new("shot: {}");
    static DEATH: Site = Site::new("death: {}");
    let _ = set("power");
    assert_eq!([SHOT.route(), DEATH.route()], [Route::Skip, Route::Keep]);
    let _ = set("shot");
    assert_eq!([SHOT.route(), DEATH.route()], [Route::Print, Route::Keep]);
    let _ = set("off");
    assert_eq!([SHOT.route(), DEATH.route()], [Route::Skip, Route::Keep]);
}
