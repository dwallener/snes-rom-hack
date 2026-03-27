use crate::rommap::load_rom;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerGfxOp {
    pub callsite: String,
    pub command_id: u16,
    pub vram_dest: u16,
    pub source_bank: u8,
    pub source_addr: u16,
    pub source_pc: usize,
    pub preview_bytes: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerGfxReport {
    pub ops: Vec<PlayerGfxOp>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerSheetMatch {
    pub callsite: String,
    pub source_bank: u8,
    pub source_addr: u16,
    pub vram_dest: u16,
    pub matched_tiles: usize,
    pub tile_count: usize,
    pub score: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerSheetMatchReport {
    pub sheet: String,
    pub matches: Vec<PlayerSheetMatch>,
}

#[derive(Debug, Clone, Copy)]
struct ParsedInstr<'a> {
    snes: &'a str,
    mnemonic: &'a str,
    immediate: Option<u16>,
}

#[derive(Clone, Copy)]
struct Rgba(u8, u8, u8, u8);

struct Image {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Image {
    fn new(width: usize, height: usize, color: Rgba) -> Self {
        let mut pixels = vec![0u8; width * height * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&[color.0, color.1, color.2, color.3]);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: Rgba) {
        let index = (y * self.width + x) * 4;
        self.pixels[index..index + 4].copy_from_slice(&[color.0, color.1, color.2, color.3]);
    }
}

pub fn extract_player_gfx_ops(disasm_text: &str, preview_bytes: usize) -> PlayerGfxReport {
    let mut parsed = Vec::<ParsedInstr<'_>>::new();
    for line in disasm_text.lines() {
        if let Some(instr) = parse_instruction_line(line) {
            parsed.push(instr);
        }
    }

    let mut ops = Vec::new();
    let mut warnings = Vec::new();
    for window in parsed.windows(4) {
        let [lda, ldx, ldy, jsr] = match window {
            [a, b, c, d] => [a, b, c, d],
            _ => continue,
        };
        if lda.mnemonic != "lda"
            || ldx.mnemonic != "ldx"
            || ldy.mnemonic != "ldy"
            || jsr.mnemonic != "jsr_a39a"
        {
            continue;
        }

        let Some(command_id) = lda.immediate else {
            warnings.push(format!("missing lda immediate near {}", lda.snes));
            continue;
        };
        let Some(vram_dest) = ldx.immediate else {
            warnings.push(format!("missing ldx immediate near {}", ldx.snes));
            continue;
        };
        let Some(y_base) = ldy.immediate else {
            warnings.push(format!("missing ldy immediate near {}", ldy.snes));
            continue;
        };

        let (source_bank, source_addr) = decode_a39a_source(command_id, y_base);
        let Some(source_pc) = lorom_pc(source_bank, source_addr) else {
            warnings.push(format!(
                "could not map decoded source ${source_bank:02X}:{source_addr:04X} from {}",
                jsr.snes
            ));
            continue;
        };

        ops.push(PlayerGfxOp {
            callsite: jsr.snes.to_string(),
            command_id,
            vram_dest,
            source_bank,
            source_addr,
            source_pc,
            preview_bytes,
        });
    }

    PlayerGfxReport { ops, warnings }
}

pub fn run_player_gfx_report_cli(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut disasm_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut preview_bytes = 0x400usize;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--disasm" => {
                index += 1;
                disasm_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--preview-bytes" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing preview byte count")
                })?;
                preview_bytes = parse_usize(raw)?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `player-gfx-report --rom <path> --disasm <path> --out <dir> [--preview-bytes N]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `player-gfx-report`",
        )
    })?;
    let disasm_path = disasm_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--disasm <path>` for `player-gfx-report`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `player-gfx-report`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;
    let loaded = load_rom(&rom_path)?;
    let disasm_text = fs::read_to_string(disasm_path)?;
    let report = extract_player_gfx_ops(&disasm_text, preview_bytes);

    fs::write(out_dir.join("player_gfx_summary.txt"), format_report(&report))?;
    fs::write(
        out_dir.join("player_gfx_report.json"),
        serde_json::to_vec_pretty(&report).map_err(to_io_error)?,
    )?;

    let previews_dir = out_dir.join("previews");
    fs::create_dir_all(&previews_dir)?;
    for (index, op) in report.ops.iter().enumerate() {
        let png = render_4bpp_preview(&loaded.bytes, op.source_pc, op.preview_bytes);
        let path = previews_dir.join(format!(
            "{index:02}_{}_rom_{:02X}_{:04X}_vram_{:04X}.png",
            op.callsite.replace(':', "_").replace('$', ""),
            op.source_bank,
            op.source_addr,
            op.vram_dest
        ));
        write_png_rgba(&path, png.width, png.height, &png.pixels)?;
    }

    println!(
        "generated player graphics report {} -> {}",
        rom_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn run_patch_player_gfx_cli(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut disasm_path = None::<PathBuf>;
    let mut png_path = None::<PathBuf>;
    let mut out_path = None::<PathBuf>;
    let mut callsite = None::<String>;
    let mut preview_bytes = 0x400usize;
    let mut sheet_tile_offset = 0usize;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--disasm" => {
                index += 1;
                disasm_path = args.get(index).map(PathBuf::from);
            }
            "--png" => {
                index += 1;
                png_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_path = args.get(index).map(PathBuf::from);
            }
            "--callsite" => {
                index += 1;
                callsite = args.get(index).cloned();
            }
            "--preview-bytes" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing preview byte count")
                })?;
                preview_bytes = parse_usize(raw)?;
            }
            "--sheet-tile-offset" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing sheet tile offset")
                })?;
                sheet_tile_offset = parse_usize(raw)?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `patch-player-gfx --rom <path> --disasm <path> --callsite <$80:....> --png <path> --out <path> [--preview-bytes N] [--sheet-tile-offset N]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `patch-player-gfx`",
        )
    })?;
    let disasm_path = disasm_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--disasm <path>` for `patch-player-gfx`",
        )
    })?;
    let png_path = png_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--png <path>` for `patch-player-gfx`",
        )
    })?;
    let out_path = out_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <path>` for `patch-player-gfx`",
        )
    })?;
    let callsite = callsite.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--callsite <$80:....>` for `patch-player-gfx`",
        )
    })?;

    let loaded = load_rom(&rom_path)?;
    let disasm_text = fs::read_to_string(disasm_path)?;
    let report = extract_player_gfx_ops(&disasm_text, preview_bytes);
    let op = report
        .ops
        .iter()
        .find(|op| op.callsite.eq_ignore_ascii_case(&callsite))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "callsite not found in player batch"))?;

    let tiles = load_png_as_4bpp_tiles(&png_path)?;
    let tile_count = op.preview_bytes / 32;
    if sheet_tile_offset + tile_count > tiles.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "sheet does not contain enough tiles: need {} starting at {}, only {} available",
                tile_count,
                sheet_tile_offset,
                tiles.len()
            ),
        ));
    }

    let mut patched = loaded.bytes.clone();
    let start = op.source_pc;
    let end = start + op.preview_bytes;
    if end > patched.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "target patch range extends beyond ROM",
        ));
    }

    for (index, tile) in tiles[sheet_tile_offset..sheet_tile_offset + tile_count]
        .iter()
        .enumerate()
    {
        let offset = start + index * 32;
        patched[offset..offset + 32].copy_from_slice(tile);
    }

    fs::write(&out_path, patched)?;
    println!(
        "patched {} at {} (${:#06X}:{:04X} pc=0x{:06X}) using {} -> {}",
        rom_path.display(),
        op.callsite,
        op.source_bank,
        op.source_addr,
        op.source_pc,
        png_path.display(),
        out_path.display()
    );
    Ok(())
}

pub fn run_match_player_gfx_sheet_cli(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut disasm_path = None::<PathBuf>;
    let mut sheet_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut preview_bytes = 0x400usize;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--disasm" => {
                index += 1;
                disasm_path = args.get(index).map(PathBuf::from);
            }
            "--sheet" => {
                index += 1;
                sheet_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--preview-bytes" => {
                index += 1;
                let raw = args.get(index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing preview byte count")
                })?;
                preview_bytes = parse_usize(raw)?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `match-player-gfx-sheet --rom <path> --disasm <path> --sheet <png> --out <dir> [--preview-bytes N]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `match-player-gfx-sheet`",
        )
    })?;
    let disasm_path = disasm_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--disasm <path>` for `match-player-gfx-sheet`",
        )
    })?;
    let sheet_path = sheet_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--sheet <png>` for `match-player-gfx-sheet`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `match-player-gfx-sheet`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;
    let loaded = load_rom(&rom_path)?;
    let disasm_text = fs::read_to_string(disasm_path)?;
    let report = extract_player_gfx_ops(&disasm_text, preview_bytes);
    let sheet_tiles = load_png_as_4bpp_tiles(&sheet_path)?;
    let sheet_signatures = sheet_tiles
        .iter()
        .map(|tile| decode_4bpp_tile(tile))
        .map(|pixels| canonical_signature(&pixels))
        .collect::<Vec<_>>();

    let mut matches = Vec::new();
    for op in &report.ops {
        let tile_count = op.preview_bytes / 32;
        let mut matched = 0usize;
        let mut score = 0usize;
        for index in 0..tile_count {
            let offset = op.source_pc + index * 32;
            if offset + 32 > loaded.bytes.len() {
                break;
            }
            let tile = &loaded.bytes[offset..offset + 32];
            let pixels = decode_4bpp_tile(tile);
            let variants = canonical_variants(&pixels);
            if sheet_signatures
                .iter()
                .any(|sig| variants.iter().any(|candidate| candidate == sig))
            {
                matched += 1;
                score += 1;
            }
        }

        matches.push(PlayerSheetMatch {
            callsite: op.callsite.clone(),
            source_bank: op.source_bank,
            source_addr: op.source_addr,
            vram_dest: op.vram_dest,
            matched_tiles: matched,
            tile_count,
            score,
        });
    }

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.callsite.cmp(&right.callsite))
    });

    let summary = format_player_sheet_match_summary(&sheet_path, &matches);
    fs::write(out_dir.join("player_sheet_match_summary.txt"), summary)?;
    let json = PlayerSheetMatchReport {
        sheet: sheet_path.display().to_string(),
        matches,
    };
    fs::write(
        out_dir.join("player_sheet_match_report.json"),
        serde_json::to_vec_pretty(&json).map_err(to_io_error)?,
    )?;

    println!(
        "matched player sheet {} -> {}",
        sheet_path.display(),
        out_dir.display()
    );
    Ok(())
}

