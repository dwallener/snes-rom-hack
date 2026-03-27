use crate::runtime::{RuntimeCorrelationReport, RuntimeTransferEpisode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplacementCandidate {
    pub frames: String,
    pub producer: Option<String>,
    pub staging_writer: Option<String>,
    pub primary_routine: Option<String>,
    pub targets: Vec<String>,
    pub staging_buffers: Vec<String>,
    pub transfers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplacementReport {
    pub graphics_candidates: Vec<ReplacementCandidate>,
    pub palette_candidates: Vec<ReplacementCandidate>,
    pub sprite_candidates: Vec<ReplacementCandidate>,
    pub sound_candidates: Vec<ReplacementCandidate>,
    pub warnings: Vec<String>,
}

pub fn run_replacement_report_cli(args: &[String]) -> io::Result<()> {
    let mut runtime_report_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--runtime-report" => {
                index += 1;
                runtime_report_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `replacement-report --runtime-report <runtime_report.json> --out <dir>`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let runtime_report_path = runtime_report_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--runtime-report <runtime_report.json>` for `replacement-report`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `replacement-report`",
        )
    })?;

    fs::create_dir_all(&out_dir)?;

    let runtime: RuntimeCorrelationReport =
        serde_json::from_str(&fs::read_to_string(&runtime_report_path)?).map_err(io::Error::other)?;
    let report = derive_replacement_report(&runtime);

    fs::write(
        out_dir.join("replacement_report.json"),
        serde_json::to_string_pretty(&report).map_err(io::Error::other)?,
    )?;
    fs::write(
        out_dir.join("replacement_summary.txt"),
        format_replacement_report(&report),
    )?;

    println!(
        "derived replacement report {} -> {}",
        runtime_report_path.display(),
        out_dir.display()
    );
    Ok(())
}

pub fn derive_replacement_report(runtime: &RuntimeCorrelationReport) -> ReplacementReport {
    let mut graphics_candidates = Vec::new();
    let mut palette_candidates = Vec::new();
    let mut sprite_candidates = Vec::new();
    let mut sound_candidates = Vec::new();

    for episode in &runtime.top_episodes {
        let candidate = candidate_from_episode(episode);
        if episode.replacement_targets.iter().any(|item| item == "graphics") {
            graphics_candidates.push(candidate.clone());
        }
        if episode.replacement_targets.iter().any(|item| item == "palette") {
            palette_candidates.push(candidate.clone());
        }
        if episode
            .replacement_targets
            .iter()
            .any(|item| item == "sprite_attr")
        {
            sprite_candidates.push(candidate.clone());
        }
        if episode.replacement_targets.iter().any(|item| item == "sound") {
            sound_candidates.push(candidate);
        }
    }

    let mut warnings = Vec::new();
    if graphics_candidates.is_empty()
        && palette_candidates.is_empty()
        && sprite_candidates.is_empty()
        && sound_candidates.is_empty()
    {
        warnings.push("no replacement-oriented candidates found".to_string());
    }

    ReplacementReport {
        graphics_candidates,
        palette_candidates,
        sprite_candidates,
        sound_candidates,
        warnings,
    }
}

fn candidate_from_episode(episode: &RuntimeTransferEpisode) -> ReplacementCandidate {
    let mut notes = Vec::new();
    if episode.producer_candidate.is_some() && episode.primary_routine != episode.producer_candidate {
        notes.push("producer differs from primary hot routine".to_string());
    }
    if episode.staging_writer_candidate.is_some()
        && episode.staging_writer_candidate != episode.producer_candidate
    {
        notes.push("staging writer differs from upload-side producer".to_string());
    }
    if !episode.staging_buffers.is_empty() {
        notes.push("uses WRAM staging buffers".to_string());
    }

    ReplacementCandidate {
        frames: format!("{}..{}", episode.start_frame, episode.end_frame),
        producer: episode.producer_candidate.clone(),
        staging_writer: episode.staging_writer_candidate.clone(),
        primary_routine: episode.primary_routine.clone(),
        targets: episode.replacement_targets.clone(),
        staging_buffers: episode.staging_buffers.clone(),
        transfers: episode
            .transfers
            .iter()
            .map(|item| {
                format!(
                    "{} {} -> {} {} {}",
                    item.source, item.destination, item.size, item.pipeline, item.launch_kind
                )
            })
            .collect(),
        notes,
    }
}

pub fn format_replacement_report(report: &ReplacementReport) -> String {
    let mut out = String::new();
    out.push_str("; Replacement Candidates\n");
    append_section(&mut out, "Graphics", &report.graphics_candidates);
    append_section(&mut out, "Palette", &report.palette_candidates);
    append_section(&mut out, "Sprites", &report.sprite_candidates);
    append_section(&mut out, "Sound", &report.sound_candidates);
    if !report.warnings.is_empty() {
        out.push_str("\n; Warnings\n");
        for item in &report.warnings {
            out.push_str(&format!("; {item}\n"));
        }
    }
    out
}

fn append_section(out: &mut String, title: &str, items: &[ReplacementCandidate]) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("\n; {title}\n"));
    for item in items {
        out.push_str(&format!(
            "; frames={} producer={} staging_writer={} primary={} staging={:?} transfers={:?} notes={:?}\n",
            item.frames,
            item.producer.as_deref().unwrap_or("n/a"),
            item.staging_writer.as_deref().unwrap_or("n/a"),
            item.primary_routine.as_deref().unwrap_or("n/a"),
            item.staging_buffers,
            item.transfers,
            item.notes
        ));
    }
}
