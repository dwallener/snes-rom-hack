use super::assets::{
    FrameEntity, load_asset_bundle, render_scene_frame, resolve_asset_references, write_rgba_preview,
};
use super::content::{build_room_asset_table, load_compiled_content};
use super::engine::{apply_input_frame, build_engine_plan, initial_boot_scene, runtime_entity_states};
use super::runtime::default_runtime_skeleton;
use super::{default_content_contracts, load_manifest};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SimulationReport {
    pub scene_id: String,
    pub input_sequence: String,
    pub frames: Vec<SimulationFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SimulationFrame {
    pub frame_index: usize,
    pub input: String,
    pub player_x: u32,
    pub player_y: u32,
    pub npc_x: u32,
    pub npc_y: u32,
    pub output_file: String,
}

pub(crate) fn run_template_simulate_cli(args: &[String]) -> io::Result<()> {
    let mut project = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut scene_id = None::<String>;
    let mut input = "RRRR..DDLL..UU..RR".to_string();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--project" => {
                index += 1;
                project = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--scene" => {
                index += 1;
                scene_id = args.get(index).cloned();
            }
            "--input" => {
                index += 1;
                input = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing input sequence"))?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `template simulate --project <dir> --out <dir> [--scene <id>] [--input <sequence>]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let project = project.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "missing `--project <dir>` for `template simulate`")
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "missing `--out <dir>` for `template simulate`")
    })?;

    let manifest = load_manifest(&project)?;
    let contracts = default_content_contracts(manifest.template);
    let content = load_compiled_content(&project, &manifest, &contracts)?;
    let assets = load_asset_bundle(&project, manifest.template)?;
    let room_table = build_room_asset_table(&content);
    let resolution = resolve_asset_references(&content, &assets, &room_table)?;
    let runtime = default_runtime_skeleton(manifest.template);
    let packets = super::assets::build_scene_load_packets(
        &std::env::temp_dir().join("template-sim-packets"),
        &resolution,
        &resolution.entities,
    )?;
    let engine_plan = build_engine_plan(&content, &resolution, &packets, &runtime)?;
    let default_scene = initial_boot_scene(&engine_plan).to_string();
    let scene = pick_scene(&content, scene_id.as_deref().or(Some(default_scene.as_str())))?;

    fs::create_dir_all(&out_dir)?;
    let frames_dir = out_dir.join("frames");
    fs::create_dir_all(&frames_dir)?;

    let spawn = parse_spawn(&scene.player_spawn);
    let mut states = runtime_entity_states(&engine_plan, spawn);

    let mut frames = Vec::new();
    for (frame_index, raw) in input.chars().enumerate() {
        apply_input_frame(&engine_plan, raw, spawn, &mut states);
        let entities = states
            .iter()
            .map(|state| FrameEntity {
                entity_id: state.entity_id.clone(),
                x: state.x,
                y: state.y,
                frame: state.frame,
            })
            .collect::<Vec<_>>();
        let (rgba, width, height) = render_scene_frame(scene, &assets, &resolution, &entities)?;
        let output_file = format!("frame_{frame_index:03}.png");
        write_rgba_preview(&frames_dir.join(&output_file), width, height, &rgba)?;
        let player = states.iter().find(|state| state.entity_id == "player");
        let npc = states.iter().find(|state| state.entity_id == "npc_ball");
        frames.push(SimulationFrame {
            frame_index,
            input: raw.to_string(),
            player_x: player.map(|s| s.x).unwrap_or(0),
            player_y: player.map(|s| s.y).unwrap_or(0),
            npc_x: npc.map(|s| s.x).unwrap_or(0),
            npc_y: npc.map(|s| s.y).unwrap_or(0),
            output_file,
        });
    }

    let report = SimulationReport {
        scene_id: scene.id.clone(),
        input_sequence: input,
        frames,
    };
    fs::write(
        out_dir.join("simulation_report.json"),
        serde_json::to_vec_pretty(&report).map_err(io::Error::other)?,
    )?;
    fs::write(
        out_dir.join("simulation_summary.txt"),
        render_simulation_summary(&report),
    )?;
    println!("simulated template scene {} -> {}", project.display(), out_dir.display());
    Ok(())
}

fn pick_scene<'a>(
    content: &'a super::content::CompiledContent,
    requested: Option<&str>,
) -> io::Result<&'a super::content::SceneDef> {
    if let Some(requested) = requested {
        return content
            .scenes
            .iter()
            .find(|scene| scene.id == requested)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "requested scene not found"));
    }
    content
        .scenes
        .iter()
        .find(|scene| scene.id == requested.unwrap_or(""))
        .or_else(|| content.scenes.iter().find(|scene| scene.kind == "gameplay"))
        .or_else(|| content.scenes.first())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no scenes found"))
}

fn parse_spawn(raw: &str) -> (u32, u32) {
    let Some((x, y)) = raw.split_once(',') else {
        return (32, 32);
    };
    let x = x.trim().parse::<u32>().unwrap_or(8) * 4;
    let y = y.trim().parse::<u32>().unwrap_or(8) * 4;
    (x.min(108), y.min(92))
}

fn render_simulation_summary(report: &SimulationReport) -> String {
    let mut out = String::new();
    out.push_str("Template Simulation Summary\n");
    out.push_str(&format!("scene: {}\n", report.scene_id));
    out.push_str(&format!("input: {}\n\n", report.input_sequence));
    for frame in &report.frames {
        out.push_str(&format!(
            "- frame {:03} input={} player=({}, {}) npc=({}, {}) file={}\n",
            frame.frame_index,
            frame.input,
            frame.player_x,
            frame.player_y,
            frame.npc_x,
            frame.npc_y,
            frame.output_file
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{render_simulation_summary, SimulationReport, SimulationFrame};

    #[test]
    fn summary_lists_frames() {
        let report = SimulationReport {
            scene_id: "room_000".to_string(),
            input_sequence: "R.".to_string(),
            frames: vec![SimulationFrame {
                frame_index: 0,
                input: "R".to_string(),
                player_x: 10,
                player_y: 10,
                npc_x: 20,
                npc_y: 10,
                output_file: "frame_000.png".to_string(),
            }],
        };
        let text = render_simulation_summary(&report);
        assert!(text.contains("frame 000"));
    }
}
