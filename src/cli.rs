use crate::disasm65816::{DisassemblyResult, analyze_rom};
use crate::mapper::pc_to_lorom;
use crate::rommap::{format_reset_summary, load_rom};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct Report<'a> {
    rom: &'a crate::rommap::RomInfo,
    counts: &'a crate::disasm65816::AnalysisCounts,
    unresolved_transfers: &'a [String],
    warnings: &'a [String],
}

#[derive(Serialize)]
struct CodeMap<'a> {
    classification: &'a [String],
    likely_data_regions: &'a [crate::disasm65816::DataRegion],
}

#[derive(Serialize)]
struct CfgReport<'a> {
    blocks: &'a [crate::disasm65816::BasicBlock],
    edges: &'a [crate::disasm65816::CfgEdge],
}

pub fn run_disasm_cli(args: &[String]) -> io::Result<()> {
    if !matches!(args.first().map(String::as_str), Some("rom")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected `disasm rom <path> --out <dir>`",
        ));
    }

    let rom_path = args
        .get(1)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing ROM path"))?;
    let mut out_dir = None::<PathBuf>;
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument `{other}`"),
                ));
            }
        }
        index += 1;
    }

    let out_dir = out_dir
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing --out <dir>"))?;
    fs::create_dir_all(&out_dir)?;

    let loaded = load_rom(&rom_path)?;
    let disasm = analyze_rom(&loaded.info, &loaded.bytes);

    write_text(
        &out_dir.join("disasm.txt"),
        &render_disasm(&loaded.info, &disasm),
    )?;
    write_json(
        &out_dir.join("report.json"),
        &Report {
            rom: &loaded.info,
            counts: &disasm.counts,
            unresolved_transfers: &disasm.unresolved_transfers,
            warnings: &disasm.warnings,
        },
    )?;
    write_json(
        &out_dir.join("code_map.json"),
        &CodeMap {
            classification: &disasm.classification,
            likely_data_regions: &disasm.data_regions,
        },
    )?;
    write_json(
        &out_dir.join("cfg.json"),
        &CfgReport {
            blocks: &disasm.blocks,
            edges: &disasm.cfg_edges,
        },
    )?;
    write_json(&out_dir.join("labels.json"), &render_labels(&disasm.labels))?;

    println!(
        "disassembled {} -> {}",
        rom_path.display(),
        out_dir.display()
    );
    Ok(())
}

fn write_text(path: &Path, text: &str) -> io::Result<()> {
    fs::write(path, text)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    fs::write(path, text)
}

fn render_labels(labels: &BTreeMap<usize, String>) -> BTreeMap<String, String> {
    labels
        .iter()
        .map(|(pc, label)| (pc_to_lorom(*pc).format_snes(), label.clone()))
        .collect()
}

fn render_disasm(info: &crate::rommap::RomInfo, disasm: &DisassemblyResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("; ROM: {}\n", info.path));
    out.push_str(&format!("; Mapping: {}\n", info.mapping.name()));
    out.push_str(&format!("; Reset: {}\n", format_reset_summary(info)));
    out.push('\n');

    let mut ordered = disasm.instructions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(pc, _)| **pc);
    for (pc, instruction) in ordered {
        if let Some(label) = disasm.labels.get(pc) {
            out.push_str(label);
            out.push_str(":\n");
        }
        let bytes = instruction
            .bytes_
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut line = format!(
            "{}  {:<11} {}",
            pc_to_lorom(*pc).format_snes(),
            bytes,
            instruction.mnemonic
        );
        if !instruction.operand.is_empty() {
            line.push(' ');
            line.push_str(&instruction.operand);
        }
        if let Some(target_pc) = instruction.target_pc {
            if let Some(label) = disasm.labels.get(&target_pc) {
                line.push_str(&format!(" ; -> {label}"));
            } else {
                line.push_str(&format!(" ; -> {}", pc_to_lorom(target_pc).format_snes()));
            }
        }
        if instruction.confidence != "high" {
            line.push_str(&format!(" ; confidence={}", instruction.confidence));
        }
        if !instruction.notes.is_empty() {
            line.push_str(&format!(" ; {}", instruction.notes.join(", ")));
        }
        out.push_str(&line);
        out.push('\n');
    }

    out.push('\n');
    out.push_str("; Summary\n");
    out.push_str(&format!(
        "; reachable_code_bytes={} untouched_bytes={} basic_blocks={} subroutines={} unresolved_indirect_jumps={}\n",
        disasm.counts.reachable_code_bytes,
        disasm.counts.untouched_bytes,
        disasm.counts.basic_blocks,
        disasm.counts.subroutines,
        disasm.counts.unresolved_indirect_jumps
    ));
    if !disasm.jump_tables.is_empty() {
        out.push_str("\n; Jump tables\n");
        for candidate in &disasm.jump_tables {
            let targets = candidate
                .targets
                .iter()
                .map(|pc| pc_to_lorom(*pc).format_snes())
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "; {} @ {} confidence={} targets=[{}]\n",
                disasm
                    .labels
                    .get(&candidate.table_pc)
                    .cloned()
                    .unwrap_or_else(|| format!("jtbl_{:06X}", candidate.table_pc)),
                candidate.table_addr.format_snes(),
                candidate.confidence,
                targets
            ));
        }
    }
    if !disasm.warnings.is_empty() {
        out.push_str("\n; Warnings\n");
        for warning in &disasm.warnings {
            out.push_str(&format!("; {warning}\n"));
        }
    }
    out
}
