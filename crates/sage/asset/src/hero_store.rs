use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use mint_sage_cah::{file_name, parse, stats, write, HeroRecord};
use mint_vfs::Vfs;

const SYSTEM_PREFIX: &str = "data/systemheroes/";
const MAX_HERO_FILE: u64 = 1 << 20;
const MY_HERO_FILE: &str = "MyHero.dat";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroSource {
    System,
    User,
}

#[derive(Debug, Clone)]
pub struct StoredHero {
    pub record: HeroRecord,
    pub source: HeroSource,
    pub file: String,
}

#[derive(Debug)]
pub struct HeroStore {
    own_dir: PathBuf,
    heroes: Vec<StoredHero>,
    skipped: Vec<String>,
}

fn is_hero_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("myhero") && lower.ends_with(".cah")
}

impl HeroStore {
    pub fn load(
        vfs: &Vfs,
        own_dir: impl Into<PathBuf>,
        read_only_dirs: &[PathBuf],
    ) -> io::Result<Self> {
        let own_dir = own_dir.into();
        if read_only_dirs.iter().any(|d| same_dir(d, &own_dir)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the writable hero directory is also listed as read-only",
            ));
        }
        let mut store = Self {
            own_dir,
            heroes: Vec::new(),
            skipped: Vec::new(),
        };

        let mut system: Vec<_> = vfs
            .paths_under(SYSTEM_PREFIX)
            .filter(|p| {
                let name = p.as_str().rsplit('/').next().unwrap_or("");
                is_hero_file(name)
            })
            .cloned()
            .collect();
        system.sort();
        for path in system {
            let name = path.as_str().rsplit('/').next().unwrap_or("").to_owned();
            match vfs.read_path(&path) {
                Ok(bytes) => store.add(&name, &bytes, HeroSource::System),
                Err(e) => store.skipped.push(format!("{}: {e}", path.as_str())),
            }
        }

        let dirs: Vec<PathBuf> = std::iter::once(store.own_dir.clone())
            .chain(read_only_dirs.iter().cloned())
            .collect();
        for dir in dirs {
            let mut files: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    is_hero_file(&name).then(|| (name, e.path()))
                })
                .collect();
            files.sort();
            for (name, path) in files {
                match read_capped(&path) {
                    Ok(bytes) => store.add(&name, &bytes, HeroSource::User),
                    Err(e) => store.skipped.push(format!("{}: {e}", path.display())),
                }
            }
        }
        Ok(store)
    }

    fn add(&mut self, file: &str, bytes: &[u8], source: HeroSource) {
        match parse(bytes) {
            Ok(p) if p.crc_ok => {
                let duplicate = source == HeroSource::User
                    && p.record.id.is_some()
                    && self.heroes.iter().any(|h| h.record.id == p.record.id);
                if !duplicate {
                    self.heroes.push(StoredHero {
                        record: p.record,
                        source,
                        file: file.to_owned(),
                    });
                }
            }
            Ok(_) => self.skipped.push(format!("{file}: checksum mismatch")),
            Err(e) => self.skipped.push(format!("{file}: {e}")),
        }
    }

    #[must_use]
    pub fn heroes(&self) -> &[StoredHero] {
        &self.heroes
    }

    #[must_use]
    pub fn skipped(&self) -> &[String] {
        &self.skipped
    }

    #[must_use]
    pub fn own_dir(&self) -> &Path {
        &self.own_dir
    }

    pub fn save(&mut self, mut record: HeroRecord) -> io::Result<()> {
        let id = record
            .id_str()
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "hero id is empty or not alphanumeric",
                )
            })?;
        record.is_system = Some(false);
        let file = file_name(&id);
        std::fs::create_dir_all(&self.own_dir)?;
        std::fs::write(self.own_dir.join(&file), write(&record))?;
        let stored = StoredHero {
            record,
            source: HeroSource::User,
            file,
        };
        match self
            .heroes
            .iter_mut()
            .find(|h| h.source == HeroSource::User && h.record.id == stored.record.id)
        {
            Some(slot) => *slot = stored,
            None => self.heroes.push(stored),
        }
        Ok(())
    }

    pub fn write_my_hero(&self, record: &HeroRecord) -> io::Result<bool> {
        if record.id.as_ref().is_none_or(Vec::is_empty) {
            return Ok(false);
        }
        let mut record = record.clone();
        record.object_id = 0;
        std::fs::create_dir_all(&self.own_dir)?;
        std::fs::write(self.own_dir.join(MY_HERO_FILE), write(&record))?;
        Ok(true)
    }

    pub fn delete(&mut self, id: &str) -> io::Result<bool> {
        if !self.forget(id) {
            return Ok(false);
        }
        self.remove_file(id)?;
        Ok(true)
    }

    pub fn forget(&mut self, id: &str) -> bool {
        let Some(at) = self
            .heroes
            .iter()
            .position(|h| h.source == HeroSource::User && h.record.id_str().as_deref() == Some(id))
        else {
            return false;
        };
        self.heroes.remove(at);
        true
    }

    pub fn remove_file(&self, id: &str) -> io::Result<()> {
        match std::fs::remove_file(self.own_dir.join(file_name(id))) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

impl HeroStore {
    #[must_use]
    pub fn stats(&self, id: &str) -> BTreeMap<String, i32> {
        read_stats(&self.own_dir, id)
    }
}

fn is_plain_name(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains(['/', '\\', ':'])
}

#[must_use]
pub fn read_stats(dir: &Path, id: &str) -> BTreeMap<String, i32> {
    if !is_plain_name(id) {
        return BTreeMap::new();
    }
    read_capped(&dir.join(id))
        .map(|bytes| stats::parse(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

pub fn write_stats(dir: &Path, id: &str, counters: &BTreeMap<String, i32>) -> io::Result<()> {
    if !is_plain_name(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a plain file name",
        ));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(id), stats::write(counters))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn read_capped(path: &Path) -> io::Result<Vec<u8>> {
    if std::fs::metadata(path)?.len() > MAX_HERO_FILE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
    }
    std::fs::read(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_vfs::{DirMount, Mount};
    use std::sync::Arc;

    fn record(id: &str, name: &str) -> HeroRecord {
        HeroRecord {
            name: name.encode_utf16().collect(),
            id: Some(id.as_bytes().to_vec()),
            ..HeroRecord::default()
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mint-hero-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn system_heroes_list_before_user_heroes_and_bad_files_are_skipped() {
        let root = scratch("list");
        let sys = root.join("game/Data/SystemHeroes");
        std::fs::create_dir_all(&sys).unwrap();
        let mut sys_rec = record("S1", "Shipped");
        sys_rec.is_system = Some(true);
        std::fs::write(sys.join("MyHero_S1.cah"), write(&sys_rec)).unwrap();
        let mut bad = write(&record("S2", "Broken"));
        let last = bad.len() - 1;
        bad[last] ^= 1;
        std::fs::write(sys.join("MyHero_S2.cah"), bad).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(Arc::new(DirMount::new(root.join("game")).unwrap()) as Arc<dyn Mount>);

        let own = root.join("own");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("MyHero_U1.cah"), write(&record("U1", "Mine"))).unwrap();
        let store = HeroStore::load(&vfs, &own, &[]).unwrap();
        let names: Vec<_> = store
            .heroes()
            .iter()
            .map(|h| h.record.name_string())
            .collect();
        assert_eq!(names, ["Shipped", "Mine"]);
        assert_eq!(store.skipped().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hero_reads_its_stat_file_by_id_and_nothing_outside_the_own_directory() {
        let root = scratch("stats");
        let own = root.join("own");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(
            own.join("T1"),
            "COUNT_A = 3
COUNT_B = 11
",
        )
        .unwrap();
        std::fs::write(
            root.join("T2"),
            "COUNT_A = 5
",
        )
        .unwrap();
        let store = HeroStore::load(&Vfs::new(), &own, &[]).unwrap();
        let stats = store.stats("T1");
        assert_eq!(stats.get("COUNT_A"), Some(&3));
        assert_eq!(stats.get("COUNT_B"), Some(&11));
        assert!(store.stats("missing").is_empty());
        assert!(store.stats("../T2").is_empty());
        assert!(store.stats("..").is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_writes_only_the_own_directory_and_delete_removes_it() {
        let root = scratch("write");
        let retail = root.join("retail");
        std::fs::create_dir_all(&retail).unwrap();
        let own = root.join("own");
        let mut store = HeroStore::load(&Vfs::new(), &own, std::slice::from_ref(&retail)).unwrap();
        store.save(record("A1", "First")).unwrap();
        store.save(record("A1", "Renamed")).unwrap();
        assert_eq!(store.heroes().len(), 1);
        assert_eq!(store.heroes()[0].record.name_string(), "Renamed");
        assert!(own.join("MyHero_A1.cah").exists());
        assert_eq!(std::fs::read_dir(&retail).unwrap().count(), 0);
        assert!(store.save(record("../x", "Bad")).is_err());
        assert!(store.delete("A1").unwrap());
        assert!(!own.join("MyHero_A1.cah").exists());
        assert!(HeroStore::load(&Vfs::new(), &retail, std::slice::from_ref(&retail)).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_chosen_hero_is_written_with_a_cleared_object_id_and_only_when_it_has_an_id() {
        let root = scratch("my-hero");
        let store = HeroStore::load(&Vfs::new(), &root, &[]).unwrap();
        let mut chosen = record("B2", "Chosen");
        chosen.object_id = 77;
        assert!(store.write_my_hero(&chosen).unwrap());
        let written = parse(&std::fs::read(root.join(MY_HERO_FILE)).unwrap()).unwrap();
        assert!(written.crc_ok);
        assert_eq!(written.record.object_id, 0);
        assert_eq!(written.record.name_string(), "Chosen");
        std::fs::remove_file(root.join(MY_HERO_FILE)).unwrap();
        chosen.id = Some(Vec::new());
        assert!(!store.write_my_hero(&chosen).unwrap());
        assert!(!root.join(MY_HERO_FILE).exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
