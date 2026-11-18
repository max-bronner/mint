use std::path::{Path, PathBuf};

use mint_image::dds::Dds;
use mint_image::{jpeg, tga};

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("MINT_CORPUS_IMAGES").map(PathBuf::from)
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(wanted))
}

fn images(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    if has_extension(&path, "tga")
                        || has_extension(&path, "dds")
                        || has_extension(&path, "jpg")
                        || has_extension(&path, "jpeg")
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

#[test]
#[ignore = "needs real images; set MINT_CORPUS_IMAGES to a directory of them"]
fn every_image_decodes_and_agrees_with_its_own_header() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS_IMAGES is not set, so there is nothing to validate against");
        return;
    };

    let paths = images(&root);
    assert!(
        !paths.is_empty(),
        "no .tga, .dds or .jpg files under {}",
        root.display()
    );

    let mut decoded = 0usize;
    let mut bc1 = 0usize;
    let mut bc3 = 0usize;
    let mut tga_files = 0usize;
    let mut jpeg_files = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut linear_size_mismatch: Vec<String> = Vec::new();
    let mut chain_mismatch: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            failures.push(format!("{}: could not be read", path.display()));
            continue;
        };

        if has_extension(path, "dds") {
            let dds = match Dds::parse(&bytes) {
                Ok(dds) => dds,
                Err(error) => {
                    failures.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            match dds.format() {
                mint_image::dds::BlockFormat::Bc1 => bc1 += 1,
                mint_image::dds::BlockFormat::Bc3 => bc3 += 1,
            }

            if dds.declared_linear_size() != 0
                && u64::from(dds.declared_linear_size())
                    != u64::try_from(dds.computed_linear_size()).unwrap_or(u64::MAX)
            {
                linear_size_mismatch.push(format!(
                    "{}: declares {} bytes for the top level, computed {}",
                    path.display(),
                    dds.declared_linear_size(),
                    dds.computed_linear_size()
                ));
            }

            let chain: usize = dds
                .levels()
                .iter()
                .map(|level| level.blocks().len())
                .fold(0usize, |sum, len| sum.saturating_add(len));
            if chain.saturating_add(128) != bytes.len() {
                chain_mismatch.push(format!(
                    "{}: {} bytes of mip chain plus a 128-byte header, file is {}",
                    path.display(),
                    chain,
                    bytes.len()
                ));
            }

            match dds.decode() {
                Ok(image) => {
                    assert_eq!(image.width(), dds.width());
                    assert_eq!(image.height(), dds.height());
                    decoded += 1;
                }
                Err(error) => failures.push(format!("{}: decode: {error}", path.display())),
            }
            continue;
        }

        if has_extension(path, "jpg") || has_extension(path, "jpeg") {
            jpeg_files += 1;
            match jpeg::decode(&bytes) {
                Ok(image) => {
                    assert_eq!(
                        u64::from(image.width()) * u64::from(image.height()) * 4,
                        image.pixels().len() as u64
                    );
                    decoded += 1;
                }
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
            continue;
        }

        tga_files += 1;
        let info = match tga::info(&bytes) {
            Ok(info) => info,
            Err(error) => {
                failures.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        match tga::decode(&bytes) {
            Ok(image) => {
                assert_eq!(image.width(), u32::from(info.width));
                assert_eq!(image.height(), u32::from(info.height));
                decoded += 1;
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }

    println!(
        "{}: {decoded} of {} images decoded \
         ({tga_files} tga, {jpeg_files} jpeg, {bc1} DXT1, {bc3} DXT5)",
        root.display(),
        paths.len()
    );
    for line in failures
        .iter()
        .chain(&linear_size_mismatch)
        .chain(&chain_mismatch)
    {
        println!("  {line}");
    }

    assert!(failures.is_empty(), "{} failures", failures.len());
    assert!(
        linear_size_mismatch.is_empty(),
        "{} files disagree with their own declared top-level size",
        linear_size_mismatch.len()
    );
    assert!(
        chain_mismatch.is_empty(),
        "{} files have a mip chain that does not fill the payload",
        chain_mismatch.len()
    );
}
