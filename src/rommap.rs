use crate::mapper::{CpuAddress, lorom_vector_target_to_pc, pc_to_lorom};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;

const HEADER_CANDIDATES: [usize; 4] = [0x7FC0, 0xFFC0, 0x40_7FC0, 0x40_FFC0];

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub enum MappingKind {
    LoRom,
    HiRom,
    ExLoRom,
    ExHiRom,
    Unknown,
}

impl MappingKind {
    pub fn name(&self) -> &'static str {
        match self {
            MappingKind::LoRom => "LoROM",
            MappingKind::HiRom => "HiROM",
            MappingKind::ExLoRom => "ExLoROM",
            MappingKind::ExHiRom => "ExHiROM",
            MappingKind::Unknown => "Unknown",
        }
    }

    pub fn supports_v1_disasm(&self) -> bool {
        matches!(self, MappingKind::LoRom)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
pub struct VectorInfo {
    pub emulation_cop: u16,
    pub emulation_brk: u16,
    pub emulation_abort: u16,
    pub emulation_nmi: u16,
    pub emulation_reset: u16,
    pub emulation_irq: u16,
    pub native_cop: u16,
    pub native_brk: u16,
    pub native_abort: u16,
    pub native_nmi: u16,
    pub native_reset: u16,
    pub native_irq: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct RomInfo {
    pub path: String,
    pub size: usize,
    pub source_size: usize,
    pub has_copier_header: bool,
    pub mapping: MappingKind,
    pub header_offset: usize,
    pub reset_vector: Option<u16>,
    pub nmi_vector: Option<u16>,
    pub irq_vector: Option<u16>,
    pub title: String,
    pub region: u8,
    pub checksum: u16,
    pub checksum_complement: u16,
    pub vectors: VectorInfo,
    pub header_score: i32,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct LoadedRom {
    pub info: RomInfo,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct HeaderCandidate {
    offset: usize,
    score: i32,
    mapping: MappingKind,
    title: String,
    region: u8,
    checksum: u16,
    checksum_complement: u16,
    vectors: VectorInfo,
}

pub fn load_rom(path: &Path) -> io::Result<LoadedRom> {
    let raw = fs::read(path)?;
    let (bytes, has_copier_header) = strip_copier_header(&raw);
    let mut warnings = Vec::new();
    let header = detect_header(&bytes).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "no plausible SNES header candidate found",
        )
    })?;

    if !header.mapping.supports_v1_disasm() {
        warnings.push(format!(
            "mapping {} recognized but disassembly currently supports LoROM only",
            header.mapping.name()
        ));
    }

    let info = RomInfo {
        path: path.display().to_string(),
        size: bytes.len(),
        source_size: raw.len(),
        has_copier_header,
        mapping: header.mapping,
        header_offset: header.offset,
        reset_vector: nonzero_vector(header.vectors.emulation_reset),
        nmi_vector: nonzero_vector(header.vectors.emulation_nmi),
        irq_vector: nonzero_vector(header.vectors.emulation_irq),
        title: header.title,
        region: header.region,
        checksum: header.checksum,
        checksum_complement: header.checksum_complement,
        vectors: header.vectors,
        header_score: header.score,
        warnings,
    };

    Ok(LoadedRom { info, bytes })
}

pub fn strip_copier_header(bytes: &[u8]) -> (Vec<u8>, bool) {
    if bytes.len() >= 0x8000 && bytes.len() % 1024 == 512 {
        (bytes[512..].to_vec(), true)
    } else {
        (bytes.to_vec(), false)
    }
}

pub fn vector_targets(info: &RomInfo) -> Vec<(String, u16, Option<usize>, CpuAddress)> {
    let entries = [
        ("reset_entry", info.vectors.emulation_reset),
        ("nmi_entry", info.vectors.emulation_nmi),
        ("irq_entry", info.vectors.emulation_irq),
        ("cop_entry", info.vectors.emulation_cop),
        ("abort_entry", info.vectors.emulation_abort),
        ("brk_entry", info.vectors.emulation_brk),
        ("native_nmi_entry", info.vectors.native_nmi),
        ("native_irq_entry", info.vectors.native_irq),
        ("native_cop_entry", info.vectors.native_cop),
        ("native_abort_entry", info.vectors.native_abort),
        ("native_brk_entry", info.vectors.native_brk),
    ];

    entries
        .into_iter()
        .filter(|(_, vector)| *vector >= 0x8000)
        .map(|(name, vector)| {
            (
                name.to_string(),
                vector,
                lorom_vector_target_to_pc(vector, info.size),
                CpuAddress::new(0x80, vector),
            )
        })
        .collect()
}

pub fn header_region(info: &RomInfo) -> std::ops::Range<usize> {
    info.header_offset..(info.header_offset + 0x40).min(info.size)
}

pub fn vector_region(info: &RomInfo) -> std::ops::Range<usize> {
    (info.header_offset + 0x24).min(info.size)..(info.header_offset + 0x40).min(info.size)
}

pub fn format_reset_summary(info: &RomInfo) -> String {
    match info
        .reset_vector
        .and_then(|v| lorom_vector_target_to_pc(v, info.size))
    {
        Some(pc) => format!(
            "{} -> PC {}",
            CpuAddress::new(0x80, info.reset_vector.unwrap_or(0)).format_snes(),
            crate::mapper::format_pc(pc)
        ),
        None => "unmapped".to_string(),
    }
}

fn detect_header(bytes: &[u8]) -> Option<HeaderCandidate> {
    HEADER_CANDIDATES
        .iter()
        .copied()
        .filter(|offset| offset + 0x50 <= bytes.len())
        .map(|offset| parse_header_candidate(bytes, offset))
        .max_by_key(|candidate| candidate.score)
        .filter(|candidate| candidate.score >= 8)
}

fn parse_header_candidate(bytes: &[u8], offset: usize) -> HeaderCandidate {
    let map_mode = bytes[offset + 0x15];
    let vectors = VectorInfo {
        native_cop: read_u16(bytes, offset + 0x24),
        native_brk: read_u16(bytes, offset + 0x26),
        native_abort: read_u16(bytes, offset + 0x28),
        native_nmi: read_u16(bytes, offset + 0x2A),
        native_reset: read_u16(bytes, offset + 0x2C),
        native_irq: read_u16(bytes, offset + 0x2E),
        emulation_cop: read_u16(bytes, offset + 0x34),
        emulation_abort: read_u16(bytes, offset + 0x38),
        emulation_nmi: read_u16(bytes, offset + 0x3A),
        emulation_reset: read_u16(bytes, offset + 0x3C),
        emulation_brk: read_u16(bytes, offset + 0x3E),
        emulation_irq: read_u16(bytes, offset + 0x3E),
    };
    HeaderCandidate {
        offset,
        score: score_header(bytes, offset),
        mapping: infer_mapping(offset, map_mode),
        title: sanitize_title(&bytes[offset..offset + 0x15]),
        region: bytes[offset + 0x19],
        checksum_complement: read_u16(bytes, offset + 0x1C),
        checksum: read_u16(bytes, offset + 0x1E),
        vectors,
    }
}

fn score_header(bytes: &[u8], offset: usize) -> i32 {
    let map_mode = bytes[offset + 0x15] & !0x10;
    let checksum_complement = read_u16(bytes, offset + 0x1C);
    let checksum = read_u16(bytes, offset + 0x1E);
    let reset_vector = read_u16(bytes, offset + 0x3C);
    if reset_vector < 0x8000 {
        return 0;
    }
    let opcode_offset = (offset & !0x7FFF) | (usize::from(reset_vector) & 0x7FFF);
    if opcode_offset >= bytes.len() {
        return 0;
    }
    let opcode = bytes[opcode_offset];
    let mut score = 0;
    if matches!(
        opcode,
        0x78 | 0x18 | 0x38 | 0x4C | 0x5C | 0xC2 | 0xE2 | 0x20 | 0x22
    ) {
        score += 8;
    }
    if matches!(opcode, 0xA9 | 0xA2 | 0xA0 | 0x9C | 0xAD | 0xAE | 0xAC) {
        score += 4;
    }
    if matches!(
        opcode,
        0x40 | 0x60 | 0x6B | 0x00 | 0x02 | 0xDB | 0x42 | 0xFF
    ) {
        score -= 8;
    }
    if checksum.wrapping_add(checksum_complement) == 0xFFFF {
        score += 4;
    }
    if offset == 0x7FC0 && map_mode == 0x20 {
        score += 2;
    }
    if offset == 0xFFC0 && map_mode == 0x21 {
        score += 2;
    }
    score.max(0)
}

fn infer_mapping(offset: usize, map_mode: u8) -> MappingKind {
    match map_mode & !0x10 {
        0x20 => MappingKind::LoRom,
        0x21 => MappingKind::HiRom,
        0x25 => MappingKind::ExHiRom,
        0x2A => MappingKind::ExLoRom,
        _ => match offset {
            0x7FC0 | 0x40_7FC0 => MappingKind::LoRom,
            0xFFC0 | 0x40_FFC0 => MappingKind::HiRom,
            _ => MappingKind::Unknown,
        },
    }
}

fn sanitize_title(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        let ch = if (0x20..=0x7E).contains(byte) {
            *byte as char
        } else {
            ' '
        };
        out.push(ch);
    }
    out.trim().to_string()
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn nonzero_vector(vector: u16) -> Option<u16> {
    (vector >= 0x8000).then_some(vector)
}

pub fn reset_cpu_address(info: &RomInfo) -> Option<CpuAddress> {
    info.reset_vector
        .and_then(|vector| lorom_vector_target_to_pc(vector, info.size).map(|pc| pc_to_lorom(pc)))
}
