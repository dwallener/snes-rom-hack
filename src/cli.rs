use crate::disasm65816::{DisassemblyResult, analyze_rom, analyze_rom_with_seeds};
use crate::annotate::run_annotate_evidence_cli;
use crate::asset_paths::run_asset_paths_cli;
use crate::capture::run_collect_trace_cli;
use crate::mapper::pc_to_lorom;
use crate::player_gfx::{
    run_match_player_gfx_sheet_cli, run_patch_player_gfx_cli, run_player_gfx_report_cli,
};
use crate::replacement::run_replacement_report_cli;
use crate::rommap::{format_reset_summary, load_rom};
use crate::runtime::{
    correlate_runtime_lines, extract_runtime_seed_pcs, format_runtime_summary, load_labels_by_pc,
    load_runtime_cfg,
};
use crate::usage::run_usage_map_import_cli;
use crate::evidence::run_evidence_report_cli;
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
            "expected `disasm rom <path> --out <dir> [--runtime-seeds <trace.jsonl>]`",
        ));
    }

    let rom_path = args
        .get(1)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing ROM path"))?;
    let mut out_dir = None::<PathBuf>;
    let mut runtime_seed_path = None::<PathBuf>;
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--runtime-seeds" => {
                index += 1;
                runtime_seed_path = args.get(index).map(PathBuf::from);
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
    let disasm = if let Some(seed_path) = runtime_seed_path {
        let lines = fs::read_to_string(seed_path)?
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let seed_pcs = extract_runtime_seed_pcs(&lines)?;
        analyze_rom_with_seeds(&loaded.info, &loaded.bytes, &seed_pcs)
    } else {
        analyze_rom(&loaded.info, &loaded.bytes)
    };

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

