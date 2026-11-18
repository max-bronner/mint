use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::{ChannelData, Model, ShaderMaterialValue, Value};

fn archives(root: &Path) -> Vec<PathBuf> {
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
                    if has_extension(&path, "big") {
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

fn loose_models(root: &Path) -> Vec<PathBuf> {
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
                    if has_extension(&path, "w3d") {
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

fn has_extension(path: &Path, want: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(want))
}

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("MINT_CORPUS").map(PathBuf::from)
}

#[derive(Default)]
struct Totals {
    files_seen: usize,
    files_parsed: usize,
    bytes: u64,

    meshes: usize,
    vertices: usize,
    triangles: usize,
    skinned_meshes: usize,
    hierarchies: usize,
    pivots: usize,
    hlods: usize,
    lod_arrays: usize,
    animations: usize,
    channel_chunks: u64,
    shader_materials: usize,

    failures: Vec<String>,
    count_disagreements: Vec<String>,
    bad_parents: Vec<String>,
    attribute_length_mismatches: Vec<String>,
    weight_sum_anomalies: Vec<String>,
    two_bone_vertices: usize,
    zero_weight_vertices: usize,
    unordered_hierarchies: Vec<String>,
    unknown_property_tags: BTreeMap<u32, usize>,
    chunk_ids: BTreeMap<u32, u64>,
}

impl Totals {
    fn absorb(&mut self, name: &str, bytes: &[u8]) {
        self.files_seen += 1;
        self.bytes += bytes.len() as u64;

        let model = match Model::parse(bytes) {
            Ok(model) => model,
            Err(error) => {
                self.failures.push(format!("{name}: {error}"));
                return;
            }
        };
        self.files_parsed += 1;

        for (kind, count) in &model.chunk_counts {
            *self.chunk_ids.entry(kind.0).or_default() += u64::from(*count);
        }

        for mesh in &model.meshes {
            self.meshes += 1;
            self.vertices += mesh.vertex_count();
            self.triangles += mesh.triangle_count();
            if mesh.header.is_skin() {
                self.skinned_meshes += 1;
            }
            if !mesh.counts_agree() {
                self.count_disagreements.push(format!(
                    "{name} :: {}: header says {} verts / {} tris, arrays hold {} / {}",
                    mesh.name(),
                    mesh.header.num_vertices,
                    mesh.header.num_tris,
                    mesh.vertex_count(),
                    mesh.triangle_count(),
                ));
            }

            for influence in mesh.influences.iter().flatten() {
                let sum = u32::from(influence.weights[0]) + u32::from(influence.weights[1]);
                match sum {
                    0 => self.zero_weight_vertices += 1,
                    100 => {
                        if influence.weights[1] != 0 {
                            self.two_bone_vertices += 1;
                        }
                    }
                    other => self.weight_sum_anomalies.push(format!(
                        "{name} :: {}: bones {:?} weights {:?} sum to {other}",
                        mesh.name(),
                        influence.bones,
                        influence.weights,
                    )),
                }
            }

            for (label, len) in [
                ("TANGENTS", mesh.tangents.len()),
                ("BITANGENTS", mesh.bitangents.len()),
                ("VERTICES_2", mesh.positions2.len()),
                ("VERTEX_NORMALS_2", mesh.normals2.len()),
            ] {
                if len != 0 && len != mesh.vertex_count() {
                    self.attribute_length_mismatches.push(format!(
                        "{name} :: {}: {label} holds {len} entries for {} vertices",
                        mesh.name(),
                        mesh.vertex_count(),
                    ));
                }
            }

            self.shader_materials += mesh.shader_materials.len();
            for material in &mesh.shader_materials {
                for property in &material.properties {
                    if let ShaderMaterialValue::Unknown { tag, .. } = property.value {
                        *self.unknown_property_tags.entry(tag).or_default() += 1;
                    }
                }
            }
        }

        if let Some(hierarchy) = &model.hierarchy {
            self.hierarchies += 1;
            self.pivots += hierarchy.len();
            let bad = hierarchy.unusable_parents();
            if !bad.is_empty() {
                self.bad_parents
                    .push(format!("{name}: pivots {bad:?} have unusable parents"));
            }
            if !hierarchy.parents_precede_children() {
                self.unordered_hierarchies.push(name.to_owned());
            }
        }

        for hlod in &model.hlods {
            self.hlods += 1;
            self.lod_arrays += hlod.lods.len();
        }

        for animation in &model.animations {
            self.animations += 1;
            self.channel_chunks += u64::from(animation.channel_chunks);
        }
    }

    fn report(&self, root: &Path) {
        println!("== {} ==", root.display());
        println!(
            "  {} of {} .w3d files parsed ({:.1} MiB)",
            self.files_parsed,
            self.files_seen,
            self.bytes as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  {} meshes, {} vertices, {} triangles ({} skinned)",
            self.meshes, self.vertices, self.triangles, self.skinned_meshes
        );
        println!(
            "  {} skeletons ({} pivots), {} HLODs, {} LOD arrays",
            self.hierarchies, self.pivots, self.hlods, self.lod_arrays
        );
        println!(
            "  {} shader materials, {} animations carrying {} channel chunks \
             (decoded: see animation_channels_reproduce_the_research_document)",
            self.shader_materials, self.animations, self.channel_chunks
        );
        println!("  cross-checks:");
        println!(
            "    mesh header counts == array lengths:   {} of {} meshes",
            self.meshes.saturating_sub(self.count_disagreements.len()),
            self.meshes
        );
        println!(
            "    every pivot parent resolves to a root: {} of {} skeletons",
            self.hierarchies.saturating_sub(self.bad_parents.len()),
            self.hierarchies
        );
        println!(
            "    extension arrays are one Vec3/vertex:  {} mismatches",
            self.attribute_length_mismatches.len()
        );
        println!(
            "    influence weights sum to 100 or 0:     {} anomalies",
            self.weight_sum_anomalies.len()
        );
        println!(
            "  skinning: {} vertices use a second bone, {} store both weights as zero",
            self.two_bone_vertices, self.zero_weight_vertices
        );
        println!(
            "  {} distinct chunk ids seen; {} hierarchies not stored parents-first",
            self.chunk_ids.len(),
            self.unordered_hierarchies.len()
        );
        if !self.unknown_property_tags.is_empty() {
            println!(
                "  unknown shader-material property tags: {:?}",
                self.unknown_property_tags
            );
        }
        if !self.failures.is_empty() {
            println!("  {} failures:", self.failures.len());
            for failure in &self.failures {
                println!("    {failure}");
            }
        }
        for line in self
            .count_disagreements
            .iter()
            .chain(&self.bad_parents)
            .chain(&self.attribute_length_mismatches)
            .chain(&self.weight_sum_anomalies)
            .take(50)
        {
            println!("    CROSS-CHECK: {line}");
        }
    }
}

fn walk(root: &Path) -> Totals {
    let mut totals = Totals::default();

    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for entry in archive.entries() {
            if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                continue;
            }
            match archive.read_entry(entry) {
                Ok(bytes) => totals.absorb(&format!("{label}::{}", entry.name), &bytes),
                Err(error) => {
                    totals.files_seen += 1;
                    totals
                        .failures
                        .push(format!("{label}::{}: {error}", entry.name));
                }
            }
        }
    }

    for path in loose_models(root) {
        match std::fs::read(&path) {
            Ok(bytes) => totals.absorb(&path.display().to_string(), &bytes),
            Err(error) => {
                totals.files_seen += 1;
                totals.failures.push(format!("{}: {error}", path.display()));
            }
        }
    }

    totals
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_shipped_model_parses_and_agrees_with_its_own_header() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let totals = walk(&root);
    totals.report(&root);

    assert!(
        totals.files_seen > 0,
        "no .w3d files under {}; is MINT_CORPUS pointing at an install?",
        root.display()
    );

    assert!(
        totals.count_disagreements.is_empty(),
        "{} of {} meshes disagree with their own header counts",
        totals.count_disagreements.len(),
        totals.meshes
    );
    assert!(
        totals.bad_parents.is_empty(),
        "{} of {} skeletons have an unusable parent index",
        totals.bad_parents.len(),
        totals.hierarchies
    );
    assert!(
        totals.attribute_length_mismatches.is_empty(),
        "{} per-vertex extension arrays are not one Vec3 per vertex, which is \
         the only evidence for how those chunks are laid out",
        totals.attribute_length_mismatches.len()
    );
    assert!(
        totals.weight_sum_anomalies.is_empty(),
        "{} vertex influences have weights summing to neither 100 nor 0; the \
         two-u16-weight reading of VERTEX_INFLUENCES rests on this holding",
        totals.weight_sum_anomalies.len()
    );

    let floor = totals.files_seen.saturating_mul(99) / 100;
    assert!(
        totals.files_parsed >= floor,
        "only {} of {} files parsed, below the 99% regression floor",
        totals.files_parsed,
        totals.files_seen
    );
}