fn parse_instruction_line(line: &str) -> Option<ParsedInstr<'_>> {
    let trimmed = line.trim_start();
    let snes = trimmed.get(0..9)?;
    if !snes.starts_with('$') {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let snes = parts.next()?;
    let _bytes0 = parts.next()?;
    let _bytes1 = parts.next();
    let _bytes2 = parts.next();
    let mnemonic = parts.next()?;
    let rest = parts.collect::<Vec<_>>().join(" ");
    let immediate = rest
        .split("#$")
        .nth(1)
        .and_then(|value| value.get(..4))
        .and_then(|value| u16::from_str_radix(value, 16).ok());

    if mnemonic == "jsr" && rest.contains("$A39A") {
        return Some(ParsedInstr {
            snes,
            mnemonic: "jsr_a39a",
            immediate: None,
        });
    }

    Some(ParsedInstr {
        snes,
        mnemonic,
        immediate,
    })
}

fn decode_a39a_source(command_id: u16, y_base: u16) -> (u8, u16) {
    let source_low = if (command_id & 0x0001) != 0 { 0x80 } else { 0x00 };
    let hi_bank = ((command_id & 0x007c) << 1).wrapping_add(y_base);
    let source_hi = (hi_bank & 0x00ff) as u16;
    let source_bank = ((hi_bank >> 8) & 0x00ff) as u8;
    (source_bank, (source_hi << 8) | source_low)
}

