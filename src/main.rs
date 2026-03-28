use snes_rom_hack::cli::{run_annotate_cli, run_asset_paths_report_cli, run_collect_trace_wrapper_cli, run_disasm_cli, run_evidence_cli, run_match_player_gfx_sheet_wrapper_cli, run_patch_player_gfx_wrapper_cli, run_phase2_cli, run_player_gfx_cli, run_replacement_cli, run_runtime_correlate_cli, run_template_wrapper_cli, run_usage_import_cli};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const INPUT_DIR: &str = "roms-original";
const OUTPUT_DIR: &str = "roms-modified";
const HEADER_CANDIDATES: [usize; 4] = [0x7FB0, 0xFFB0, 0x40_7FB0, 0x40_FFB0];
const PALETTE_WINDOW_BYTES: usize = 32;
const TILE_GRAPHICS_WINDOW_BYTES: usize = 256;
const TILEMAP_WINDOW_BYTES: usize = 512;
const ENTROPY_WINDOW_BYTES: usize = 1024;
const MAX_RESULTS_PER_CATEGORY: usize = 12;
const PALETTE_PREVIEW_LIMIT: usize = 8;
const TILE_GRAPHICS_PREVIEW_LIMIT: usize = 8;
const TILEMAP_PREVIEW_LIMIT: usize = 6;
const TILEMAP_COMBO_LIMIT: usize = 2;
const TILE_4BPP_PREVIEW_BYTES: usize = 4096;
const TILE_2BPP_PREVIEW_BYTES: usize = 2048;
const TILE_SIZE: usize = 8;

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run_cli() -> io::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.first().map(String::as_str), Some("disasm")) {
        return run_disasm_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("runtime-correlate")) {
        return run_runtime_correlate_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("usage-map-import")) {
        return run_usage_import_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("evidence-report")) {
        return run_evidence_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("annotate-evidence")) {
        return run_annotate_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("asset-paths")) {
        return run_asset_paths_report_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("phase2-analyze")) {
        return run_phase2_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("collect-trace")) {
        return run_collect_trace_wrapper_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("replacement-report")) {
        return run_replacement_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("player-gfx-report")) {
        return run_player_gfx_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("patch-player-gfx")) {
        return run_patch_player_gfx_wrapper_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("match-player-gfx-sheet")) {
        return run_match_player_gfx_sheet_wrapper_cli(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("match-sheet")) {
        return run_match_sheet(&args[1..]);
    }
    if matches!(args.first().map(String::as_str), Some("template")) {
        return run_template_wrapper_cli(&args[1..]);
    }

    run_analysis()
}

fn run_analysis() -> io::Result<()> {
    let input_dir = Path::new(INPUT_DIR);
    let output_dir = Path::new(OUTPUT_DIR);
    fs::create_dir_all(output_dir)?;

    let mut roms = list_roms(input_dir)?;
    roms.sort();

    if roms.is_empty() {
        println!("No .sfc files found in {}", input_dir.display());
        return Ok(());
    }

    for rom_path in roms {
        let raw_bytes = fs::read(&rom_path)?;
        let normalized = normalize_rom(&raw_bytes);
        let analysis = analyze_rom_with_metadata(
            &rom_path,
            &normalized.bytes,
            normalized.source_size,
            normalized.bytes.len(),
            normalized.had_copier_header,
        );
        let rom_output_dir = output_dir.join(&analysis.stem);
        fs::create_dir_all(&rom_output_dir)?;

        let report = render_report(&analysis);
        let report_path = rom_output_dir.join("analysis.txt");
        fs::write(&report_path, report)?;

        generate_previews(&analysis, &normalized.bytes, &rom_output_dir)?;

        println!(
            "analyzed {} -> {}",
            rom_path.display(),
            rom_output_dir.display()
        );
    }

    Ok(())
}