#[derive(Default)]
struct AnimationTotals {
    animations: u64,
    channels: u64,
    keys: u64,
    vector_len_disagreements: u64,

    time_coded_channels: u64,
    time_codes_sorted_and_in_range: u64,

    out_of_range_pivots: u64,
    channels_with_resolvable_hierarchy: u64,

    verbatim_quaternions: u64,
    verbatim_within_1e3: u64,
    reconstructed_quaternions: u64,
    reconstructed_within_1e2: u64,

    dense_channels: u64,
    dense_channels_not_starting_at_zero: u64,
    dense_channels_ending_early: u64,
    bit_channels: u64,
    bit_channels_not_starting_at_zero: u64,
    bit_channels_ending_early: u64,
    sparse_channels: u64,
    sparse_channels_not_starting_at_zero: u64,
    sparse_channels_ending_early: u64,
    adaptive_delta_channels: u64,
    adaptive_delta_channels_ending_early: u64,

    parse_failures: Vec<String>,
}

fn key_count(data: &ChannelData) -> u64 {
    match data {
        ChannelData::Dense { values, .. } => values.len() as u64,
        ChannelData::Bits {
            first_frame,
            last_frame,
            ..
        } => u64::from(last_frame.saturating_sub(*first_frame).saturating_add(1)),
        ChannelData::TimeCoded(keys) => keys.len() as u64,
        ChannelData::SparseBits { keys, .. } => keys.len() as u64,
        ChannelData::AdaptiveDelta { values } => values.len() as u64,
    }
}

