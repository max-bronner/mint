use std::path::Path;

use mint_sage_save::{
    read_info, sort_newest_first, tab_accepts, GameStateBlock, SaveDate, SaveTab,
};

const TABS: [SaveTab; 4] = [
    SaveTab::Campaign,
    SaveTab::Skirmish,
    SaveTab::WotrSingle,
    SaveTab::WotrMulti,
];

struct Entry {
    file: String,
    info: GameStateBlock,
}

fn main() {
    for arg in std::env::args().skip(1) {
        println!("== {arg}");
        let names: Vec<String> = match std::fs::read_dir(Path::new(&arg)) {
            Ok(rd) => {
                let mut v: Vec<String> = rd
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                v.sort_by_key(|n| n.to_lowercase());
                v
            }
            Err(e) => {
                println!("unreadable: {e}");
                continue;
            }
        };
        let mut failed = 0usize;
        for tab in TABS {
            let mut rows = Vec::new();
            for name in names.iter().filter(|n| tab.lists_file(n)) {
                let Ok(bytes) = std::fs::read(Path::new(&arg).join(name)) else {
                    failed += 1;
                    continue;
                };
                match read_info(&bytes) {
                    Ok(info) if tab_accepts(tab, info.save_file_type) => rows.push(Entry {
                        file: name.clone(),
                        info,
                    }),
                    Ok(_) => {}
                    Err(e) => {
                        failed += 1;
                        println!("  skipped {name}: {e}");
                    }
                }
            }
            sort_newest_first(&mut rows, |e| SaveDate(e.info.date));
            println!("-- {tab:?} ({} rows)", rows.len());
            for e in &rows {
                let [y, mo, d, _, h, mi, s, ms] = e.info.date;
                println!(
                    "  type {} auto {} {y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{ms:03} label {:?} file {}",
                    e.info.save_file_type,
                    e.info.is_auto_save,
                    e.info.map_label.to_string_lossy(),
                    e.file
                );
            }
        }
        println!("header failures: {failed}");
    }
}
