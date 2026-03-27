use crate::runtime::load_labels_by_pc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const USAGE_EXECUTED: u8 = 1 << 0;
const USAGE_DATA: u8 = 1 << 1;
const BIZHAWK_SNES_EXEC_FIRST: u8 = 0x01;
const BIZHAWK_SNES_EXEC_OPERAND: u8 = 0x02;
const BIZHAWK_SNES_CPU_DATA: u8 = 0x04;
const BIZHAWK_SNES_DMA_DATA: u8 = 0x08;

#[derive(Clone, Debug, Deserialize)]
struct CodeMapFile {
    classification: Vec<String>,
    likely_data_regions: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRoutineActivity {
    pub name: String,
    pub executed_bytes: usize,
    pub data_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageImportReport {
    pub rom_size: usize,
    pub usage_size: usize,
    pub observed_executed_bytes: usize,
    pub observed_data_bytes: usize,
    pub observed_unknown_to_code: usize,
    pub observed_unknown_to_data: usize,
    pub code_data_overlap_bytes: usize,
    pub top_routines: Vec<UsageRoutineActivity>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct UsageImportResult {
    pub merged_classification: Vec<String>,
    pub report: UsageImportReport,
}

pub fn run_usage_map_import_cli(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut input_path = None::<PathBuf>;
    let mut labels_path = None::<PathBuf>;
    let mut code_map_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut format = "simple-bits".to_string();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--input" => {
                index += 1;
                input_path = args.get(index).map(PathBuf::from);
            }
            "--labels" => {
                index += 1;
                labels_path = args.get(index).map(PathBuf::from);
            }
            "--code-map" => {
                index += 1;
                code_map_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--format" => {
                index += 1;
                format = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing format"))?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `usage-map-import --rom <path> --input <usage.bin> --labels <labels.json> --code-map <code_map.json> --out <dir> [--format simple-bits]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    if !matches!(format.as_str(), "simple-bits" | "bizhawk-cdl-snes") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported usage map format `{format}`"),
        ));
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `usage-map-import`",
        )
    })?;
    let input_path = input_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--input <usage.bin>` for `usage-map-import`",
        )
    })?;
    let labels_path = labels_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--labels <labels.json>` for `usage-map-import`",
        )
    })?;
    let code_map_path = code_map_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--code-map <code_map.json>` for `usage-map-import`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `usage-map-import`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let loaded = crate::rommap::load_rom(&rom_path)?;
    let labels = load_labels_by_pc(&fs::read_to_string(&labels_path)?)?;
    let code_map: CodeMapFile =
        serde_json::from_str(&fs::read_to_string(&code_map_path)?).map_err(io::Error::other)?;
    let usage_bytes = fs::read(&input_path)?;
    let usage = match format.as_str() {
        "simple-bits" => usage_bytes,
        "bizhawk-cdl-snes" => load_bizhawk_cdl_snes_usage(&usage_bytes, loaded.bytes.len())?,
        _ => unreachable!(),
    };

    let result = import_usage_map(&loaded.bytes, &usage, &labels, &code_map.classification)?;
    fs::write(
        out_dir.join("usage_summary.txt"),
        format_usage_summary(&result.report),
    )?;
    fs::write(
        out_dir.join("usage_report.json"),
        serde_json::to_string_pretty(&result.report).map_err(io::Error::other)?,
    )?;
    fs::write(
        out_dir.join("merged_code_map.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "classification": result.merged_classification,
            "likely_data_regions": code_map.likely_data_regions,
        }))
        .map_err(io::Error::other)?,
    )?;

    println!(
        "imported usage map {} -> {}",
        input_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn import_usage_map(
    rom: &[u8],
    usage: &[u8],
    labels_by_pc: &BTreeMap<usize, String>,
    classification: &[String],
) -> io::Result<UsageImportResult> {
    if usage.len() != rom.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "usage map size {} does not match normalized ROM size {}",
                usage.len(),
                rom.len()
            ),
        ));
    }
    if classification.len() != rom.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "code map classification size {} does not match normalized ROM size {}",
                classification.len(),
                rom.len()
            ),
        ));
    }

    let mut merged = classification.to_vec();
    let mut observed_executed_bytes = 0usize;
    let mut observed_data_bytes = 0usize;
    let mut observed_unknown_to_code = 0usize;
    let mut observed_unknown_to_data = 0usize;
    let mut code_data_overlap_bytes = 0usize;
    let mut routine_counts = BTreeMap::<String, (usize, usize)>::new();
    let mut warnings = Vec::new();

    for (pc, flags) in usage.iter().copied().enumerate() {
        let executed = flags & USAGE_EXECUTED != 0;
        let data = flags & USAGE_DATA != 0;
        if executed {
            observed_executed_bytes += 1;
        }
        if data {
            observed_data_bytes += 1;
        }
        if !executed && !data {
            continue;
        }

        let current = merged[pc].clone();
        let next = merge_usage_classification(&current, executed, data);
        if next == "observed_code" && current == "unknown" {
            observed_unknown_to_code += 1;
        }
        if next == "observed_data" && current == "unknown" {
            observed_unknown_to_data += 1;
        }
        if next == "code_data_overlap" {
            code_data_overlap_bytes += 1;
        }
        if next != current {
            merged[pc] = next;
        }

        if let Some(routine) = find_routine_for_pc(labels_by_pc, pc) {
            let entry = routine_counts.entry(routine).or_insert((0, 0));
            if executed {
                entry.0 += 1;
            }
            if data {
                entry.1 += 1;
            }
        } else if executed {
            warnings.push(format!("observed execute byte with no routine context at PC 0x{pc:06X}"));
        }
    }

    let mut top_routines = routine_counts
        .into_iter()
        .map(|(name, (executed_bytes, data_bytes))| UsageRoutineActivity {
            name,
            executed_bytes,
            data_bytes,
        })
        .collect::<Vec<_>>();
    top_routines.sort_by(|left, right| {
        right
            .executed_bytes
            .cmp(&left.executed_bytes)
            .then_with(|| right.data_bytes.cmp(&left.data_bytes))
            .then_with(|| left.name.cmp(&right.name))
    });
    top_routines.truncate(16);
    warnings.truncate(32);

    Ok(UsageImportResult {
        merged_classification: merged,
        report: UsageImportReport {
            rom_size: rom.len(),
            usage_size: usage.len(),
            observed_executed_bytes,
            observed_data_bytes,
            observed_unknown_to_code,
            observed_unknown_to_data,
            code_data_overlap_bytes,
            top_routines,
            warnings,
        },
    })
}

fn load_bizhawk_cdl_snes_usage(bytes: &[u8], rom_size: usize) -> io::Result<Vec<u8>> {
    let mut cursor = 0usize;
    let id = read_leb128_string(bytes, &mut cursor)?;
    if id != "BIZHAWK-CDL-2" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported BizHawk CDL identifier `{id}`"),
        ));
    }
    let sub_id = read_leb128_string(bytes, &mut cursor)?;
    if sub_id.trim_end() != "SNES" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("BizHawk CDL is not for SNES: `{sub_id}`"),
        ));
    }
    let _sub_version = read_u32_le(bytes, &mut cursor)?;
    let block_count = read_u32_le(bytes, &mut cursor)? as usize;

    let mut cartrom = None::<Vec<u8>>;
    for _ in 0..block_count {
        let block_name = read_leb128_string(bytes, &mut cursor)?;
        let length = read_u32_le(bytes, &mut cursor)? as usize;
        let data = read_slice(bytes, &mut cursor, length)?.to_vec();
        if block_name == "CARTROM" {
            cartrom = Some(data);
        }
    }

    let cartrom = cartrom.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "BizHawk SNES CDL did not contain a `CARTROM` block",
        )
    })?;
    if cartrom.len() != rom_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "BizHawk CARTROM block size {} does not match normalized ROM size {}",
                cartrom.len(),
                rom_size
            ),
        ));
    }

    Ok(cartrom
        .into_iter()
        .map(|flags| {
            let mut usage = 0u8;
            if flags & (BIZHAWK_SNES_EXEC_FIRST | BIZHAWK_SNES_EXEC_OPERAND) != 0 {
                usage |= USAGE_EXECUTED;
            }
            if flags & (BIZHAWK_SNES_CPU_DATA | BIZHAWK_SNES_DMA_DATA) != 0 {
                usage |= USAGE_DATA;
            }
            usage
        })
        .collect())
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> io::Result<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "overflow while reading usage map")
    })?;
    let slice = bytes.get(*cursor..end).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected end of usage map while reading block",
        )
    })?;
    *cursor = end;
    Ok(slice)
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> io::Result<u32> {
    let raw = read_slice(bytes, cursor, 4)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_leb128_string(bytes: &[u8], cursor: &mut usize) -> io::Result<String> {
    let len = read_leb128_u32(bytes, cursor)? as usize;
    let raw = read_slice(bytes, cursor, len)?;
    std::str::from_utf8(raw)
        .map(|text| text.to_string())
        .map_err(io::Error::other)
}

fn read_leb128_u32(bytes: &[u8], cursor: &mut usize) -> io::Result<u32> {
    let mut shift = 0u32;
    let mut value = 0u32;
    loop {
        let byte = *bytes.get(*cursor).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected end of usage map while reading LEB128 value",
            )
        })?;
        *cursor += 1;
        value |= u32::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LEB128 value is too large",
            ));
        }
    }
}

fn merge_usage_classification(current: &str, executed: bool, data: bool) -> String {
    match (current, executed, data) {
        ("header", _, _) | ("vector", _, _) => current.to_string(),
        ("code", true, false) => "code".to_string(),
        ("code", false, true) | ("code", true, true) => "code_data_overlap".to_string(),
        ("observed_code", true, false) => "observed_code".to_string(),
        ("observed_code", false, true) | ("observed_code", true, true) => {
            "code_data_overlap".to_string()
        }
        ("observed_data", true, false) | ("observed_data", true, true) => {
            "code_data_overlap".to_string()
        }
        ("observed_data", false, true) => "observed_data".to_string(),
        ("referenced_data", true, false)
        | ("jump_table", true, false)
        | ("likely_data_or_unknown", true, false) => "observed_code".to_string(),
        ("referenced_data", false, true)
        | ("jump_table", false, true)
        | ("likely_data_or_unknown", false, true) => current.to_string(),
        ("referenced_data", true, true)
        | ("jump_table", true, true)
        | ("likely_data_or_unknown", true, true) => "code_data_overlap".to_string(),
        ("unknown", true, false) => "observed_code".to_string(),
        ("unknown", false, true) => "observed_data".to_string(),
        ("unknown", true, true) => "code_data_overlap".to_string(),
        (_, true, false) => "observed_code".to_string(),
        (_, false, true) => "observed_data".to_string(),
        (_, true, true) => "code_data_overlap".to_string(),
        _ => current.to_string(),
    }
}

fn find_routine_for_pc(labels_by_pc: &BTreeMap<usize, String>, pc: usize) -> Option<String> {
    labels_by_pc
        .range(..=pc)
        .rev()
        .find(|(_, label)| is_subroutine_like(label))
        .map(|(_, label)| label.clone())
}

fn is_subroutine_like(label: &str) -> bool {
    label.starts_with("sub_")
        || matches!(
            label,
            "reset_entry" | "nmi_entry" | "irq_entry" | "abort_entry" | "cop_entry"
        )
}

pub fn format_usage_summary(report: &UsageImportReport) -> String {
    let mut out = String::new();
    out.push_str("; Usage Map Summary\n");
    out.push_str(&format!(
        "; rom_size={} usage_size={} observed_execute={} observed_data={} unknown_to_code={} unknown_to_data={} code_data_overlap={}\n",
        report.rom_size,
        report.usage_size,
        report.observed_executed_bytes,
        report.observed_data_bytes,
        report.observed_unknown_to_code,
        report.observed_unknown_to_data,
        report.code_data_overlap_bytes
    ));
    if !report.top_routines.is_empty() {
        out.push_str("\n; Hot routines\n");
        for routine in &report.top_routines {
            out.push_str(&format!(
                "; {}: executed_bytes={} data_bytes={}\n",
                routine.name, routine.executed_bytes, routine.data_bytes
            ));
        }
    }
    if !report.warnings.is_empty() {
        out.push_str("\n; Warnings\n");
        for warning in &report.warnings {
            out.push_str(&format!("; {warning}\n"));
        }
    }
    out
}

#[allow(dead_code)]
fn _assert_path(_path: &Path) {}