fn lorom_pc(bank: u8, addr: u16) -> Option<usize> {
    if addr < 0x8000 {
        return None;
    }
    Some(bank as usize * 0x8000 + (addr as usize - 0x8000))
}

fn parse_usize(raw: &str) -> io::Result<usize> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        return usize::from_str_radix(hex, 16)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error));
    }
    raw.parse::<usize>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn format_report(report: &PlayerGfxReport) -> String {
    let mut out = String::new();
    out.push_str("; Player Graphics Batch Report\n");
    out.push_str(&format!("; ops={}\n", report.ops.len()));
    if !report.warnings.is_empty() {
        out.push_str("; warnings\n");
        for warning in &report.warnings {
            out.push_str("; ");
            out.push_str(warning);
            out.push('\n');
        }
    }
    out.push('\n');
    for op in &report.ops {
        out.push_str(&format!(
            "{} cmd=${:04X} rom=${:02X}:{:04X} pc=0x{:06X} -> vram=${:04X} preview=0x{:X}\n",
            op.callsite,
            op.command_id,
            op.source_bank,
            op.source_addr,
            op.source_pc,
            op.vram_dest,
            op.preview_bytes
        ));
    }
    out
}

fn render_4bpp_preview(bytes: &[u8], offset: usize, preview_bytes: usize) -> Image {
    let palette = grayscale_palette();
    let available = bytes.len().saturating_sub(offset);
    let length = available.min(preview_bytes);
    let tile_count = (length / 32).max(1);
    let tiles_per_row = 16usize;
    let rows = tile_count.div_ceil(tiles_per_row);
    let mut image = Image::new(tiles_per_row * 8, rows * 8, Rgba(0, 0, 0, 255));

    for tile_index in 0..tile_count {
        let tile_offset = offset + tile_index * 32;
        if tile_offset + 32 > bytes.len() {
            break;
        }
        let tile = &bytes[tile_offset..tile_offset + 32];
        let base_x = (tile_index % tiles_per_row) * 8;
        let base_y = (tile_index / tiles_per_row) * 8;
        draw_4bpp_tile(&mut image, tile, base_x, base_y, &palette);
    }

    image
}