fn run_match_sheet(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut sheet_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--sheet" => {
                index += 1;
                sheet_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            unknown => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{unknown}`; expected `match-sheet --rom <path> --sheet <path> [--out <dir>]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `match-sheet`",
        )
    })?;
    let sheet_path = sheet_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--sheet <path>` for `match-sheet`",
        )
    })?;

    let rom_bytes = fs::read(&rom_path)?;
    let normalized = normalize_rom(&rom_bytes);
    let sheet = load_sheet_png(&sheet_path)?;
    let analysis = analyze_rom_with_metadata(
        &rom_path,
        &normalized.bytes,
        normalized.source_size,
        normalized.bytes.len(),
        normalized.had_copier_header,
    );
    let matches = match_sheet_to_rom(&sheet, &normalized.bytes, analysis.header.as_ref());

    let default_out = Path::new(OUTPUT_DIR).join("sheet-match").join(format!(
        "{}--{}",
        path_stem(&rom_path),
        path_stem(&sheet_path)
    ));
    let output_dir = out_dir.unwrap_or(default_out);
    fs::create_dir_all(&output_dir)?;

    let report_path = output_dir.join("report.txt");
    fs::write(
        &report_path,
        render_sheet_match_report(&sheet, &analysis, &matches),
    )?;

    let mask = render_sheet_match_mask(&sheet, &matches);
    let mask_path = output_dir.join("match-mask.png");
    write_png_rgba(&mask_path, mask.width, mask.height, &mask.pixels)?;

    println!(
        "matched sheet {} against {} -> {}",
        sheet_path.display(),
        rom_path.display(),
        output_dir.display()
    );

    Ok(())
}

fn list_roms(input_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut roms = Vec::new();
    collect_roms(input_dir, &mut roms)?;

    Ok(roms)
}

fn collect_roms(dir: &Path, roms: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_roms(&path, roms)?;
            continue;
        }

        let is_sfc = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sfc"));

        if is_sfc {
            roms.push(path);
        }
    }

    Ok(())
}

fn analyze_rom_with_metadata(
    path: &Path,
    bytes: &[u8],
    source_file_size: usize,
    normalized_size: usize,
    had_copier_header: bool,
) -> RomAnalysis {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("rom")
        .to_string();

    let header = detect_header(bytes);
    let palettes = scan_palettes(bytes);
    let tile_graphic_regions = scan_tile_graphics(bytes);
    let tilemaps = scan_tilemaps(bytes);
    let compressed_regions = scan_entropy_regions(bytes);

    RomAnalysis {
        path: path.display().to_string(),
        stem,
        file_size: source_file_size,
        normalized_size,
        had_copier_header,
        header,
        palettes,
        tile_graphic_regions,
        tilemaps,
        compressed_regions,
    }
}

fn normalize_rom(bytes: &[u8]) -> NormalizedRom {
    if bytes.len() >= 0x8000 && (bytes.len() & 0x7FFF) == 512 {
        NormalizedRom {
            bytes: bytes[512..].to_vec(),
            had_copier_header: true,
            source_size: bytes.len(),
        }
    } else {
        NormalizedRom {
            bytes: bytes.to_vec(),
            had_copier_header: false,
            source_size: bytes.len(),
        }
    }
}

fn analysis_mapping(analysis: &RomAnalysis) -> Option<CartMapping> {
    analysis
        .header
        .as_ref()
        .and_then(|header| header.mapping.cpu_map_mode())
}

fn generate_previews(
    analysis: &RomAnalysis,
    bytes: &[u8],
    rom_output_dir: &Path,
) -> io::Result<()> {
    let palettes_dir = rom_output_dir.join("palettes");
    let tiles_dir = rom_output_dir.join("tiles");
    let tilemaps_dir = rom_output_dir.join("tilemaps");
    fs::create_dir_all(&palettes_dir)?;
    fs::create_dir_all(&tiles_dir)?;
    fs::create_dir_all(&tilemaps_dir)?;

    let palette_candidates = analysis
        .palettes
        .iter()
        .take(PALETTE_PREVIEW_LIMIT)
        .collect::<Vec<_>>();
    let tile_candidates = analysis
        .tile_graphic_regions
        .iter()
        .take(TILE_GRAPHICS_PREVIEW_LIMIT)
        .collect::<Vec<_>>();
    let tilemap_candidates = analysis
        .tilemaps
        .iter()
        .take(TILEMAP_PREVIEW_LIMIT)
        .collect::<Vec<_>>();

    for (index, candidate) in palette_candidates.iter().enumerate() {
        let palette = read_palette(bytes, candidate.offset)?;
        let strip = render_palette_strip(&palette, 24, 8);
        let path = palettes_dir.join(format!(
            "palette_{index:02}_off_{:06X}.png",
            candidate.offset
        ));
        write_png_rgba(&path, strip.width, strip.height, &strip.pixels)?;
    }

    for (index, candidate) in tile_candidates.iter().enumerate() {
        let palette = palette_candidates
            .get(index % palette_candidates.len().max(1))
            .and_then(|palette_candidate| read_palette(bytes, palette_candidate.offset).ok())
            .unwrap_or_else(default_palette);

        let preview_4bpp = render_raw_tiles_preview(
            bytes,
            candidate.offset,
            TILE_4BPP_PREVIEW_BYTES,
            32,
            TileBitDepth::Bpp4,
            &palette,
        );
        let path_4bpp = tiles_dir.join(format!(
            "tiles4bpp_{index:02}_off_{:06X}.png",
            candidate.offset
        ));
        write_png_rgba(
            &path_4bpp,
            preview_4bpp.width,
            preview_4bpp.height,
            &preview_4bpp.pixels,
        )?;

        let preview_2bpp = render_raw_tiles_preview(
            bytes,
            candidate.offset,
            TILE_2BPP_PREVIEW_BYTES,
            32,
            TileBitDepth::Bpp2,
            &palette,
        );
        let path_2bpp = tiles_dir.join(format!(
            "tiles2bpp_{index:02}_off_{:06X}.png",
            candidate.offset
        ));
        write_png_rgba(
            &path_2bpp,
            preview_2bpp.width,
            preview_2bpp.height,
            &preview_2bpp.pixels,
        )?;
    }

    let tilemap_palettes = palette_candidates
        .iter()
        .take(TILEMAP_COMBO_LIMIT)
        .enumerate()
        .filter_map(|(index, candidate)| {
            read_palette(bytes, candidate.offset)
                .ok()
                .map(|palette| (index, candidate.offset, palette))
        })
        .collect::<Vec<_>>();

    let tilemap_tiles = tile_candidates
        .iter()
        .take(TILEMAP_COMBO_LIMIT)
        .enumerate()
        .map(|(index, candidate)| (index, candidate.offset))
        .collect::<Vec<_>>();

    for (tilemap_index, tilemap_candidate) in tilemap_candidates.iter().enumerate() {
        for (palette_index, palette_offset, palette) in &tilemap_palettes {
            for (tile_index, tile_offset) in &tilemap_tiles {
                let preview = render_tilemap_preview(
                    bytes,
                    tilemap_candidate.offset,
                    *tile_offset,
                    TileBitDepth::Bpp4,
                    palette,
                )?;

                let path = tilemaps_dir.join(format!(
                    "tilemap_{tilemap_index:02}_map_{:06X}_tiles_{tile_index:02}_{:06X}_pal_{palette_index:02}_{:06X}.png",
                    tilemap_candidate.offset, tile_offset, palette_offset
                ));
                write_png_rgba(&path, preview.width, preview.height, &preview.pixels)?;
            }
        }
    }

    Ok(())
}

fn path_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("input")
        .to_string()
}

fn load_sheet_png(path: &Path) -> io::Result<SheetImage> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(to_io_error)?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(to_io_error)?;
    let data = &buffer[..info.buffer_size()];

    let width = info.width as usize;
    let height = info.height as usize;

    let (rgba, indexed) = match info.color_type {
        png::ColorType::Indexed => {
            let palette = reader.info().palette.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "indexed PNG missing palette")
            })?;
            let trns = reader.info().trns.as_ref();
            let mut rgba = Vec::with_capacity(width * height * 4);
            let mut indices = Vec::with_capacity(width * height);
            for index in data {
                let index = *index as usize;
                let base = index * 3;
                if base + 2 >= palette.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "indexed PNG palette index out of bounds",
                    ));
                }
                let alpha = trns
                    .and_then(|trns| trns.get(index))
                    .copied()
                    .unwrap_or(255);
                rgba.extend_from_slice(&[
                    palette[base],
                    palette[base + 1],
                    palette[base + 2],
                    alpha,
                ]);
                indices.push(index as u8);
            }
            (rgba, Some(indices))
        }
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(width * height * 4);
            for chunk in data.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            (rgba, None)
        }
        png::ColorType::Rgba => (data.to_vec(), None),
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(width * height * 4);
            for value in data {
                rgba.extend_from_slice(&[*value, *value, *value, 255]);
            }
            (rgba, None)
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(width * height * 4);
            for chunk in data.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            (rgba, None)
        }
    };

    Ok(SheetImage {
        path: path.display().to_string(),
        width,
        height,
        rgba,
        indexed,
    })
}

fn match_sheet_to_rom(
    sheet: &SheetImage,
    rom_bytes: &[u8],
    header: Option<&HeaderCandidate>,
) -> SheetMatchResult {
    let mapping = header.and_then(|header| header.mapping.cpu_map_mode());
    let extracted = extract_sheet_tiles(sheet);
    let matched_tiles = extracted
        .tiles
        .iter()
        .filter(|tile| tile.encoded_4bpp.is_some())
        .count();

    let mut unique_patterns = Vec::<UniqueTilePattern>::new();
    let mut pattern_index = HashMap::<[u8; 32], usize>::new();
    for tile in &extracted.tiles {
        let Some(encoded) = tile.encoded_4bpp else {
            continue;
        };

        if let Some(existing) = pattern_index.get(&encoded).copied() {
            unique_patterns[existing].tile_indices.push(tile.tile_index);
            continue;
        }

        let next = unique_patterns.len();
        pattern_index.insert(encoded, next);
        unique_patterns.push(UniqueTilePattern {
            bytes: encoded,
            representative_tile_index: tile.tile_index,
            tile_indices: vec![tile.tile_index],
            palette_bank: tile.palette_bank,
            nonzero_pixels: count_nonzero_tile_pixels(&encoded),
            color_count: count_tile_colors(&encoded),
        });
    }

    let mut pattern_map: HashMap<[u8; 32], Vec<TileNeedle>> = HashMap::new();
    let mut searched_pattern_count = 0usize;
    for (pattern_id, pattern) in unique_patterns.iter().enumerate() {
        if !is_discriminative_pattern(pattern) {
            continue;
        }
        searched_pattern_count += 1;
        for (variant, bytes) in generate_tile_variants(&pattern.bytes) {
            pattern_map.entry(bytes).or_default().push(TileNeedle {
                pattern_id,
                tile_index: pattern.representative_tile_index,
                variant,
                palette_bank: pattern.palette_bank,
                sheet_occurrences: pattern.tile_indices.len(),
            });
        }
    }

    let mut hits = Vec::new();
    let mut matched_pattern_ids = HashSet::<usize>::new();
    for offset in (0..rom_bytes.len().saturating_sub(32)).step_by(32) {
        let mut key = [0u8; 32];
        key.copy_from_slice(&rom_bytes[offset..offset + 32]);
        if let Some(needles) = pattern_map.get(&key) {
            for needle in needles {
                matched_pattern_ids.insert(needle.pattern_id);
                hits.push(TileMatchHit {
                    rom_offset: offset,
                    cpu_address: format_cpu_address_for_offset(offset, mapping),
                    tile_index: needle.tile_index,
                    variant: needle.variant,
                    palette_bank: needle.palette_bank,
                    sheet_occurrences: needle.sheet_occurrences,
                });
            }
        }
    }

    let clusters = cluster_tile_hits(&hits);
    let mut matched_tile_indices = HashSet::<usize>::new();
    for pattern_id in matched_pattern_ids {
        matched_tile_indices.extend(unique_patterns[pattern_id].tile_indices.iter().copied());
    }

    SheetMatchResult {
        tile_columns: extracted.tile_columns,
        tile_rows: extracted.tile_rows,
        total_tiles: extracted.tiles.len(),
        encodable_tiles: matched_tiles,
        unique_pattern_count: unique_patterns.len(),
        searched_pattern_count,
        exact_hits: hits,
        clusters,
        matched_tile_indices,
        indexed_source: sheet.indexed.is_some(),
    }
}

fn extract_sheet_tiles(sheet: &SheetImage) -> ExtractedSheetTiles {
    let tile_columns = sheet.width / TILE_SIZE;
    let tile_rows = sheet.height / TILE_SIZE;
    let mut tiles = Vec::new();

    for tile_y in 0..tile_rows {
        for tile_x in 0..tile_columns {
            let tile_index = tile_y * tile_columns + tile_x;
            let rgba_tile = extract_rgba_tile(sheet, tile_x, tile_y);
            let encoded = match &sheet.indexed {
                Some(indices) => encode_indexed_tile(sheet, indices, tile_x, tile_y),
                None => encode_rgba_tile_fallback(&rgba_tile),
            };
            let palette_bank = encoded.as_ref().and_then(|value| value.palette_bank);
            let encoded_4bpp = encoded.as_ref().map(|value| value.bytes);

            tiles.push(ExtractedTile {
                tile_index,
                encoded_4bpp,
                palette_bank,
            });
        }
    }

    ExtractedSheetTiles {
        tile_columns,
        tile_rows,
        tiles,
    }
}

fn extract_rgba_tile(sheet: &SheetImage, tile_x: usize, tile_y: usize) -> Vec<u8> {
    let mut tile = Vec::with_capacity(TILE_SIZE * TILE_SIZE * 4);
    for y in 0..TILE_SIZE {
        let source_y = tile_y * TILE_SIZE + y;
        for x in 0..TILE_SIZE {
            let source_x = tile_x * TILE_SIZE + x;
            let index = (source_y * sheet.width + source_x) * 4;
            tile.extend_from_slice(&sheet.rgba[index..index + 4]);
        }
    }
    tile
}

fn encode_indexed_tile(
    sheet: &SheetImage,
    indices: &[u8],
    tile_x: usize,
    tile_y: usize,
) -> Option<EncodedTile> {
    let mut tile_indices = [0u8; TILE_SIZE * TILE_SIZE];
    let mut palette_bank = None::<u8>;
    let mut has_visible = false;

    for y in 0..TILE_SIZE {
        let source_y = tile_y * TILE_SIZE + y;
        for x in 0..TILE_SIZE {
            let source_x = tile_x * TILE_SIZE + x;
            let src = indices[source_y * sheet.width + source_x];
            let rgba_index = (source_y * sheet.width + source_x) * 4;
            let alpha = sheet.rgba[rgba_index + 3];
            let dst = y * TILE_SIZE + x;

            if alpha == 0 {
                tile_indices[dst] = 0;
                continue;
            }

            has_visible = true;
            let bank = src / 16;
            if let Some(existing_bank) = palette_bank {
                if existing_bank != bank {
                    return None;
                }
            } else {
                palette_bank = Some(bank);
            }
            tile_indices[dst] = src % 16;
        }
    }

    if !has_visible {
        return None;
    }

    Some(EncodedTile {
        bytes: encode_4bpp_tile(&tile_indices),
        palette_bank,
    })
}

fn encode_rgba_tile_fallback(rgba_tile: &[u8]) -> Option<EncodedTile> {
    let mut tile_indices = [0u8; TILE_SIZE * TILE_SIZE];
    let mut colors = Vec::<[u8; 4]>::new();
    colors.push([0, 0, 0, 0]);
    let mut has_visible = false;

    for (index, pixel) in rgba_tile.chunks_exact(4).enumerate() {
        let key = [pixel[0], pixel[1], pixel[2], pixel[3]];
        if key[3] == 0 {
            tile_indices[index] = 0;
            continue;
        }

        has_visible = true;
        let palette_index = if let Some(existing) = colors.iter().position(|color| *color == key) {
            existing as u8
        } else {
            if colors.len() >= 16 {
                return None;
            }
            colors.push(key);
            (colors.len() - 1) as u8
        };
        tile_indices[index] = palette_index;
    }

    if !has_visible {
        return None;
    }

    Some(EncodedTile {
        bytes: encode_4bpp_tile(&tile_indices),
        palette_bank: None,
    })
}

fn encode_4bpp_tile(indices: &[u8; 64]) -> [u8; 32] {
    let mut encoded = [0u8; 32];
    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let value = indices[y * TILE_SIZE + x];
            let bit = 7 - x;
            let row = y * 2;
            encoded[row] |= (value & 1) << bit;
            encoded[row + 1] |= ((value >> 1) & 1) << bit;
            encoded[16 + row] |= ((value >> 2) & 1) << bit;
            encoded[16 + row + 1] |= ((value >> 3) & 1) << bit;
        }
    }
    encoded
}

fn generate_tile_variants(tile: &[u8; 32]) -> Vec<(MatchVariant, [u8; 32])> {
    let mut pixels = [[0u8; TILE_SIZE]; TILE_SIZE];
    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            pixels[y][x] = decode_4bpp_pixel(tile, x, y);
        }
    }

    let variants = [
        (MatchVariant::Normal, pixels),
        (MatchVariant::HFlip, flip_tile_pixels(&pixels, true, false)),
        (MatchVariant::VFlip, flip_tile_pixels(&pixels, false, true)),
        (MatchVariant::HvFlip, flip_tile_pixels(&pixels, true, true)),
    ];

    let mut unique = BTreeMap::<[u8; 32], MatchVariant>::new();
    for (variant, pixels) in variants {
        let mut flat = [0u8; 64];
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                flat[y * TILE_SIZE + x] = pixels[y][x];
            }
        }
        unique.entry(encode_4bpp_tile(&flat)).or_insert(variant);
    }

    unique
        .into_iter()
        .map(|(bytes, variant)| (variant, bytes))
        .collect()
}

fn flip_tile_pixels(
    pixels: &[[u8; TILE_SIZE]; TILE_SIZE],
    hflip: bool,
    vflip: bool,
) -> [[u8; TILE_SIZE]; TILE_SIZE] {
    let mut output = [[0u8; TILE_SIZE]; TILE_SIZE];
    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let sx = if hflip { TILE_SIZE - 1 - x } else { x };
            let sy = if vflip { TILE_SIZE - 1 - y } else { y };
            output[y][x] = pixels[sy][sx];
        }
    }
    output
}

fn cluster_tile_hits(hits: &[TileMatchHit]) -> Vec<TileMatchCluster> {
    let mut by_offset = hits.to_vec();
    by_offset.sort_by_key(|hit| hit.rom_offset);

    let mut clusters = Vec::<TileMatchCluster>::new();
    let mut current = Vec::<TileMatchHit>::new();

    for hit in by_offset {
        let should_split = current
            .last()
            .is_some_and(|last| hit.rom_offset.saturating_sub(last.rom_offset) > 0x800);

        if should_split && !current.is_empty() {
            clusters.push(build_cluster(&current));
            current.clear();
        }
        current.push(hit);
    }

    if !current.is_empty() {
        clusters.push(build_cluster(&current));
    }

    clusters.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.start_offset.cmp(&b.start_offset))
    });
    clusters
}

fn build_cluster(hits: &[TileMatchHit]) -> TileMatchCluster {
    let start_offset = hits.first().map(|hit| hit.rom_offset).unwrap_or(0);
    let end_offset = hits
        .last()
        .map(|hit| hit.rom_offset + 32)
        .unwrap_or(start_offset);
    let mut unique_tiles = HashSet::<usize>::new();
    let mut unique_tile_weight = 0.0f64;
    for hit in hits {
        if unique_tiles.insert(hit.tile_index) {
            unique_tile_weight += 1.0 / (hit.sheet_occurrences as f64).sqrt();
        }
    }
    let palette_banks = hits
        .iter()
        .filter_map(|hit| hit.palette_bank)
        .collect::<HashSet<_>>();
    let weighted_hit_count = hits
        .iter()
        .map(|hit| 1.0 / hit.sheet_occurrences as f64)
        .sum::<f64>();
    let score =
        unique_tile_weight * 14.0 + weighted_hit_count * 6.0 + palette_banks.len() as f64 * 1.5;

    TileMatchCluster {
        start_offset,
        end_offset,
        hit_count: hits.len(),
        weighted_hit_count,
        unique_tile_count: unique_tiles.len(),
        unique_tile_weight,
        palette_banks: palette_banks.into_iter().collect(),
        score,
        sample_hits: hits.iter().take(16).cloned().collect(),
    }
}

fn count_nonzero_tile_pixels(tile: &[u8; 32]) -> usize {
    let mut count = 0usize;
    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            if decode_4bpp_pixel(tile, x, y) != 0 {
                count += 1;
            }
        }
    }
    count
}

fn count_tile_colors(tile: &[u8; 32]) -> usize {
    let mut colors = HashSet::<u8>::new();
    for y in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let color = decode_4bpp_pixel(tile, x, y);
            if color != 0 {
                colors.insert(color);
            }
        }
    }
    colors.len()
}

fn is_discriminative_pattern(pattern: &UniqueTilePattern) -> bool {
    pattern.tile_indices.len() <= 16 && pattern.nonzero_pixels >= 8 && pattern.color_count >= 2
}

fn render_sheet_match_report(
    sheet: &SheetImage,
    analysis: &RomAnalysis,
    matches: &SheetMatchResult,
) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "ROM: {}", analysis.path);
    let _ = writeln!(output, "Sheet: {}", sheet.path);
    let _ = writeln!(output, "Sheet size: {}x{}", sheet.width, sheet.height);
    let _ = writeln!(
        output,
        "Sheet grid: {} x {} tiles",
        matches.tile_columns, matches.tile_rows
    );
    let _ = writeln!(
        output,
        "Indexed PNG source: {}",
        yes_no(matches.indexed_source)
    );
    let _ = writeln!(output, "Total tiles: {}", matches.total_tiles);
    let _ = writeln!(output, "Encodable 4bpp tiles: {}", matches.encodable_tiles);
    let _ = writeln!(
        output,
        "Unique encodable tile patterns: {}",
        matches.unique_pattern_count
    );
    let _ = writeln!(
        output,
        "Discriminative patterns searched: {}",
        matches.searched_pattern_count
    );
    let _ = writeln!(output, "Exact raw 4bpp hits: {}", matches.exact_hits.len());
    let _ = writeln!(
        output,
        "Matched sheet tiles: {}",
        matches.matched_tile_indices.len()
    );
    let _ = writeln!(output, "Clusters: {}", matches.clusters.len());
    if let Some(header) = &analysis.header {
        let _ = writeln!(output, "Header mapping: {}", header.mapping.name());
        let _ = writeln!(output, "Header board: {}", header.board);
        let _ = writeln!(
            output,
            "Compressed graphics likely: {}",
            yes_no(header.features.compressed_graphics_likely)
        );
    }
    let _ = writeln!(output);
    let _ = writeln!(output, "Top clusters");

    if matches.clusters.is_empty() {
        let _ = writeln!(output, "  none");
    } else {
        for cluster in matches.clusters.iter().take(12) {
            let _ = writeln!(
                output,
                "  0x{:06X}-0x{:06X} score {:.2} raw_hits {} weighted_hits {:.2} unique_tiles {} rarity_weight {:.2} palette_banks {:?}",
                cluster.start_offset,
                cluster.end_offset,
                cluster.score,
                cluster.hit_count,
                cluster.weighted_hit_count,
                cluster.unique_tile_count,
                cluster.unique_tile_weight,
                cluster.palette_banks
            );
            for hit in cluster.sample_hits.iter().take(6) {
                let _ = writeln!(
                    output,
                    "    tile {:05} -> 0x{:06X} ({}) variant {} palette_bank {} sheet_occurrences {}",
                    hit.tile_index,
                    hit.rom_offset,
                    hit.cpu_address,
                    hit.variant.name(),
                    hit.palette_bank
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    hit.sheet_occurrences
                );
            }
        }
    }

    let _ = writeln!(output);
    let _ = writeln!(output, "Interpretation");
    if matches.exact_hits.is_empty() {
        let _ = writeln!(
            output,
            "  No exact raw 4bpp matches found. This points toward compression, tile reordering, or palette/index mismatch in the source sheet."
        );
    } else {
        let _ = writeln!(
            output,
            "  Exact raw 4bpp matches exist. The top clusters are likely source regions for at least part of the supplied character sheet."
        );
    }

    output
}

fn render_sheet_match_mask(sheet: &SheetImage, matches: &SheetMatchResult) -> Image {
    let mut image = Image {
        width: sheet.width,
        height: sheet.height,
        pixels: sheet.rgba.clone(),
    };

    for tile_y in 0..matches.tile_rows {
        for tile_x in 0..matches.tile_columns {
            let tile_index = tile_y * matches.tile_columns + tile_x;
            let matched = matches.matched_tile_indices.contains(&tile_index);
            for y in 0..TILE_SIZE {
                let py = tile_y * TILE_SIZE + y;
                for x in 0..TILE_SIZE {
                    let px = tile_x * TILE_SIZE + x;
                    let index = (py * image.width + px) * 4;
                    let [r, g, b, a] = [
                        image.pixels[index],
                        image.pixels[index + 1],
                        image.pixels[index + 2],
                        image.pixels[index + 3],
                    ];
                    if a == 0 {
                        continue;
                    }

                    if matched {
                        image.pixels[index] = r.saturating_add(24);
                        image.pixels[index + 1] = g.saturating_add(80);
                        image.pixels[index + 2] = b.saturating_sub(16);
                    } else {
                        image.pixels[index] = (r as u16 * 45 / 100) as u8;
                        image.pixels[index + 1] = (g as u16 * 45 / 100) as u8;
                        image.pixels[index + 2] = (b as u16 * 45 / 100) as u8;
                    }
                }
            }
        }
    }

    image
}

fn to_io_error(error: png::DecodingError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn detect_header(bytes: &[u8]) -> Option<HeaderCandidate> {
    let mut candidates = HEADER_CANDIDATES
        .iter()
        .copied()
        .filter(|offset| offset + 0x50 <= bytes.len())
        .map(|offset| parse_header_candidate(bytes, offset))
        .collect::<Vec<_>>();

    for candidate in &mut candidates {
        if matches!(
            candidate.mapping,
            CartMapping::ExLoRom | CartMapping::ExHiRom
        ) {
            candidate.score += 4.0;
        }
    }

    let best = candidates
        .into_iter()
        .max_by(|left, right| left.score.total_cmp(&right.score));

    best.filter(|candidate| candidate.score >= 8.0)
}

fn parse_header_candidate(bytes: &[u8], offset: usize) -> HeaderCandidate {
    let title_slice = &bytes[offset + 0x10..offset + 0x25];
    let title = sanitize_ascii(title_slice);
    let map_mode = bytes[offset + 0x25];
    let rom_type = bytes[offset + 0x26];
    let rom_size = bytes[offset + 0x27];
    let sram_size = bytes[offset + 0x28];
    let region = bytes[offset + 0x29];
    let developer_id = bytes[offset + 0x2A];
    let version = bytes[offset + 0x2B];
    let checksum_complement = read_u16_le(bytes, offset + 0x2C);
    let checksum = read_u16_le(bytes, offset + 0x2E);
    let native_reset_vector = read_u16_le(bytes, offset + 0x3C);
    let emulation_reset_vector = read_u16_le(bytes, offset + 0x4C);

    let mut score = score_header(bytes, offset);
    if !is_reasonable_map_mode(map_mode) {
        score -= 4.0;
    }
    let obvious_garbage_fields = [map_mode, rom_type, rom_size, sram_size]
        .iter()
        .filter(|value| matches!(**value, 0x00 | 0xFF))
        .count();
    if obvious_garbage_fields >= 3 {
        score -= 2.5;
    }
    score = score.max(0.0);

    let mapping = infer_mapping(offset, map_mode);
    let board = infer_board(bytes, offset, mapping, &title);
    let features = infer_board_features(bytes, offset, mapping, &board);

    HeaderCandidate {
        offset,
        title,
        map_mode,
        rom_type,
        rom_size,
        sram_size,
        region,
        developer_id,
        version,
        checksum_complement,
        checksum,
        native_reset_vector,
        emulation_reset_vector,
        mapping,
        board,
        features,
        score,
    }
}

fn score_header(bytes: &[u8], offset: usize) -> f64 {
    if bytes.len() < offset + 0x50 {
        return 0.0;
    }

    let map_mode = bytes[offset + 0x25] & !0x10;
    let checksum_complement = read_u16_le(bytes, offset + 0x2C);
    let checksum = read_u16_le(bytes, offset + 0x2E);
    let reset_vector = read_u16_le(bytes, offset + 0x4C);

    if reset_vector < 0x8000 {
        return 0.0;
    }

    let opcode_offset = (offset & !0x7FFF) | (reset_vector as usize & 0x7FFF);
    if opcode_offset >= bytes.len() {
        return 0.0;
    }

    let opcode = bytes[opcode_offset];
    let mut score = 0i32;

    if matches!(opcode, 0x78 | 0x18 | 0x38 | 0x9C | 0x4C | 0x5C) {
        score += 8;
    }
    if matches!(
        opcode,
        0xC2 | 0xE2 | 0xAD | 0xAE | 0xAC | 0xAF | 0xA9 | 0xA2 | 0xA0 | 0x20 | 0x22
    ) {
        score += 4;
    }
    if matches!(opcode, 0x40 | 0x60 | 0x6B | 0xCD | 0xEC | 0xCC) {
        score -= 4;
    }
    if matches!(opcode, 0x00 | 0x02 | 0xDB | 0x42 | 0xFF) {
        score -= 8;
    }

    if checksum.wrapping_add(checksum_complement) == 0xFFFF {
        score += 4;
    }
    if offset == 0x7FB0 && map_mode == 0x20 {
        score += 2;
    }
    if offset == 0xFFB0 && map_mode == 0x21 {
        score += 2;
    }

    score.max(0) as f64
}

fn infer_mapping(offset: usize, map_mode: u8) -> CartMapping {
    match map_mode {
        0x20 => CartMapping::LoRom,
        0x21 => CartMapping::HiRom,
        0x23 => CartMapping::Sa1,
        0x30 => CartMapping::FastLoRom,
        0x31 => CartMapping::FastHiRom,
        0x32 => CartMapping::Sdd1,
        0x35 => CartMapping::ExHiRom,
        0x2A | 0x3A => CartMapping::ExLoRom,
        _ => match offset {
            0x7FB0 | 0x40_7FB0 => CartMapping::LoRom,
            0xFFB0 | 0x40_FFB0 => CartMapping::HiRom,
            _ => CartMapping::Unknown,
        },
    }
}

fn is_reasonable_map_mode(map_mode: u8) -> bool {
    matches!(
        map_mode,
        0x20 | 0x21 | 0x22 | 0x23 | 0x25 | 0x2A | 0x30 | 0x31 | 0x32 | 0x33 | 0x35 | 0x3A
    )
}

fn infer_board(bytes: &[u8], offset: usize, mapping: CartMapping, title: &str) -> String {
    let map_mode = bytes[offset + 0x25];
    let cartridge_type_lo = bytes[offset + 0x26] & 0x0F;
    let cartridge_type_hi = bytes[offset + 0x26] >> 4;
    let cartridge_sub_type = bytes[offset + 0x0F];
    let serial = infer_serial(bytes, offset);

    let mut mode = match map_mode {
        0x20 | 0x30 => "LOROM-".to_string(),
        0x21 | 0x31 => "HIROM-".to_string(),
        0x22 | 0x32 => "SDD1-".to_string(),
        0x23 | 0x33 => "SA1-".to_string(),
        0x25 | 0x35 => "EXHIROM-".to_string(),
        0x2A | 0x3A => "SPC7110-".to_string(),
        _ => match offset {
            0x7FB0 => "LOROM-".to_string(),
            0xFFB0 => "HIROM-".to_string(),
            0x407FB0 => "EXLOROM-".to_string(),
            0x40FFB0 => "EXHIROM-".to_string(),
            _ => String::new(),
        },
    };

    if title == "YUYU NO QUIZ DE GO!GO" {
        mode = "LOROM-".to_string();
    }
    if mode == "LOROM-" && offset == 0x407FB0 {
        mode = "EXLOROM-".to_string();
    }

    let mut board = String::new();
    let mut epson_rtc = false;
    let mut sharp_rtc = false;

    if serial == "ZBSJ" {
        board.push_str("BS-MCC-");
    } else if serial == "042J" {
        board.push_str("GB-");
        board.push_str(&mode);
    } else if serial.starts_with('Z') && serial.ends_with('J') && serial.len() == 4 {
        board.push_str("BS-");
        board.push_str(&mode);
    } else if cartridge_type_lo >= 0x3 {
        match cartridge_type_hi {
            0x0 => {
                board.push_str("NEC-");
                board.push_str(&mode);
            }
            0x1 => board.push_str("GSU-"),
            0x2 => board.push_str("OBC1-"),
            0x3 => board.push_str("SA1-"),
            0x4 => board.push_str("SDD1-"),
            0x5 => {
                board.push_str(&mode);
                sharp_rtc = true;
            }
            0xE if cartridge_type_lo == 0x3 => {
                board.push_str("GB-");
                board.push_str(&mode);
            }
            0xF if cartridge_type_lo == 0x5 && cartridge_sub_type == 0x00 => {
                board.push_str("SPC7110-")
            }
            0xF if cartridge_type_lo == 0x9 && cartridge_sub_type == 0x00 => {
                board.push_str("SPC7110-");
                epson_rtc = true;
            }
            0xF if cartridge_sub_type == 0x01 => {
                board.push_str("EXNEC-");
                board.push_str(&mode);
            }
            0xF if cartridge_sub_type == 0x02 => {
                board.push_str("ARM-");
                board.push_str(&mode);
            }
            0xF if cartridge_sub_type == 0x10 => {
                board.push_str("HITACHI-");
                board.push_str(&mode);
            }
            _ => {}
        }
    }

    if board.is_empty() {
        board.push_str(&mode);
    }

    if infer_ram_size(bytes, offset) > 0 || infer_expansion_ram_size(bytes, offset) > 0 {
        board.push_str("RAM-");
    }
    if epson_rtc {
        board.push_str("EPSONRTC-");
    }
    if sharp_rtc {
        board.push_str("SHARPRTC-");
    }

    while board.ends_with('-') {
        board.pop();
    }

    if board.starts_with("LOROM-RAM") && infer_rom_size(bytes, offset) <= 0x20_0000 {
        board.push_str("#A");
    }
    if board.starts_with("NEC-LOROM-RAM") && infer_rom_size(bytes, offset) <= 0x10_0000 {
        board.push_str("#A");
    }

    if board.starts_with("SPC7110-") && bytes.len() == 0x70_0000 {
        board = format!("EX{board}");
    }

    if board.is_empty() {
        mapping.name().to_string()
    } else {
        board
    }
}

fn infer_board_features(
    bytes: &[u8],
    offset: usize,
    mapping: CartMapping,
    board: &str,
) -> BoardFeatures {
    let board_upper = board.to_ascii_uppercase();
    let map_mode = bytes[offset + 0x25];

    let mut features = BoardFeatures {
        map_family: match mapping {
            CartMapping::LoRom | CartMapping::FastLoRom => "LoROM",
            CartMapping::HiRom | CartMapping::FastHiRom => "HiROM",
            CartMapping::ExLoRom => "ExLoROM",
            CartMapping::ExHiRom => "ExHiROM",
            CartMapping::Sa1 => "SA1",
            CartMapping::Sdd1 => "SDD1",
            CartMapping::Unknown => "Unknown",
        }
        .to_string(),
        board_family: board.to_string(),
        fast_rom: matches!(map_mode, 0x30 | 0x31 | 0x32 | 0x33 | 0x35 | 0x3A),
        ex_rom: matches!(mapping, CartMapping::ExLoRom | CartMapping::ExHiRom),
        has_ram: infer_ram_size(bytes, offset) > 0,
        has_expansion_ram: infer_expansion_ram_size(bytes, offset) > 0,
        save_ram_bytes: infer_ram_size(bytes, offset),
        expansion_ram_bytes: infer_expansion_ram_size(bytes, offset),
        firmware_rom_bytes: infer_firmware_rom_size(bytes, offset),
        program_rom_bytes: infer_program_rom_size(bytes, offset, board),
        data_rom_bytes: infer_data_rom_size(bytes, offset, board),
        expansion_rom_bytes: infer_expansion_rom_size(bytes, offset, board),
        sa1: board_upper.contains("SA1"),
        sdd1: board_upper.contains("SDD1"),
        spc7110: board_upper.contains("SPC7110"),
        superfx: board_upper.contains("GSU"),
        obc1: board_upper.contains("OBC1"),
        hitachi: board_upper.contains("HITACHI"),
        nec_dsp: board_upper.contains("NEC"),
        arm_dsp: board_upper.contains("ARM"),
        gameboy_slot: board_upper.contains("GB-"),
        bs_memory: board_upper.contains("BS-MCC") || board_upper.starts_with("BS-"),
        epson_rtc: board_upper.contains("EPSONRTC"),
        sharp_rtc: board_upper.contains("SHARPRTC"),
        special_chip: false,
        compressed_graphics_likely: false,
    };

    features.special_chip = features.sa1
        || features.sdd1
        || features.spc7110
        || features.superfx
        || features.obc1
        || features.hitachi
        || features.nec_dsp
        || features.arm_dsp
        || features.gameboy_slot
        || features.bs_memory
        || features.epson_rtc
        || features.sharp_rtc;

    features.compressed_graphics_likely =
        features.sdd1 || features.spc7110 || features.sa1 || features.superfx;

    features
}

fn scan_palettes(bytes: &[u8]) -> Vec<AssetCandidate> {
    let mut results = Vec::new();

    for offset in (0..bytes.len().saturating_sub(PALETTE_WINDOW_BYTES)).step_by(16) {
        let window = &bytes[offset..offset + PALETTE_WINDOW_BYTES];
        let mut colors = [0u16; PALETTE_WINDOW_BYTES / 2];
        let mut valid_colors = 0usize;
        let mut non_zero = 0usize;
        let mut rgb_span_sum = 0.0;

        for (index, chunk) in window.chunks_exact(2).enumerate() {
            let color = u16::from_le_bytes([chunk[0], chunk[1]]);
            colors[index] = color;

            if (color & 0x8000) == 0 {
                valid_colors += 1;
            }
            if color != 0 {
                non_zero += 1;
            }

            let r = color & 0x1F;
            let g = (color >> 5) & 0x1F;
            let b = (color >> 10) & 0x1F;
            rgb_span_sum += (r + g + b) as f64;
        }

        if valid_colors < colors.len() {
            continue;
        }

        let distinct = distinct_count_u16(&colors);

        if distinct < 6 || non_zero < 6 {
            continue;
        }

        let rgb_span_score = rgb_span_sum / colors.len() as f64;
        let score = distinct as f64 * 1.5 + non_zero as f64 * 0.25 + rgb_span_score * 0.05;

        results.push(AssetCandidate {
            offset,
            length: PALETTE_WINDOW_BYTES,
            score,
            kind: "palette",
            notes: format!(
                "{distinct} distinct colors, {non_zero} non-zero colors, avg RGB sum {:.1}",
                rgb_span_score
            ),
        });
    }

    top_candidates(results)
}

fn scan_tile_graphics(bytes: &[u8]) -> Vec<AssetCandidate> {
    let mut results = Vec::new();

    for offset in (0..bytes.len().saturating_sub(TILE_GRAPHICS_WINDOW_BYTES)).step_by(32) {
        let window = &bytes[offset..offset + TILE_GRAPHICS_WINDOW_BYTES];

        let non_zero = window.iter().filter(|byte| **byte != 0).count();
        if non_zero < TILE_GRAPHICS_WINDOW_BYTES / 4 {
            continue;
        }

        let entropy = shannon_entropy(window);
        if !(3.0..=7.4).contains(&entropy) {
            continue;
        }

        let repeated_32_byte_chunks = count_repeated_chunks(window, 32);
        let repeated_16_byte_chunks = count_repeated_chunks(window, 16);

        let score = entropy * 1.2
            + non_zero as f64 / TILE_GRAPHICS_WINDOW_BYTES as f64 * 4.0
            + repeated_32_byte_chunks as f64 * 0.8
            + repeated_16_byte_chunks as f64 * 0.4;

        results.push(AssetCandidate {
            offset,
            length: TILE_GRAPHICS_WINDOW_BYTES,
            score,
            kind: "tile-graphics",
            notes: format!(
                "entropy {:.2}, {} repeated 32-byte chunks, {} repeated 16-byte chunks",
                entropy, repeated_32_byte_chunks, repeated_16_byte_chunks
            ),
        });
    }

    top_candidates(results)
}

fn scan_tilemaps(bytes: &[u8]) -> Vec<AssetCandidate> {
    let mut results = Vec::new();

    for offset in (0..bytes.len().saturating_sub(TILEMAP_WINDOW_BYTES)).step_by(32) {
        let mut repeated_words = 0usize;
        let mut tile_indices_below_1024 = 0usize;
        let mut palette_seen = [false; 8];
        let mut distinct_palettes = 0usize;
        let mut word_count = 0usize;
        let mut previous_word = None;
        let mut current_run = 0usize;
        let mut max_run = 0usize;

        for chunk in bytes[offset..offset + TILEMAP_WINDOW_BYTES].chunks_exact(2) {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);
            word_count += 1;

            if previous_word == Some(word) {
                repeated_words += 1;
                current_run += 1;
            } else {
                max_run = max_run.max(current_run);
                current_run = 1;
            }
            previous_word = Some(word);

            let palette_index = ((word >> 10) & 0x7) as usize;
            if !palette_seen[palette_index] {
                palette_seen[palette_index] = true;
                distinct_palettes += 1;
            }

            if (word & 0x03FF) < 1024 {
                tile_indices_below_1024 += 1;
            }
        }
        max_run = max_run.max(current_run);

        if repeated_words < word_count / 12 || tile_indices_below_1024 < word_count / 3 {
            continue;
        }

        if max_run > word_count * 3 / 4 {
            continue;
        }

        let score = repeated_words as f64 * 0.6
            + distinct_palettes as f64 * 1.5
            + tile_indices_below_1024 as f64 / word_count as f64 * 4.0;

        results.push(AssetCandidate {
            offset,
            length: TILEMAP_WINDOW_BYTES,
            score,
            kind: "tilemap",
            notes: format!(
                "{repeated_words} adjacent repeats, {distinct_palettes} palette values, {} low tile indices",
                tile_indices_below_1024
            ),
        });
    }

    top_candidates(results)
}

fn scan_entropy_regions(bytes: &[u8]) -> Vec<AssetCandidate> {
    let mut results = Vec::new();

    for offset in (0..bytes.len().saturating_sub(ENTROPY_WINDOW_BYTES)).step_by(256) {
        let window = &bytes[offset..offset + ENTROPY_WINDOW_BYTES];
        let entropy = shannon_entropy(window);
        if entropy < 7.3 {
            continue;
        }

        let non_zero = window.iter().filter(|byte| **byte != 0).count();
        let score = entropy * 2.0 + non_zero as f64 / window.len() as f64 * 3.0;

        results.push(AssetCandidate {
            offset,
            length: ENTROPY_WINDOW_BYTES,
            score,
            kind: "compressed-or-packed",
            notes: format!(
                "entropy {:.2}, {:.1}% non-zero bytes",
                entropy,
                (non_zero as f64 / window.len() as f64) * 100.0
            ),
        });
    }

    top_candidates(results)
}

fn top_candidates(mut results: Vec<AssetCandidate>) -> Vec<AssetCandidate> {
    results.sort_by(|left, right| right.score.total_cmp(&left.score));

    let mut filtered = Vec::new();
    for candidate in results {
        let overlaps_existing = filtered.iter().any(|existing: &AssetCandidate| {
            let start = existing.offset;
            let end = existing.offset + existing.length;
            let candidate_start = candidate.offset;
            let candidate_end = candidate.offset + candidate.length;
            candidate_start < end && start < candidate_end
        });

        if !overlaps_existing {
            filtered.push(candidate);
        }

        if filtered.len() == MAX_RESULTS_PER_CATEGORY {
            break;
        }
    }

    filtered
}

fn render_report(analysis: &RomAnalysis) -> String {
    let mut output = String::new();
    let mapping = analysis_mapping(analysis);

    let _ = writeln!(output, "ROM: {}", analysis.path);
    let _ = writeln!(
        output,
        "Size: {} bytes (0x{:X})",
        analysis.file_size, analysis.file_size
    );
    let _ = writeln!(
        output,
        "Normalized Size: {} bytes (0x{:X})",
        analysis.normalized_size, analysis.normalized_size
    );
    let _ = writeln!(
        output,
        "Copier Header Removed: {}",
        if analysis.had_copier_header {
            "yes"
        } else {
            "no"
        }
    );
    let _ = writeln!(output);

    match &analysis.header {
        Some(header) => {
            let _ = writeln!(output, "Header");
            let _ = writeln!(output, "  offset: 0x{:X}", header.offset);
            let _ = writeln!(
                output,
                "  cpu_address: {}",
                format_cpu_address_for_offset(header.offset, mapping)
            );
            let _ = writeln!(output, "  score: {:.2}", header.score);
            let _ = writeln!(output, "  title: {}", header.title);
            let _ = writeln!(output, "  mapping: {}", header.mapping.name());
            let _ = writeln!(output, "  board: {}", header.board);
            let _ = writeln!(output, "  board_family: {}", header.features.board_family);
            let _ = writeln!(output, "  map_family: {}", header.features.map_family);
            let _ = writeln!(output, "  map_mode: 0x{:02X}", header.map_mode);
            let _ = writeln!(output, "  rom_type: 0x{:02X}", header.rom_type);
            let _ = writeln!(output, "  rom_size: 0x{:02X}", header.rom_size);
            let _ = writeln!(output, "  sram_size: 0x{:02X}", header.sram_size);
            let _ = writeln!(output, "  region: 0x{:02X}", header.region);
            let _ = writeln!(output, "  developer_id: 0x{:02X}", header.developer_id);
            let _ = writeln!(output, "  version: 0x{:02X}", header.version);
            let _ = writeln!(
                output,
                "  checksum_complement: 0x{:04X}",
                header.checksum_complement
            );
            let _ = writeln!(output, "  checksum: 0x{:04X}", header.checksum);
            let _ = writeln!(
                output,
                "  native_reset_vector: 0x{:04X}",
                header.native_reset_vector
            );
            let _ = writeln!(
                output,
                "  emulation_reset_vector: 0x{:04X}",
                header.emulation_reset_vector
            );
            let _ = writeln!(
                output,
                "  save_ram_bytes: 0x{:X}",
                header.features.save_ram_bytes
            );
            let _ = writeln!(
                output,
                "  expansion_ram_bytes: 0x{:X}",
                header.features.expansion_ram_bytes
            );
            let _ = writeln!(
                output,
                "  program_rom_bytes: 0x{:X}",
                header.features.program_rom_bytes
            );
            let _ = writeln!(
                output,
                "  data_rom_bytes: 0x{:X}",
                header.features.data_rom_bytes
            );
            let _ = writeln!(
                output,
                "  expansion_rom_bytes: 0x{:X}",
                header.features.expansion_rom_bytes
            );
            let _ = writeln!(
                output,
                "  firmware_rom_bytes: 0x{:X}",
                header.features.firmware_rom_bytes
            );
            let _ = writeln!(output, "  fast_rom: {}", yes_no(header.features.fast_rom));
            let _ = writeln!(output, "  ex_rom: {}", yes_no(header.features.ex_rom));
            let _ = writeln!(output, "  has_ram: {}", yes_no(header.features.has_ram));
            let _ = writeln!(
                output,
                "  has_expansion_ram: {}",
                yes_no(header.features.has_expansion_ram)
            );
            let _ = writeln!(output, "  sa1: {}", yes_no(header.features.sa1));
            let _ = writeln!(output, "  sdd1: {}", yes_no(header.features.sdd1));
            let _ = writeln!(output, "  spc7110: {}", yes_no(header.features.spc7110));
            let _ = writeln!(output, "  superfx: {}", yes_no(header.features.superfx));
            let _ = writeln!(output, "  obc1: {}", yes_no(header.features.obc1));
            let _ = writeln!(output, "  hitachi: {}", yes_no(header.features.hitachi));
            let _ = writeln!(output, "  nec_dsp: {}", yes_no(header.features.nec_dsp));
            let _ = writeln!(output, "  arm_dsp: {}", yes_no(header.features.arm_dsp));
            let _ = writeln!(
                output,
                "  gameboy_slot: {}",
                yes_no(header.features.gameboy_slot)
            );
            let _ = writeln!(output, "  bs_memory: {}", yes_no(header.features.bs_memory));
            let _ = writeln!(output, "  epson_rtc: {}", yes_no(header.features.epson_rtc));
            let _ = writeln!(output, "  sharp_rtc: {}", yes_no(header.features.sharp_rtc));
            let _ = writeln!(
                output,
                "  special_chip: {}",
                yes_no(header.features.special_chip)
            );
            let _ = writeln!(
                output,
                "  compressed_graphics_likely: {}",
                yes_no(header.features.compressed_graphics_likely)
            );
        }
        None => {
            let _ = writeln!(output, "Header");
            let _ = writeln!(output, "  no plausible SNES header candidate found");
        }
    }

    let _ = writeln!(output);
    write_candidates(&mut output, "Likely palettes", &analysis.palettes, mapping);
    let _ = writeln!(output);
    write_candidates(
        &mut output,
        "Likely tile graphics",
        &analysis.tile_graphic_regions,
        mapping,
    );
    let _ = writeln!(output);
    write_candidates(&mut output, "Likely tilemaps", &analysis.tilemaps, mapping);
    let _ = writeln!(output);
    write_candidates(
        &mut output,
        "High-entropy regions",
        &analysis.compressed_regions,
        mapping,
    );
    let _ = writeln!(output);
    let _ = writeln!(output, "Preview outputs");
    let _ = writeln!(output, "  palettes/: 16-color swatch strips");
    let _ = writeln!(output, "  tiles/: raw tile sheets decoded as 4bpp and 2bpp");
    let _ = writeln!(
        output,
        "  tilemaps/: 32x32 tilemaps rendered with top tile/palette combinations"
    );
    let _ = writeln!(
        output,
        "  note: CPU addresses are derived from the detected cartridge mapping when available"
    );

    output
}

fn write_candidates(
    output: &mut String,
    title: &str,
    candidates: &[AssetCandidate],
    mapping: Option<CartMapping>,
) {
    let _ = writeln!(output, "{title}");

    if candidates.is_empty() {
        let _ = writeln!(output, "  none");
        return;
    }

    for candidate in candidates {
        let _ = writeln!(
            output,
            "  0x{:06X} ({}) len 0x{:X} score {:.2} [{}] {}",
            candidate.offset,
            format_cpu_address_for_offset(candidate.offset, mapping),
            candidate.length,
            candidate.score,
            candidate.kind,
            candidate.notes
        );
    }
}

fn format_cpu_address_for_offset(offset: usize, mapping: Option<CartMapping>) -> String {
    mapping
        .and_then(|mapping| mapping.file_offset_to_cpu_address(offset))
        .map(|address| format!("{:02X}:{:04X}", address.bank, address.address))
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_palette(bytes: &[u8], offset: usize) -> io::Result<Vec<Rgba>> {
    if offset + PALETTE_WINDOW_BYTES > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("palette offset out of bounds: 0x{offset:X}"),
        ));
    }

    Ok(bytes[offset..offset + PALETTE_WINDOW_BYTES]
        .chunks_exact(2)
        .map(|chunk| bgr555_to_rgba(u16::from_le_bytes([chunk[0], chunk[1]])))
        .collect())
}

fn render_palette_strip(palette: &[Rgba], swatch_size: usize, border: usize) -> Image {
    let width = palette.len() * swatch_size + (palette.len() + 1) * border;
    let height = swatch_size + border * 2;
    let mut image = Image::new(width, height, Rgba::rgb(18, 18, 18));

    for (index, color) in palette.iter().enumerate() {
        let x0 = border + index * (swatch_size + border);
        let y0 = border;
        fill_rect(&mut image, x0, y0, swatch_size, swatch_size, *color);
    }

    image
}

fn render_raw_tiles_preview(
    bytes: &[u8],
    offset: usize,
    max_bytes: usize,
    tiles_per_row: usize,
    bit_depth: TileBitDepth,
    palette: &[Rgba],
) -> Image {
    let tile_size = bit_depth.bytes_per_tile();
    let available = bytes.len().saturating_sub(offset).min(max_bytes);
    let tile_count = available / tile_size;
    let tile_count = tile_count.max(1);
    let rows = tile_count.div_ceil(tiles_per_row);
    let mut image = Image::new(tiles_per_row * 8, rows * 8, Rgba::rgb(0, 0, 0));

    for tile_index in 0..tile_count {
        let tile_offset = offset + tile_index * tile_size;
        if tile_offset + tile_size > bytes.len() {
            break;
        }

        let tile = &bytes[tile_offset..tile_offset + tile_size];
        let tile_x = (tile_index % tiles_per_row) * 8;
        let tile_y = (tile_index / tiles_per_row) * 8;
        draw_tile(
            &mut image, tile_x, tile_y, tile, bit_depth, palette, false, false,
        );
    }

    image
}

fn render_tilemap_preview(
    bytes: &[u8],
    tilemap_offset: usize,
    tile_data_offset: usize,
    bit_depth: TileBitDepth,
    palette: &[Rgba],
) -> io::Result<Image> {
    if tilemap_offset + TILEMAP_WINDOW_BYTES > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("tilemap offset out of bounds: 0x{tilemap_offset:X}"),
        ));
    }

    let entries = bytes[tilemap_offset..tilemap_offset + TILEMAP_WINDOW_BYTES]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();

    let width_tiles = 32;
    let height_tiles = entries.len() / width_tiles;
    let mut image = Image::new(width_tiles * 8, height_tiles * 8, Rgba::rgb(0, 0, 0));
    let tile_size = bit_depth.bytes_per_tile();

    for (index, entry) in entries.iter().enumerate() {
        let tile_number = (entry & 0x03FF) as usize;
        let hflip = (entry & 0x4000) != 0;
        let vflip = (entry & 0x8000) != 0;
        let palette_bank = ((entry >> 10) & 0x7) as usize;
        let effective_palette = palette_shifted(palette, palette_bank);
        let data_offset = tile_data_offset + tile_number * tile_size;

        if data_offset + tile_size > bytes.len() {
            continue;
        }

        let tile = &bytes[data_offset..data_offset + tile_size];
        let tile_x = (index % width_tiles) * 8;
        let tile_y = (index / width_tiles) * 8;
        draw_tile(
            &mut image,
            tile_x,
            tile_y,
            tile,
            bit_depth,
            &effective_palette,
            hflip,
            vflip,
        );
    }

    Ok(image)
}

fn draw_tile(
    image: &mut Image,
    dest_x: usize,
    dest_y: usize,
    tile: &[u8],
    bit_depth: TileBitDepth,
    palette: &[Rgba],
    hflip: bool,
    vflip: bool,
) {
    for y in 0..8 {
        for x in 0..8 {
            let index = match bit_depth {
                TileBitDepth::Bpp2 => decode_2bpp_pixel(tile, x, y) as usize,
                TileBitDepth::Bpp4 => decode_4bpp_pixel(tile, x, y) as usize,
            };

            let source_x = if hflip { 7 - x } else { x };
            let source_y = if vflip { 7 - y } else { y };
            let color = palette
                .get(index)
                .copied()
                .unwrap_or_else(|| grayscale(index, bit_depth.max_palette_entries()));

            image.set_pixel(dest_x + source_x, dest_y + source_y, color);
        }
    }
}

fn decode_2bpp_pixel(tile: &[u8], x: usize, y: usize) -> u8 {
    let plane0 = tile[y * 2];
    let plane1 = tile[y * 2 + 1];
    let bit = 7 - x;
    ((plane0 >> bit) & 1) | (((plane1 >> bit) & 1) << 1)
}

fn decode_4bpp_pixel(tile: &[u8], x: usize, y: usize) -> u8 {
    let row = y * 2;
    let bit = 7 - x;
    let low0 = (tile[row] >> bit) & 1;
    let low1 = (tile[row + 1] >> bit) & 1;
    let high0 = (tile[16 + row] >> bit) & 1;
    let high1 = (tile[16 + row + 1] >> bit) & 1;
    low0 | (low1 << 1) | (high0 << 2) | (high1 << 3)
}

fn palette_shifted(base: &[Rgba], palette_bank: usize) -> Vec<Rgba> {
    if base.is_empty() {
        return default_palette();
    }

    let mut shifted = base.to_vec();
    let shift = (palette_bank * 3) as u8;
    for color in &mut shifted {
        color.r = color.r.saturating_add(shift);
        color.g = color.g.saturating_add(shift / 2);
        color.b = color.b.saturating_add(shift);
    }
    shifted
}

fn default_palette() -> Vec<Rgba> {
    (0..16)
        .map(|index| grayscale(index, 16))
        .collect::<Vec<_>>()
}

fn grayscale(index: usize, max_entries: usize) -> Rgba {
    let divisor = max_entries.saturating_sub(1).max(1);
    let value = ((index * 255) / divisor) as u8;
    Rgba::rgb(value, value, value)
}

fn bgr555_to_rgba(color: u16) -> Rgba {
    let r = expand_5bit((color & 0x1F) as u8);
    let g = expand_5bit(((color >> 5) & 0x1F) as u8);
    let b = expand_5bit(((color >> 10) & 0x1F) as u8);
    Rgba::rgb(r, g, b)
}

fn expand_5bit(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn fill_rect(image: &mut Image, x0: usize, y0: usize, width: usize, height: usize, color: Rgba) {
    for y in y0..(y0 + height) {
        for x in x0..(x0 + width) {
            image.set_pixel(x, y, color);
        }
    }
}

fn sanitize_ascii(bytes: &[u8]) -> String {
    let trimmed = match bytes.iter().position(|byte| *byte == 0) {
        Some(end) => &bytes[..end],
        None => bytes,
    };

    trimmed
        .iter()
        .map(|byte| match byte {
            b' '..=b'~' => *byte as char,
            _ => ' ',
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn infer_serial(bytes: &[u8], offset: usize) -> String {
    let a = bytes[offset + 0x02];
    let b = bytes[offset + 0x03];
    let c = bytes[offset + 0x04];
    let d = bytes[offset + 0x05];

    let valid = |n: u8| n.is_ascii_digit() || n.is_ascii_uppercase();
    if bytes[offset + 0x2A] == 0x33 && valid(a) && valid(b) && valid(c) && valid(d) {
        String::from_utf8_lossy(&[a, b, c, d]).to_string()
    } else {
        String::new()
    }
}

fn infer_firmware_rom_size(bytes: &[u8], offset: usize) -> usize {
    let cartridge_type_lo = bytes[offset + 0x26] & 0x0F;
    let cartridge_type_hi = bytes[offset + 0x26] >> 4;
    let cartridge_sub_type = bytes[offset + 0x0F];
    let serial = infer_serial(bytes, offset);

    if serial == "042J" || (cartridge_type_lo == 0x3 && cartridge_type_hi == 0xE) {
        if (bytes.len() & 0x7FFF) == 0x100 {
            return 0x100;
        }
    }
    if cartridge_type_lo >= 0x3 && cartridge_type_hi == 0xF && cartridge_sub_type == 0x10 {
        if (bytes.len() & 0x7FFF) == 0x0C00 {
            return 0x0C00;
        }
    }
    if cartridge_type_lo >= 0x3 && cartridge_type_hi == 0x0 {
        if (bytes.len() & 0x7FFF) == 0x2000 {
            return 0x2000;
        }
    }
    if cartridge_type_lo >= 0x3 && cartridge_type_hi == 0xF && cartridge_sub_type == 0x01 {
        if (bytes.len() & 0xFFFF) == 0xD000 {
            return 0xD000;
        }
    }
    if cartridge_type_lo >= 0x3 && cartridge_type_hi == 0xF && cartridge_sub_type == 0x02 {
        if (bytes.len() & 0x3FFFF) == 0x28000 {
            return 0x28000;
        }
    }

    0
}

fn infer_rom_size(bytes: &[u8], offset: usize) -> usize {
    bytes
        .len()
        .saturating_sub(infer_firmware_rom_size(bytes, offset))
}

fn infer_ram_size(bytes: &[u8], offset: usize) -> usize {
    let mut ram_size = bytes[offset + 0x28] & 0x0F;
    if ram_size > 8 {
        ram_size = 8;
    }
    if ram_size > 0 {
        1024usize << ram_size
    } else {
        0
    }
}

fn infer_expansion_ram_size(bytes: &[u8], offset: usize) -> usize {
    if bytes[offset + 0x2A] == 0x33 {
        let mut ram_size = bytes[offset + 0x0D] & 0x0F;
        if ram_size > 8 {
            ram_size = 8;
        }
        if ram_size > 0 {
            return 1024usize << ram_size;
        }
    }

    if (bytes[offset + 0x26] >> 4) == 1 {
        return 0x8000;
    }

    0
}

fn infer_program_rom_size(bytes: &[u8], offset: usize, board: &str) -> usize {
    if board.starts_with("SPC7110-") || board.starts_with("EXSPC7110-") {
        0x100000
    } else {
        infer_rom_size(bytes, offset)
    }
}

fn infer_data_rom_size(bytes: &[u8], offset: usize, board: &str) -> usize {
    if board.starts_with("SPC7110-") {
        infer_rom_size(bytes, offset).saturating_sub(0x100000)
    } else if board.starts_with("EXSPC7110-") {
        0x500000
    } else {
        0
    }
}

fn infer_expansion_rom_size(_bytes: &[u8], _offset: usize, board: &str) -> usize {
    if board.starts_with("EXSPC7110-") {
        0x100000
    } else {
        0
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn shannon_entropy(bytes: &[u8]) -> f64 {
    let mut counts = [0usize; 256];
    for byte in bytes {
        counts[*byte as usize] += 1;
    }

    let len = bytes.len() as f64;
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = *count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

fn distinct_count_u16(values: &[u16]) -> usize {
    let mut unique = values.to_vec();
    unique.sort_unstable();
    unique.dedup();
    unique.len()
}

fn count_repeated_chunks(bytes: &[u8], chunk_size: usize) -> usize {
    let mut chunks: Vec<&[u8]> = bytes.chunks_exact(chunk_size).collect();
    chunks.sort_unstable_by(|left, right| {
        let ordering = left.len().cmp(&right.len());
        if ordering != Ordering::Equal {
            return ordering;
        }

        left.cmp(right)
    });

    chunks
        .windows(2)
        .filter(|pair| pair[0] == pair[1] && pair[0].iter().any(|byte| *byte != 0))
        .count()
}

fn lorom_file_offset_to_cpu_address(offset: usize) -> Option<CpuAddress> {
    if offset >= 0x40_0000 {
        return None;
    }

    let chunk = offset / 0x8000;
    let bank = if chunk <= 0x7D {
        chunk as u8
    } else if chunk <= 0x7F {
        (chunk as u8).wrapping_add(0x80)
    } else {
        return None;
    };

    Some(CpuAddress {
        bank,
        address: 0x8000 | (offset as u16 & 0x7FFF),
    })
}

fn hirom_file_offset_to_cpu_address(offset: usize) -> Option<CpuAddress> {
    if offset >= 0x40_0000 {
        return None;
    }

    let bank_index = offset / 0x1_0000;
    if bank_index >= 0x40 {
        return None;
    }

    Some(CpuAddress {
        bank: 0xC0 + bank_index as u8,
        address: (offset & 0xFFFF) as u16,
    })
}

fn write_png_rgba(path: &Path, width: usize, height: usize, rgba: &[u8]) -> io::Result<()> {
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "image dimensions overflow"))?;

    if rgba.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "rgba buffer length mismatch: got {}, expected {}",
                rgba.len(),
                expected_len
            ),
        ));
    }

    let stride = width * 4;
    let mut filtered = Vec::with_capacity(height * (stride + 1));
    for row in rgba.chunks_exact(stride) {
        filtered.push(0);
        filtered.extend_from_slice(row);
    }

    let compressed = zlib_store_compress(&filtered);

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_png_chunk(&mut png, b"IHDR", &ihdr);
    push_png_chunk(&mut png, b"IDAT", &compressed);
    push_png_chunk(&mut png, b"IEND", &[]);

    fs::write(path, png)
}

fn zlib_store_compress(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(&[0x78, 0x01]);

    let mut remaining = data;
    while !remaining.is_empty() {
        let block_len = remaining.len().min(65_535);
        let is_final = block_len == remaining.len();
        output.push(if is_final { 0x01 } else { 0x00 });
        output.extend_from_slice(&(block_len as u16).to_le_bytes());
        output.extend_from_slice((!(block_len as u16)).to_le_bytes().as_slice());
        output.extend_from_slice(&remaining[..block_len]);
        remaining = &remaining[block_len..];
    }

    let checksum = adler32(data);
    output.extend_from_slice(&checksum.to_be_bytes());
    output
}

fn push_png_chunk(output: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(chunk_type);
    output.extend_from_slice(data);

    let mut crc_data = Vec::with_capacity(chunk_type.len() + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    output.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;

    for byte in data {
        a = (a + *byte as u32) % MOD;
        b = (b + a) % MOD;
    }

    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }

    !crc
}

#[derive(Debug)]
struct RomAnalysis {
    path: String,
    stem: String,
    file_size: usize,
    normalized_size: usize,
    had_copier_header: bool,
    header: Option<HeaderCandidate>,
    palettes: Vec<AssetCandidate>,
    tile_graphic_regions: Vec<AssetCandidate>,
    tilemaps: Vec<AssetCandidate>,
    compressed_regions: Vec<AssetCandidate>,
}

struct NormalizedRom {
    bytes: Vec<u8>,
    had_copier_header: bool,
    source_size: usize,
}

struct SheetImage {
    path: String,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    indexed: Option<Vec<u8>>,
}

struct ExtractedSheetTiles {
    tile_columns: usize,
    tile_rows: usize,
    tiles: Vec<ExtractedTile>,
}

struct ExtractedTile {
    tile_index: usize,
    encoded_4bpp: Option<[u8; 32]>,
    palette_bank: Option<u8>,
}

struct EncodedTile {
    bytes: [u8; 32],
    palette_bank: Option<u8>,
}

struct SheetMatchResult {
    tile_columns: usize,
    tile_rows: usize,
    total_tiles: usize,
    encodable_tiles: usize,
    unique_pattern_count: usize,
    searched_pattern_count: usize,
    exact_hits: Vec<TileMatchHit>,
    clusters: Vec<TileMatchCluster>,
    matched_tile_indices: HashSet<usize>,
    indexed_source: bool,
}

#[derive(Clone)]
struct TileMatchHit {
    rom_offset: usize,
    cpu_address: String,
    tile_index: usize,
    variant: MatchVariant,
    palette_bank: Option<u8>,
    sheet_occurrences: usize,
}

struct TileMatchCluster {
    start_offset: usize,
    end_offset: usize,
    hit_count: usize,
    weighted_hit_count: f64,
    unique_tile_count: usize,
    unique_tile_weight: f64,
    palette_banks: Vec<u8>,
    score: f64,
    sample_hits: Vec<TileMatchHit>,
}

struct UniqueTilePattern {
    bytes: [u8; 32],
    representative_tile_index: usize,
    tile_indices: Vec<usize>,
    palette_bank: Option<u8>,
    nonzero_pixels: usize,
    color_count: usize,
}

struct TileNeedle {
    pattern_id: usize,
    tile_index: usize,
    variant: MatchVariant,
    palette_bank: Option<u8>,
    sheet_occurrences: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum MatchVariant {
    Normal,
    HFlip,
    VFlip,
    HvFlip,
}

impl MatchVariant {
    fn name(self) -> &'static str {
        match self {
            MatchVariant::Normal => "normal",
            MatchVariant::HFlip => "hflip",
            MatchVariant::VFlip => "vflip",
            MatchVariant::HvFlip => "hvflip",
        }
    }
}

#[derive(Debug)]
struct HeaderCandidate {
    offset: usize,
    title: String,
    map_mode: u8,
    rom_type: u8,
    rom_size: u8,
    sram_size: u8,
    region: u8,
    developer_id: u8,
    version: u8,
    checksum_complement: u16,
    checksum: u16,
    native_reset_vector: u16,
    emulation_reset_vector: u16,
    mapping: CartMapping,
    board: String,
    features: BoardFeatures,
    score: f64,
}

#[derive(Debug)]
struct AssetCandidate {
    offset: usize,
    length: usize,
    score: f64,
    kind: &'static str,
    notes: String,
}

#[derive(Debug)]
struct BoardFeatures {
    map_family: String,
    board_family: String,
    fast_rom: bool,
    ex_rom: bool,
    has_ram: bool,
    has_expansion_ram: bool,
    save_ram_bytes: usize,
    expansion_ram_bytes: usize,
    firmware_rom_bytes: usize,
    program_rom_bytes: usize,
    data_rom_bytes: usize,
    expansion_rom_bytes: usize,
    sa1: bool,
    sdd1: bool,
    spc7110: bool,
    superfx: bool,
    obc1: bool,
    hitachi: bool,
    nec_dsp: bool,
    arm_dsp: bool,
    gameboy_slot: bool,
    bs_memory: bool,
    epson_rtc: bool,
    sharp_rtc: bool,
    special_chip: bool,
    compressed_graphics_likely: bool,
}

#[derive(Clone, Copy, Debug)]
enum CartMapping {
    LoRom,
    FastLoRom,
    HiRom,
    FastHiRom,
    ExLoRom,
    ExHiRom,
    Sa1,
    Sdd1,
    Unknown,
}

impl CartMapping {
    fn name(self) -> &'static str {
        match self {
            CartMapping::LoRom => "LoROM",
            CartMapping::FastLoRom => "LoROM + FastROM",
            CartMapping::HiRom => "HiROM",
            CartMapping::FastHiRom => "HiROM + FastROM",
            CartMapping::ExLoRom => "ExLoROM",
            CartMapping::ExHiRom => "ExHiROM",
            CartMapping::Sa1 => "SA-1",
            CartMapping::Sdd1 => "SDD-1",
            CartMapping::Unknown => "Unknown",
        }
    }

    fn cpu_map_mode(self) -> Option<CartMapping> {
        match self {
            CartMapping::LoRom
            | CartMapping::FastLoRom
            | CartMapping::HiRom
            | CartMapping::FastHiRom => Some(self),
            _ => None,
        }
    }

    fn file_offset_to_cpu_address(self, offset: usize) -> Option<CpuAddress> {
        match self {
            CartMapping::LoRom | CartMapping::FastLoRom => lorom_file_offset_to_cpu_address(offset),
            CartMapping::HiRom | CartMapping::FastHiRom => hirom_file_offset_to_cpu_address(offset),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CpuAddress {
    bank: u8,
    address: u16,
}

#[derive(Clone, Copy)]
enum TileBitDepth {
    Bpp2,
    Bpp4,
}

impl TileBitDepth {
    fn bytes_per_tile(self) -> usize {
        match self {
            TileBitDepth::Bpp2 => 16,
            TileBitDepth::Bpp4 => 32,
        }
    }

    fn max_palette_entries(self) -> usize {
        match self {
            TileBitDepth::Bpp2 => 4,
            TileBitDepth::Bpp4 => 16,
        }
    }
}

#[derive(Clone, Copy)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Rgba {
    fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

struct Image {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Image {
    fn new(width: usize, height: usize, fill: Rgba) -> Self {
        let mut pixels = vec![0; width * height * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = fill.r;
            chunk[1] = fill.g;
            chunk[2] = fill.b;
            chunk[3] = fill.a;
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: Rgba) {
        if x >= self.width || y >= self.height {
            return;
        }

        let index = (y * self.width + x) * 4;
        self.pixels[index] = color.r;
        self.pixels[index + 1] = color.g;
        self.pixels[index + 2] = color.b;
        self.pixels[index + 3] = color.a;
    }
}
