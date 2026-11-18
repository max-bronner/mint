use mint_sage_ini::lex::{decode_latin1, lex, Directive, Lexed};
use mint_sage_ini::{
    expand_macros, resolve_include, BlockTable, Document, HouseColorTable, MacroTable,
    MemorySources, MultiplayerColorIndex, NameFieldIndex, ObjectIndex, Reader, Sources,
};
use mint_vfs::Vfs;

pub const INI_ROOT: &str = "data/ini/";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectIndexReport {
    pub entries: usize,
    pub top_level: usize,
    pub parsed: usize,
    pub failures: Vec<String>,
}

#[must_use]
pub fn load_object_index(vfs: &Vfs) -> (ObjectIndex, ObjectIndexReport) {
    let (index, _, report) = load_object_index_inner(vfs, None);
    (index, report)
}

#[must_use]
pub fn load_object_index_with_names(
    vfs: &Vfs,
    siblings: &[&str],
) -> (ObjectIndex, NameFieldIndex, ObjectIndexReport) {
    load_object_index_inner(vfs, Some(siblings))
}

fn load_object_index_inner(
    vfs: &Vfs,
    names: Option<&[&str]>,
) -> (ObjectIndex, NameFieldIndex, ObjectIndexReport) {
    let mut report = ObjectIndexReport::default();
    let mut sources = MemorySources::new();
    for path in vfs.paths() {
        if !path.as_str().starts_with(INI_ROOT) {
            continue;
        }
        let Ok(bytes) = vfs.read_path(path) else {
            continue;
        };
        report.entries = report.entries.saturating_add(1);
        sources.insert(path.as_str(), bytes);
    }

    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let included = included_documents(&sources, &paths);

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let mut documents = Vec::new();
    for path in paths.iter().filter(|path| !included.contains(*path)) {
        report.top_level = report.top_level.saturating_add(1);
        match reader.read(path, &table) {
            Ok(document) => {
                report.parsed = report.parsed.saturating_add(1);
                documents.push(document);
            }
            Err(error) => report.failures.push(format!("{path}: {error}")),
        }
    }
    let macros = expand_macros(&mut documents);
    let index = object_index_from_documents(&documents, &macros);
    let mut name_fields = NameFieldIndex::with_siblings(names.unwrap_or_default());
    if names.is_some() {
        for document in &documents {
            name_fields.insert_document(document);
        }
    }
    (index, name_fields, report)
}

#[must_use]
pub fn object_index_from_documents(documents: &[Document], macros: &MacroTable) -> ObjectIndex {
    let mut index = ObjectIndex::new();
    for document in documents {
        index.insert_document_with(document, macros);
    }
    index
}

pub const HOUSE_COLOR_INI: &str = "data/ini/housecolor.ini";

pub const MULTIPLAYER_INI: &str = "data/ini/multiplayer.ini";

fn read_standalone(vfs: &Vfs, path: &str) -> Result<Document, String> {
    let Ok(bytes) = vfs.read(path) else {
        return Err(format!("{path}: not present in any mounted archive"));
    };
    let mut sources = MemorySources::new();
    sources.insert(path, bytes.to_vec());
    Reader::new(&sources)
        .read(path, &BlockTable::corpus_path())
        .map_err(|error| format!("{path}: {error}"))
}

#[must_use]
pub fn load_house_colors(vfs: &Vfs) -> (HouseColorTable, Option<String>) {
    let mut table = HouseColorTable::new();
    match read_standalone(vfs, HOUSE_COLOR_INI) {
        Ok(document) => {
            table.insert_document(&document);
            (table, None)
        }
        Err(reason) => (table, Some(reason)),
    }
}

#[must_use]
pub fn load_multiplayer_colors(vfs: &Vfs) -> (MultiplayerColorIndex, Option<String>) {
    let mut index = MultiplayerColorIndex::new();
    match read_standalone(vfs, MULTIPLAYER_INI) {
        Ok(document) => {
            index.insert_document(&document);
            (index, None)
        }
        Err(reason) => (index, Some(reason)),
    }
}

