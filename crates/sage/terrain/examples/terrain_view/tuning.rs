use std::path::Path;

pub(crate) use mint_sage_terrain::TuningFile;

pub(crate) fn open(override_path: Option<&Path>) -> TuningFile {
    TuningFile::open(override_path.map_or_else(
        || mint_sage_terrain::default_tuning_path("water-tuning.txt"),
        Path::to_path_buf,
    ))
}