fn frames_sorted_and_in_range(frames: impl Iterator<Item = u32>, frame_count: u32) -> bool {
    let mut prev: Option<u32> = None;
    let mut last = None;
    for f in frames {
        if let Some(p) = prev {
            if f <= p {
                return false;
            }
        }
        prev = Some(f);
        last = Some(f);
    }
    match last {
        Some(l) => l < frame_count,
        None => true,
    }
}

fn quat_unit_deviation(q: mint_sage_w3d::Quat) -> f32 {
    let len_sq = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    (len_sq.sqrt() - 1.0).abs()
}

impl AnimationTotals {
    fn absorb(&mut self, name: &str, bytes: &[u8], hierarchy_pivots: &BTreeMap<String, usize>) {
        let model = match Model::parse(bytes) {
            Ok(model) => model,
            Err(error) => {
                self.parse_failures.push(format!("{name}: {error}"));
                return;
            }
        };

        for anim in &model.animations {
            self.animations += 1;
            self.channels += anim.channels.len() as u64;

            let hierarchy_count = hierarchy_pivots.get(&anim.hierarchy_name.to_ascii_uppercase());
            if let Some(&count) = hierarchy_count {
                self.channels_with_resolvable_hierarchy += anim.channels.len() as u64;
                self.out_of_range_pivots += anim.out_of_range_pivots(count).len() as u64;
            }

            for channel in &anim.channels {
                self.keys += key_count(&channel.data);
                if !channel.vector_len_matches_type() {
                    self.vector_len_disagreements += 1;
                }

                let last_animated_frame = anim.num_frames.checked_sub(1);

                match &channel.data {
                    ChannelData::TimeCoded(keys) => {
                        self.time_coded_channels += 1;
                        if frames_sorted_and_in_range(keys.iter().map(|k| k.frame), anim.num_frames)
                        {
                            self.time_codes_sorted_and_in_range += 1;
                        }
                        if let (Some(first), Some(last)) = (keys.first(), keys.last()) {
                            self.sparse_channels += 1;
                            if first.frame != 0 {
                                self.sparse_channels_not_starting_at_zero += 1;
                            }
                            if last_animated_frame.is_some_and(|end| last.frame < end) {
                                self.sparse_channels_ending_early += 1;
                            }
                        }
                        for k in keys {
                            if let Value::Rotation(q) = k.value {
                                self.verbatim_quaternions += 1;
                                if quat_unit_deviation(q) < 1e-3 {
                                    self.verbatim_within_1e3 += 1;
                                }
                            }
                        }
                    }
                    ChannelData::SparseBits { keys, .. } => {
                        self.time_coded_channels += 1;
                        if frames_sorted_and_in_range(keys.iter().map(|(f, _)| *f), anim.num_frames)
                        {
                            self.time_codes_sorted_and_in_range += 1;
                        }
                        if let (Some((first, _)), Some((last, _))) = (keys.first(), keys.last()) {
                            self.sparse_channels += 1;
                            if *first != 0 {
                                self.sparse_channels_not_starting_at_zero += 1;
                            }
                            if last_animated_frame.is_some_and(|end| *last < end) {
                                self.sparse_channels_ending_early += 1;
                            }
                        }
                    }
                    ChannelData::Dense {
                        first_frame,
                        values,
                    } => {
                        if let Some(len) = u32::try_from(values.len()).ok().filter(|l| *l > 0) {
                            self.dense_channels += 1;
                            if *first_frame != 0 {
                                self.dense_channels_not_starting_at_zero += 1;
                            }
                            let last = first_frame.saturating_add(len).saturating_sub(1);
                            if last_animated_frame.is_some_and(|end| last < end) {
                                self.dense_channels_ending_early += 1;
                            }
                        }
                        for v in values {
                            if let Value::Rotation(q) = v {
                                self.verbatim_quaternions += 1;
                                if quat_unit_deviation(*q) < 1e-3 {
                                    self.verbatim_within_1e3 += 1;
                                }
                            }
                        }
                    }
                    ChannelData::AdaptiveDelta { values } => {
                        if let Some(len) = u32::try_from(values.len()).ok().filter(|l| *l > 0) {
                            self.adaptive_delta_channels += 1;
                            let last = len.saturating_sub(1);
                            if last_animated_frame.is_some_and(|end| last < end) {
                                self.adaptive_delta_channels_ending_early += 1;
                            }
                        }
                        for v in values {
                            if let Value::Rotation(q) = v {
                                self.reconstructed_quaternions += 1;
                                if quat_unit_deviation(*q) < 1e-2 {
                                    self.reconstructed_within_1e2 += 1;
                                }
                            }
                        }
                    }
                    ChannelData::Bits {
                        first_frame,
                        last_frame,
                        ..
                    } => {
                        self.bit_channels += 1;
                        if *first_frame != 0 {
                            self.bit_channels_not_starting_at_zero += 1;
                        }
                        if last_animated_frame.is_some_and(|end| *last_frame < end) {
                            self.bit_channels_ending_early += 1;
                        }
                    }
                }
            }
        }
    }
}