fn included_documents(sources: &MemorySources, paths: &[String]) -> Vec<String> {
    let mut included: Vec<String> = Vec::new();
    for path in paths {
        let Some(bytes) = Sources::read(sources, path) else {
            continue;
        };
        for item in lex(&decode_latin1(&bytes)) {
            let Lexed::Directive {
                directive: Directive::Include(target),
                ..
            } = item
            else {
                continue;
            };
            included.push(resolve_include(path, target));
        }
    }
    included.sort();
    included.dedup();
    included
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use mint_vfs::VfsPath;

    struct BytesMount(Vec<(VfsPath, Vec<u8>)>);

    impl mint_vfs::Mount for BytesMount {
        fn label(&self) -> &str {
            "bytes"
        }
        fn entries(&self) -> Vec<VfsPath> {
            self.0.iter().map(|(path, _)| path.clone()).collect()
        }
        fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
            self.0
                .iter()
                .find(|(candidate, _)| candidate == path)
                .map(|(_, bytes)| bytes.clone())
                .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })
        }
    }

    fn vfs_with(entries: &[(&str, &str)]) -> Vfs {
        let mut vfs = Vfs::new();
        vfs.mount(Arc::new(BytesMount(
            entries
                .iter()
                .map(|(path, text)| (VfsPath::new(path), text.as_bytes().to_vec()))
                .collect(),
        )));
        vfs
    }

    #[test]
    fn the_colour_files_read_their_own_blocks() {
        let vfs = vfs_with(&[
            (
                MULTIPLAYER_INI,
                "MultiplayerColor ColorRed\n  RGBColor = R:255 G:0 B:0\nEnd\n",
            ),
            (
                HOUSE_COLOR_INI,
                "HouseColor\n  BaseTexture = AnyUnit.tga\n  HouseTexture = HC_AnyUnit.tga\nEnd\n",
            ),
        ]);
        let (palette, error) = load_multiplayer_colors(&vfs);
        assert_eq!((palette.len(), error), (1, None));
        let (table, error) = load_house_colors(&vfs);
        assert_eq!((table.len(), error), (1, None));
    }

    #[test]
    fn an_included_document_is_not_read_a_second_time_on_its_own() {
        let vfs = vfs_with(&[
            ("data/ini/object/root.ini", "#include \"leaf.inc\"\n"),
            (
                "data/ini/object/leaf.inc",
                "Object Thing\n  Draw = W3DTreeDraw ModuleTag_01\n    ModelName = AThing\n  \
                 End\nEnd\n",
            ),
        ]);
        let (index, report) = load_object_index(&vfs);
        assert_eq!(report.entries, 2, "both files are read out of the vfs");
        assert_eq!(report.top_level, 1, "only the includer is a document");
        assert_eq!(report.parsed, 1);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(index.len(), 1);
        assert_eq!(
            index.redefinitions(),
            0,
            "reading the include twice would make this 1"
        );
        assert_eq!(index.lookup("thing").model(), Some("AThing"));
    }

    #[test]
    fn the_vfs_load_equals_the_index_built_from_parsed_documents() {
        let files = [
            (
                "data/ini/gamedata.ini",
                "#define HURT_STATES DAMAGED SNOW\n#define BIG_SCALE 2.5\n",
            ),
            ("data/ini/object/root.ini", "#include \"leaf.inc\"\n"),
            (
                "data/ini/object/leaf.inc",
                "Object Thing\n  Scale = BIG_SCALE\n  Draw = W3DScriptedModelDraw ModuleTag_01\n    \
                 DefaultModelConditionState\n      Model = AThing\n    End\n    \
                 ModelConditionState = HURT_STATES\n      Model = AThingHurt\n    End\n  \
                 End\nEnd\n",
            ),
        ];
        let (from_vfs, report) = load_object_index(&vfs_with(&files));
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let mut sources = MemorySources::new();
        for (path, text) in files {
            sources.insert(path, text.as_bytes().to_vec());
        }
        let reader = Reader::new(&sources);
        let table = BlockTable::corpus_path();
        let mut documents: Vec<Document> = ["data/ini/gamedata.ini", "data/ini/object/root.ini"]
            .iter()
            .map(|path| reader.read(path, &table).unwrap())
            .collect();
        let macros = expand_macros(&mut documents);
        let from_documents = object_index_from_documents(&documents, &macros);

        assert_eq!(from_vfs.len(), 1);
        assert_eq!(format!("{from_vfs:?}"), format!("{from_documents:?}"));
    }

    #[test]
    fn only_documents_under_the_ini_root_are_read() {
        let vfs = vfs_with(&[
            (
                "data/ini/object/a.ini",
                "Object Inside\n  Draw = W3DTreeDraw T\n    ModelName = M\n  End\nEnd\n",
            ),
            (
                "art/w3d/no/notini.ini",
                "Object Outside\n  Draw = W3DTreeDraw T\n    ModelName = M\n  End\nEnd\n",
            ),
        ]);
        let (index, report) = load_object_index(&vfs);
        assert_eq!(report.entries, 1);
        assert_eq!(index.len(), 1);
        assert!(index.lookup("outside").model().is_none());
    }
}
