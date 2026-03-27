use crate::evidence::CombinedEvidenceReport;
use crate::runtime::AnnotatedRuntimeEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetPathCandidate {
    pub routine: String,
    pub score: usize,
    pub tags: Vec<String>,
    pub runtime_frames: Vec<i64>,
    pub dma_channels: Vec<i64>,
    pub dma_sources: Vec<String>,
    pub dma_bbus: Vec<String>,
    pub dma_sizes: Vec<String>,
    pub vram_registers: Vec<String>,
    pub cgram_registers: Vec<String>,
    pub oam_registers: Vec<String>,
    pub apu_registers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetPathReport {
    pub candidates: Vec<AssetPathCandidate>,
    pub warnings: Vec<String>,
}

pub fn run_asset_paths_cli(args: &[String]) -> io::Result<()> {
    let mut events_path = None::<PathBuf>;
    let mut evidence_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--events" => {
                index += 1;
                events_path = args.get(index).map(PathBuf::from);
            }
            "--evidence" => {
                index += 1;
                evidence_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `asset-paths --events <annotated_events.json> --evidence <evidence_report.json> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let events_path = events_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--events <annotated_events.json>` for `asset-paths`",
        )
    })?;
    let evidence_path = evidence_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--evidence <evidence_report.json>` for `asset-paths`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `asset-paths`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let events: Vec<AnnotatedRuntimeEvent> =
        serde_json::from_str(&fs::read_to_string(&events_path)?).map_err(io::Error::other)?;
    let evidence: CombinedEvidenceReport =
        serde_json::from_str(&fs::read_to_string(&evidence_path)?).map_err(io::Error::other)?;
    let report = derive_asset_paths(&events, &evidence);

    fs::write(
        out_dir.join("asset_paths.json"),
        serde_json::to_string_pretty(&report).map_err(io::Error::other)?,
    )?;
    fs::write(out_dir.join("asset_paths.txt"), format_asset_paths(&report))?;

    println!(
        "derived asset paths {} + {} -> {}",
        events_path.display(),
        evidence_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn derive_asset_paths(
    events: &[AnnotatedRuntimeEvent],
    evidence: &CombinedEvidenceReport,
) -> AssetPathReport {
    let mut grouped = BTreeMap::<String, AssetPathCandidate>::new();

    for event in events {
        let Some(routine) = event
            .subroutine
            .clone()
            .or_else(|| event.label.clone())
            .or_else(|| event.nearest_label.clone())
        else {
            continue;
        };
        let entry = grouped.entry(routine.clone()).or_insert_with(|| AssetPathCandidate {
            routine: routine.clone(),
            score: 0,
            tags: Vec::new(),
            runtime_frames: Vec::new(),
            dma_channels: Vec::new(),
            dma_sources: Vec::new(),
            dma_bbus: Vec::new(),
            dma_sizes: Vec::new(),
            vram_registers: Vec::new(),
            cgram_registers: Vec::new(),
            oam_registers: Vec::new(),
            apu_registers: Vec::new(),
        });

        if let Some(frame) = event.frame {
            push_unique(&mut entry.runtime_frames, frame);
        }
        if event.kind.contains("dma") {
            if let Some(channel) = event.channel {
                push_unique(&mut entry.dma_channels, channel);
            }
            if let Some(src) = &event.dma_source {
                push_unique(&mut entry.dma_sources, src.clone());
            }
            if let Some(bbus) = &event.dma_bbus {
                push_unique(&mut entry.dma_bbus, bbus.clone());
            }
            if let Some(size) = &event.dma_size {
                push_unique(&mut entry.dma_sizes, size.clone());
            }
        }
        match event.kind.as_str() {
            "vram_reg" => {
                if let Some(address) = &event.address {
                    push_unique(&mut entry.vram_registers, address.clone());
                }
            }
            "cgram_reg" => {
                if let Some(address) = &event.address {
                    push_unique(&mut entry.cgram_registers, address.clone());
                }
            }
            "oam_reg" => {
                if let Some(address) = &event.address {
                    push_unique(&mut entry.oam_registers, address.clone());
                }
            }
            "apu_io_reg" => {
                if let Some(address) = &event.address {
                    push_unique(&mut entry.apu_registers, address.clone());
                }
            }
            _ => {}
        }
    }

    let evidence_by_name = evidence
        .top_routines
        .iter()
        .map(|item| (item.name.clone(), item))
        .collect::<BTreeMap<_, _>>();

    let mut candidates = grouped
        .into_values()
        .filter_map(|mut candidate| {
            let evidence = evidence_by_name.get(&candidate.routine)?;
            candidate.score = evidence.score;
            candidate.tags = evidence.tags.clone();
            Some(candidate)
        })
        .filter(|candidate| {
            !candidate.dma_channels.is_empty()
                || !candidate.vram_registers.is_empty()
                || !candidate.cgram_registers.is_empty()
                || !candidate.oam_registers.is_empty()
                || !candidate.apu_registers.is_empty()
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.dma_channels.len().cmp(&left.dma_channels.len()))
            .then_with(|| left.routine.cmp(&right.routine))
    });

    let mut warnings = Vec::new();
    if candidates.is_empty() {
        warnings.push("no asset-path candidates found".to_string());
    }

    AssetPathReport { candidates, warnings }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, value: T) {
    if !items.contains(&value) {
        items.push(value);
    }
}

pub fn format_asset_paths(report: &AssetPathReport) -> String {
    let mut out = String::new();
    out.push_str("; Asset Path Candidates\n");
    for candidate in &report.candidates {
        out.push_str(&format!(
            "; {} score={} tags={} dma_channels={:?} dma_sources={:?} dma_bbus={:?} dma_sizes={:?} vram={:?} cgram={:?} oam={:?} apu={:?}\n",
            candidate.routine,
            candidate.score,
            candidate.tags.join(","),
            candidate.dma_channels,
            candidate.dma_sources,
            candidate.dma_bbus,
            candidate.dma_sizes,
            candidate.vram_registers,
            candidate.cgram_registers,
            candidate.oam_registers,
            candidate.apu_registers
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