fn collect_hierarchy_pivot_counts(root: &Path) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut absorb_bytes = |bytes: &[u8]| {
        if let Ok(model) = Model::parse(bytes) {
            if let Some(h) = &model.hierarchy {
                let key = h.header.name.to_ascii_uppercase();
                counts
                    .entry(key)
                    .and_modify(|c: &mut usize| *c = (*c).max(h.len()))
                    .or_insert_with(|| h.len());
            }
        }
    };

    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        for entry in archive.entries() {
            if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                continue;
            }
            if let Ok(bytes) = archive.read_entry(entry) {
                absorb_bytes(&bytes);
            }
        }
    }
    for path in loose_models(root) {
        if let Ok(bytes) = std::fs::read(&path) {
            absorb_bytes(&bytes);
        }
    }
    counts
}

fn walk_animations(root: &Path, hierarchy_pivots: &BTreeMap<String, usize>) -> AnimationTotals {
    let mut totals = AnimationTotals::default();

    for path in archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for entry in archive.entries() {
            if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                continue;
            }
            match archive.read_entry(entry) {
                Ok(bytes) => totals.absorb(
                    &format!("{label}::{}", entry.name),
                    &bytes,
                    hierarchy_pivots,
                ),
                Err(error) => totals
                    .parse_failures
                    .push(format!("{label}::{}: {error}", entry.name)),
            }
        }
    }
    for path in loose_models(root) {
        match std::fs::read(&path) {
            Ok(bytes) => totals.absorb(&path.display().to_string(), &bytes, hierarchy_pivots),
            Err(error) => totals
                .parse_failures
                .push(format!("{}: {error}", path.display())),
        }
    }

    totals
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn animation_channels_reproduce_the_research_document() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let hierarchy_pivots = collect_hierarchy_pivot_counts(&root);
    let totals = walk_animations(&root, &hierarchy_pivots);

    println!("== animation decode: {} ==", root.display());
    println!(
        "  {} animations, {} channels, {} keys/frames decoded",
        totals.animations, totals.channels, totals.keys
    );
    println!(
        "  {} channel-decode parse failures",
        totals.parse_failures.len()
    );
    println!(
        "  vector length matches type: {} disagreements",
        totals.vector_len_disagreements
    );
    println!(
        "  time codes sorted and in range: {} of {}",
        totals.time_codes_sorted_and_in_range, totals.time_coded_channels
    );
    println!(
        "  out-of-range pivots: {} (across {} channels with a resolvable hierarchy)",
        totals.out_of_range_pivots, totals.channels_with_resolvable_hierarchy
    );
    let verbatim_pct = if totals.verbatim_quaternions == 0 {
        100.0
    } else {
        100.0 * totals.verbatim_within_1e3 as f64 / totals.verbatim_quaternions as f64
    };
    println!(
        "  verbatim quaternions within 1e-3: {:.3}% ({} of {})",
        verbatim_pct, totals.verbatim_within_1e3, totals.verbatim_quaternions
    );
    let reconstructed_pct = if totals.reconstructed_quaternions == 0 {
        100.0
    } else {
        100.0 * totals.reconstructed_within_1e2 as f64 / totals.reconstructed_quaternions as f64
    };
    println!(
        "  adaptive-delta quaternions within 1e-2: {:.3}% ({} of {})",
        reconstructed_pct, totals.reconstructed_within_1e2, totals.reconstructed_quaternions
    );
    for failure in totals.parse_failures.iter().take(20) {
        println!("    FAILURE: {failure}");
    }

    println!("  channel range vs. animation length, by encoding:");
    println!(
        "    0x202 dense:      {} channels, {} not starting at 0, {} ending early",
        totals.dense_channels,
        totals.dense_channels_not_starting_at_zero,
        totals.dense_channels_ending_early
    );
    println!(
        "    0x203 bit:        {} channels, {} not starting at 0, {} ending early",
        totals.bit_channels,
        totals.bit_channels_not_starting_at_zero,
        totals.bit_channels_ending_early
    );
    println!(
        "    sparse (0x282/0x283/0x284 time-coded): {} channels, {} not starting at 0, {} ending early",
        totals.sparse_channels,
        totals.sparse_channels_not_starting_at_zero,
        totals.sparse_channels_ending_early
    );
    println!(
        "    adaptive-delta:   {} channels, {} ending early (never starts late - see doc)",
        totals.adaptive_delta_channels, totals.adaptive_delta_channels_ending_early
    );

    assert!(
        totals.parse_failures.len() <= 8,
        "{} channel-decode parse failures, more than the eight pre-existing known-corrupt files: {:?}",
        totals.parse_failures.len(),
        &totals.parse_failures
    );

    assert_eq!(
        totals.vector_len_disagreements, 0,
        "a channel's declared vector length disagreed with its type-derived one"
    );

    assert_eq!(
        totals.time_codes_sorted_and_in_range,
        totals.time_coded_channels,
        "{} of {} time-coded channels have out-of-order or out-of-range time codes",
        totals.time_coded_channels - totals.time_codes_sorted_and_in_range,
        totals.time_coded_channels
    );

    if totals.reconstructed_quaternions > 0 {
        assert!(
            reconstructed_pct >= 99.0,
            "only {reconstructed_pct:.3}% of reconstructed quaternions are within 1e-2 of unit \
             length, below the 99% floor"
        );
    }
    if totals.verbatim_quaternions > 0 {
        assert!(
            verbatim_pct >= 99.9,
            "only {verbatim_pct:.3}% of verbatim quaternions are within 1e-3 of unit length, \
             below the 99.9% floor"
        );
    }

    println!(
        "  expected from docs/research/w3d-animation.md: 7,076 animations, 433,536 channels, \
         40,387,665 keys, 912 out-of-range pivots, 163,466 time-coded channels"
    );
}