fn grayscale_palette() -> [Rgba; 16] {
    let mut palette = [Rgba(0, 0, 0, 255); 16];
    for (index, slot) in palette.iter_mut().enumerate() {
        let value = (index as u8) * 17;
        *slot = Rgba(value, value, value, 255);
    }
    palette
}

fn draw_4bpp_tile(image: &mut Image, tile: &[u8], base_x: usize, base_y: usize, palette: &[Rgba; 16]) {
    for y in 0..8usize {
        let row = y * 2;
        for x in 0..8usize {
            let bit = 7 - x;
            let low0 = (tile[row] >> bit) & 1;
            let low1 = (tile[row + 1] >> bit) & 1;
            let high0 = (tile[16 + row] >> bit) & 1;
            let high1 = (tile[16 + row + 1] >> bit) & 1;
            let color = (low0 | (low1 << 1) | (high0 << 2) | (high1 << 3)) as usize;
            image.set_pixel(base_x + x, base_y + y, palette[color]);
        }
    }
}

fn write_png_rgba(path: &Path, width: usize, height: usize, rgba: &[u8]) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let writer = io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(to_io_error)?;
    writer.write_image_data(rgba).map_err(to_io_error)
}

fn to_io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn load_png_as_4bpp_tiles(path: &Path) -> io::Result<Vec<[u8; 32]>> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(to_io_error)?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(to_io_error)?;
    let data = &buffer[..info.buffer_size()];

    let width = info.width as usize;
    let height = info.height as usize;
    let width_tiles = width / 8;
    let height_tiles = height / 8;
    let total_tiles = width_tiles * height_tiles;
    let mut tiles = Vec::with_capacity(total_tiles);

    let indexed = match info.color_type {
        png::ColorType::Indexed => data.to_vec(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "patch PNG must be indexed to preserve tile palette indices",
            ))
        }
    };

    for tile_y in 0..height_tiles {
        for tile_x in 0..width_tiles {
            let mut indices = [0u8; 64];
            for y in 0..8usize {
                for x in 0..8usize {
                    let src_x = tile_x * 8 + x;
                    let src_y = tile_y * 8 + y;
                    indices[y * 8 + x] = indexed[src_y * width + src_x] & 0x0F;
                }
            }
            tiles.push(encode_4bpp_tile(&indices));
        }
    }

    Ok(tiles)
}

fn encode_4bpp_tile(indices: &[u8; 64]) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for y in 0..8usize {
        let mut p0 = 0u8;
        let mut p1 = 0u8;
        let mut p2 = 0u8;
        let mut p3 = 0u8;
        for x in 0..8usize {
            let value = indices[y * 8 + x];
            let bit = 7 - x;
            p0 |= (value & 0x01) << bit;
            p1 |= ((value >> 1) & 0x01) << bit;
            p2 |= ((value >> 2) & 0x01) << bit;
            p3 |= ((value >> 3) & 0x01) << bit;
        }
        bytes[y * 2] = p0;
        bytes[y * 2 + 1] = p1;
        bytes[16 + y * 2] = p2;
        bytes[16 + y * 2 + 1] = p3;
    }
    bytes
}

fn decode_4bpp_tile(tile: &[u8]) -> [u8; 64] {
    let mut pixels = [0u8; 64];
    for y in 0..8usize {
        let row = y * 2;
        for x in 0..8usize {
            let bit = 7 - x;
            let low0 = (tile[row] >> bit) & 1;
            let low1 = (tile[row + 1] >> bit) & 1;
            let high0 = (tile[16 + row] >> bit) & 1;
            let high1 = (tile[16 + row + 1] >> bit) & 1;
            pixels[y * 8 + x] = low0 | (low1 << 1) | (high0 << 2) | (high1 << 3);
        }
    }
    pixels
}

