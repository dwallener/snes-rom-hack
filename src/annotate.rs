use crate::evidence::CombinedEvidenceReport;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct HotRoutineRecord {
    pub label: String,
    pub snes_address: String,
    pub score: usize,
    pub runtime_events: usize,
    pub dma_events: usize,
    pub vram_events: usize,
    pub cgram_events: usize,
    pub oam_events: usize,
    pub observed_executed_bytes: usize,
    pub observed_data_bytes: usize,
    pub tags: Vec<String>,
}

pub fn run_annotate_evidence_cli(args: &[String]) -> io::Result<()> {
    let mut disasm_path = None::<PathBuf>;
    let mut labels_path = None::<PathBuf>;
    let mut evidence_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--disasm" => {
                index += 1;
                disasm_path = args.get(index).map(PathBuf::from);
            }
            "--labels" => {
                index += 1;
                labels_path = args.get(index).map(PathBuf::from);
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
                        "unknown argument `{other}`; expected `annotate-evidence --disasm <disasm.txt> --labels <labels.json> --evidence <evidence_report.json> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let disasm_path = disasm_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--disasm <disasm.txt>` for `annotate-evidence`",
        )
    })?;
    let labels_path = labels_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--labels <labels.json>` for `annotate-evidence`",
        )
    })?;
    let evidence_path = evidence_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--evidence <evidence_report.json>` for `annotate-evidence`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `annotate-evidence`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let disasm = fs::read_to_string(&disasm_path)?;
    let labels: BTreeMap<String, String> =
        serde_json::from_str(&fs::read_to_string(&labels_path)?).map_err(io::Error::other)?;
    let evidence: CombinedEvidenceReport =
        serde_json::from_str(&fs::read_to_string(&evidence_path)?).map_err(io::Error::other)?;

    let annotated = annotate_disasm_with_evidence(&disasm, &evidence);
    let hot_routines = build_hot_routines(&labels, &evidence);

    fs::write(out_dir.join("annotated_disasm.txt"), annotated)?;
    fs::write(
        out_dir.join("hot_routines.json"),
        serde_json::to_string_pretty(&hot_routines).map_err(io::Error::other)?,
    )?;

    println!(
        "annotated evidence {} -> {}",
        evidence_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn annotate_disasm_with_evidence(disasm: &str, evidence: &CombinedEvidenceReport) -> String {
    let evidence_by_label = evidence
        .top_routines
        .iter()
        .map(|item| (item.name.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let mut out = String::new();
    for line in disasm.lines() {
        let trimmed = line.trim_end();
        if let Some(label) = trimmed.strip_suffix(':') {
            if let Some(item) = evidence_by_label.get(label) {
                out.push_str(&format!(
                    "; evidence score={} runtime={} dma={} vram={} cgram={} oam={} exec_bytes={} data_bytes={} tags={}\n",
                    item.score,
                    item.runtime_events,
                    item.dma_events,
                    item.vram_events,
                    item.cgram_events,
                    item.oam_events,
                    item.observed_executed_bytes,
                    item.observed_data_bytes,
                    item.tags.join(",")
                ));
            }
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

pub fn build_hot_routines(
    labels: &BTreeMap<String, String>,
    evidence: &CombinedEvidenceReport,
) -> Vec<HotRoutineRecord> {
    let addresses_by_label = labels
        .iter()
        .map(|(snes, label)| (label.clone(), snes.clone()))
        .collect::<BTreeMap<_, _>>();

    evidence
        .top_routines
        .iter()
        .map(|item| HotRoutineRecord {
            label: item.name.clone(),
            snes_address: addresses_by_label
                .get(&item.name)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            score: item.score,
            runtime_events: item.runtime_events,
            dma_events: item.dma_events,
            vram_events: item.vram_events,
            cgram_events: item.cgram_events,
            oam_events: item.oam_events,
            observed_executed_bytes: item.observed_executed_bytes,
            observed_data_bytes: item.observed_data_bytes,
            tags: item.tags.clone(),
        })
        .collect()
}
