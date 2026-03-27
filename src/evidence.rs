use crate::runtime::{RoutineActivity, RuntimeCorrelationReport};
use crate::usage::{UsageImportReport, UsageRoutineActivity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedRoutineEvidence {
    pub name: String,
    pub score: usize,
    pub runtime_events: usize,
    pub dma_events: usize,
    pub vram_events: usize,
    pub cgram_events: usize,
    pub oam_events: usize,
    pub sound_events: usize,
    pub other_ppu_events: usize,
    pub register_writes: usize,
    pub runtime_frames: Vec<i64>,
    pub observed_executed_bytes: usize,
    pub observed_data_bytes: usize,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedEvidenceReport {
    pub top_routines: Vec<CombinedRoutineEvidence>,
    pub warnings: Vec<String>,
}

pub fn run_evidence_report_cli(args: &[String]) -> io::Result<()> {
    let mut runtime_report_path = None::<PathBuf>;
    let mut usage_report_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--runtime-report" => {
                index += 1;
                runtime_report_path = args.get(index).map(PathBuf::from);
            }
            "--usage-report" => {
                index += 1;
                usage_report_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `evidence-report --runtime-report <runtime_report.json> --usage-report <usage_report.json> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let runtime_report_path = runtime_report_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--runtime-report <runtime_report.json>` for `evidence-report`",
        )
    })?;
    let usage_report_path = usage_report_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--usage-report <usage_report.json>` for `evidence-report`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `evidence-report`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let runtime: RuntimeCorrelationReport =
        serde_json::from_str(&fs::read_to_string(&runtime_report_path)?).map_err(io::Error::other)?;
    let usage: UsageImportReport =
        serde_json::from_str(&fs::read_to_string(&usage_report_path)?).map_err(io::Error::other)?;
    let report = combine_evidence(&runtime, &usage);

    fs::write(
        out_dir.join("evidence_report.json"),
        serde_json::to_string_pretty(&report).map_err(io::Error::other)?,
    )?;
    fs::write(
        out_dir.join("evidence_summary.txt"),
        format_evidence_summary(&report),
    )?;

    println!(
        "combined evidence {} + {} -> {}",
        runtime_report_path.display(),
        usage_report_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn combine_evidence(
    runtime: &RuntimeCorrelationReport,
    usage: &UsageImportReport,
) -> CombinedEvidenceReport {
    let mut routines = BTreeMap::<String, CombinedRoutineEvidence>::new();

    for item in &runtime.top_routines {
        let entry = routines
            .entry(item.name.clone())
            .or_insert_with(|| blank_routine(item.name.clone()));
        merge_runtime(entry, item);
    }

    for item in &usage.top_routines {
        let entry = routines
            .entry(item.name.clone())
            .or_insert_with(|| blank_routine(item.name.clone()));
        merge_usage(entry, item);
    }

    for item in routines.values_mut() {
        item.score = score_routine(item);
        item.tags = classify_tags(item);
    }

    let mut top_routines = routines.into_values().collect::<Vec<_>>();
    top_routines.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.runtime_events.cmp(&left.runtime_events))
            .then_with(|| right.observed_executed_bytes.cmp(&left.observed_executed_bytes))
            .then_with(|| left.name.cmp(&right.name))
    });
    top_routines.truncate(24);

    let mut warnings = Vec::new();
    if top_routines.is_empty() {
        warnings.push("no combined routine evidence found".to_string());
    }

    CombinedEvidenceReport {
        top_routines,
        warnings,
    }
}

fn blank_routine(name: String) -> CombinedRoutineEvidence {
    CombinedRoutineEvidence {
        name,
        score: 0,
        runtime_events: 0,
        dma_events: 0,
        vram_events: 0,
        cgram_events: 0,
        oam_events: 0,
        sound_events: 0,
        other_ppu_events: 0,
        register_writes: 0,
        runtime_frames: Vec::new(),
        observed_executed_bytes: 0,
        observed_data_bytes: 0,
        tags: Vec::new(),
    }
}

fn merge_runtime(target: &mut CombinedRoutineEvidence, item: &RoutineActivity) {
    target.runtime_events += item.total_events;
    target.dma_events += item.dma_events;
    target.vram_events += item.vram_events;
    target.cgram_events += item.cgram_events;
    target.oam_events += item.oam_events;
    target.sound_events += item.sound_events;
    target.other_ppu_events += item.other_ppu_events;
    target.register_writes += item.register_writes;
    for frame in &item.frames {
        if !target.runtime_frames.contains(frame) {
            target.runtime_frames.push(*frame);
        }
    }
    target.runtime_frames.sort_unstable();
}

fn merge_usage(target: &mut CombinedRoutineEvidence, item: &UsageRoutineActivity) {
    target.observed_executed_bytes += item.executed_bytes;
    target.observed_data_bytes += item.data_bytes;
}

fn score_routine(item: &CombinedRoutineEvidence) -> usize {
    item.dma_events * 8
        + item.vram_events * 6
        + item.cgram_events * 5
        + item.oam_events * 5
        + item.sound_events * 4
        + item.other_ppu_events * 2
        + item.register_writes
        + item.runtime_events
        + item.observed_executed_bytes
        + item.observed_data_bytes / 2
}

fn classify_tags(item: &CombinedRoutineEvidence) -> Vec<String> {
    let mut tags = Vec::new();
    if item.dma_events > 0 {
        tags.push("dma-heavy".to_string());
    }
    if item.vram_events > 0 || item.cgram_events > 0 || item.oam_events > 0 {
        tags.push("ppu-upload".to_string());
    }
    if item.sound_events > 0 {
        tags.push("sound-upload".to_string());
    }
    if item.observed_executed_bytes > 0 {
        tags.push("usage-backed-code".to_string());
    }
    if item.observed_data_bytes > 0 {
        tags.push("usage-backed-data".to_string());
    }
    if item.runtime_events > 0 && item.observed_executed_bytes > 0 {
        tags.push("runtime-and-usage".to_string());
    }
    tags
}

pub fn format_evidence_summary(report: &CombinedEvidenceReport) -> String {
    let mut out = String::new();
    out.push_str("; Combined Evidence Summary\n");
    for item in &report.top_routines {
        out.push_str(&format!(
            "; {} score={} runtime={} dma={} vram={} cgram={} oam={} sound={} exec_bytes={} data_bytes={} tags={}\n",
            item.name,
            item.score,
            item.runtime_events,
            item.dma_events,
            item.vram_events,
            item.cgram_events,
            item.oam_events,
            item.sound_events,
            item.observed_executed_bytes,
            item.observed_data_bytes,
            item.tags.join(",")
        ));
    }
    if !report.warnings.is_empty() {
        out.push_str("\n; Warnings\n");
        for warning in &report.warnings {
            out.push_str(&format!("; {warning}\n"));
        }
    }
    out
}