fn canonical_signature(pixels: &[u8; 64]) -> [u8; 64] {
    let mut mapping = [0u8; 16];
    let mut next = 1u8;
    let mut out = [0u8; 64];
    for (index, pixel) in pixels.iter().copied().enumerate() {
        if pixel == 0 {
            out[index] = 0;
            continue;
        }
        let slot = &mut mapping[pixel as usize];
        if *slot == 0 {
            *slot = next;
            next += 1;
        }
        out[index] = *slot;
    }
    out
}

fn canonical_variants(pixels: &[u8; 64]) -> [[u8; 64]; 4] {
    let mut out = [[0u8; 64]; 4];
    out[0] = canonical_signature(pixels);
    out[1] = canonical_signature(&flip_pixels(pixels, true, false));
    out[2] = canonical_signature(&flip_pixels(pixels, false, true));
    out[3] = canonical_signature(&flip_pixels(pixels, true, true));
    out
}

fn flip_pixels(pixels: &[u8; 64], flip_x: bool, flip_y: bool) -> [u8; 64] {
    let mut out = [0u8; 64];
    for y in 0..8usize {
        for x in 0..8usize {
            let src_x = if flip_x { 7 - x } else { x };
            let src_y = if flip_y { 7 - y } else { y };
            out[y * 8 + x] = pixels[src_y * 8 + src_x];
        }
    }
    out
}

fn format_player_sheet_match_summary(sheet_path: &Path, matches: &[PlayerSheetMatch]) -> String {
    let mut out = String::new();
    out.push_str("; Player Sheet Match Summary\n");
    out.push_str(&format!("; sheet={}\n\n", sheet_path.display()));
    for entry in matches {
        out.push_str(&format!(
            "{} rom=${:02X}:{:04X} -> vram=${:04X} matched={}/{} score={}\n",
            entry.callsite,
            entry.source_bank,
            entry.source_addr,
            entry.vram_dest,
            entry.matched_tiles,
            entry.tile_count,
            entry.score
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_signature, canonical_variants, decode_a39a_source, encode_4bpp_tile,
        extract_player_gfx_ops,
    };

    #[test]
    fn decodes_known_player_batch_sources() {
        assert_eq!(decode_a39a_source(0x0015, 0x1080), (0x10, 0xA880));
        assert_eq!(decode_a39a_source(0x0026, 0x1080), (0x10, 0xC800));
        assert_eq!(decode_a39a_source(0x0005, 0x1180), (0x11, 0x8880));
    }

    #[test]
    fn extracts_repeated_a39a_callsites() {
        let disasm = "\
$80:BCA9  a9 15 00    lda #$0015\n\
$80:BCAC  a2 00 60    ldx #$6000\n\
$80:BCAF  a0 80 10    ldy #$1080\n\
$80:BCB2  20 9a a3    jsr $A39A ; -> sub_80_A39A\n";

        let report = extract_player_gfx_ops(disasm, 0x400);
        assert!(report.warnings.is_empty());
        assert_eq!(report.ops.len(), 1);
        assert_eq!(report.ops[0].callsite, "$80:BCB2");
        assert_eq!(report.ops[0].source_bank, 0x10);
        assert_eq!(report.ops[0].source_addr, 0xA880);
        assert_eq!(report.ops[0].vram_dest, 0x6000);
    }

    #[test]
    fn encodes_simple_4bpp_tile() {
        let mut indices = [0u8; 64];
        indices[0] = 1;
        indices[1] = 2;
        indices[2] = 4;
        indices[3] = 8;
        let encoded = encode_4bpp_tile(&indices);
        assert_eq!(encoded[0], 0b1000_0000);
        assert_eq!(encoded[1], 0b0100_0000);
        assert_eq!(encoded[16], 0b0010_0000);
        assert_eq!(encoded[17], 0b0001_0000);
    }

    #[test]
    fn canonical_signature_ignores_palette_values() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        a[0] = 3;
        a[1] = 7;
        a[8] = 7;
        b[0] = 5;
        b[1] = 9;
        b[8] = 9;
        assert_eq!(canonical_signature(&a), canonical_signature(&b));
    }

    #[test]
    fn canonical_variants_include_hflip() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        a[0] = 1;
        a[1] = 2;
        b[6] = 2;
        b[7] = 1;
        let variants = canonical_variants(&a);
        let target = canonical_signature(&b);
        assert!(variants.contains(&target));
    }
}