pub fn run_runtime_correlate_cli(args: &[String]) -> io::Result<()> {
    let mut input_path = None::<PathBuf>;
    let mut labels_path = None::<PathBuf>;
    let mut cfg_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                index += 1;
                input_path = args.get(index).map(PathBuf::from);
            }
            "--labels" => {
                index += 1;
                labels_path = args.get(index).map(PathBuf::from);
            }
            "--cfg" => {
                index += 1;
                cfg_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `runtime-correlate --input <jsonl> --labels <labels.json> --cfg <cfg.json> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let input_path = input_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--input <jsonl>` for `runtime-correlate`",
        )
    })?;
    let labels_path = labels_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--labels <labels.json>` for `runtime-correlate`",
        )
    })?;
    let cfg_path = cfg_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--cfg <cfg.json>` for `runtime-correlate`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `runtime-correlate`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let labels = load_labels_by_pc(&fs::read_to_string(&labels_path)?)?;
    let cfg = load_runtime_cfg(&fs::read_to_string(&cfg_path)?)?;
    let lines = fs::read_to_string(&input_path)?
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let result = correlate_runtime_lines(&labels, &cfg, &lines)?;

    write_text(
        &out_dir.join("runtime_summary.txt"),
        &format_runtime_summary(&result),
    )?;
    write_json(&out_dir.join("runtime_report.json"), &result.report)?;
    write_json(&out_dir.join("annotated_events.json"), &result.events)?;

    println!(
        "correlated runtime log {} -> {}",
        input_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn run_usage_import_cli(args: &[String]) -> io::Result<()> {
    run_usage_map_import_cli(args)
}

pub fn run_evidence_cli(args: &[String]) -> io::Result<()> {
    run_evidence_report_cli(args)
}

pub fn run_annotate_cli(args: &[String]) -> io::Result<()> {
    run_annotate_evidence_cli(args)
}

pub fn run_asset_paths_report_cli(args: &[String]) -> io::Result<()> {
    run_asset_paths_cli(args)
}

pub fn run_phase2_cli(args: &[String]) -> io::Result<()> {
    let mut rom_path = None::<PathBuf>;
    let mut trace_path = None::<PathBuf>;
    let mut usage_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut usage_format = "simple-bits".to_string();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--trace" => {
                index += 1;
                trace_path = args.get(index).map(PathBuf::from);
            }
            "--usage" => {
                index += 1;
                usage_path = args.get(index).map(PathBuf::from);
            }
            "--usage-format" => {
                index += 1;
                usage_format = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing usage format"))?;
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `phase2-analyze --rom <path> --trace <trace.jsonl> --usage <usage.bin|cdl> --out <dir> [--usage-format simple-bits|bizhawk-cdl-snes]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing `--rom <path>` for `phase2-analyze`"))?;
    let trace_path = trace_path.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing `--trace <trace.jsonl>` for `phase2-analyze`"))?;
    let usage_path = usage_path.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing `--usage <usage.bin|cdl>` for `phase2-analyze`"))?;
    let out_dir = out_dir.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing `--out <dir>` for `phase2-analyze`"))?;

    fs::create_dir_all(&out_dir)?;
    run_disasm_cli(&[
        "rom".to_string(),
        rom_path.display().to_string(),
        "--runtime-seeds".to_string(),
        trace_path.display().to_string(),
        "--out".to_string(),
        out_dir.display().to_string(),
    ])?;

    let runtime_dir = out_dir.join("runtime");
    run_runtime_correlate_cli(&[
        "--input".to_string(),
        trace_path.display().to_string(),
        "--labels".to_string(),
        out_dir.join("labels.json").display().to_string(),
        "--cfg".to_string(),
        out_dir.join("cfg.json").display().to_string(),
        "--out".to_string(),
        runtime_dir.display().to_string(),
    ])?;

    let usage_dir = out_dir.join("usage");
    run_usage_import_cli(&[
        "--rom".to_string(),
        rom_path.display().to_string(),
        "--input".to_string(),
        usage_path.display().to_string(),
        "--labels".to_string(),
        out_dir.join("labels.json").display().to_string(),
        "--code-map".to_string(),
        out_dir.join("code_map.json").display().to_string(),
        "--out".to_string(),
        usage_dir.display().to_string(),
        "--format".to_string(),
        usage_format,
    ])?;

    let evidence_dir = out_dir.join("evidence");
    run_evidence_cli(&[
        "--runtime-report".to_string(),
        runtime_dir.join("runtime_report.json").display().to_string(),
        "--usage-report".to_string(),
        usage_dir.join("usage_report.json").display().to_string(),
        "--out".to_string(),
        evidence_dir.display().to_string(),
    ])?;

    let annotated_dir = out_dir.join("annotated");
    run_annotate_cli(&[
        "--disasm".to_string(),
        out_dir.join("disasm.txt").display().to_string(),
        "--labels".to_string(),
        out_dir.join("labels.json").display().to_string(),
        "--evidence".to_string(),
        evidence_dir.join("evidence_report.json").display().to_string(),
        "--out".to_string(),
        annotated_dir.display().to_string(),
    ])?;

    let asset_paths_dir = out_dir.join("asset-paths");
    run_asset_paths_report_cli(&[
        "--events".to_string(),
        runtime_dir.join("annotated_events.json").display().to_string(),
        "--evidence".to_string(),
        evidence_dir.join("evidence_report.json").display().to_string(),
        "--out".to_string(),
        asset_paths_dir.display().to_string(),
    ])?;

    println!("phase2 pipeline complete -> {}", out_dir.display());
    Ok(())
}

pub fn run_collect_trace_wrapper_cli(args: &[String]) -> io::Result<()> {
    run_collect_trace_cli(args)
}

pub fn run_replacement_cli(args: &[String]) -> io::Result<()> {
    run_replacement_report_cli(args)
}

pub fn run_player_gfx_cli(args: &[String]) -> io::Result<()> {
    run_player_gfx_report_cli(args)
}

pub fn run_patch_player_gfx_wrapper_cli(args: &[String]) -> io::Result<()> {
    run_patch_player_gfx_cli(args)
}

pub fn run_match_player_gfx_sheet_wrapper_cli(args: &[String]) -> io::Result<()> {
    run_match_player_gfx_sheet_cli(args)
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
