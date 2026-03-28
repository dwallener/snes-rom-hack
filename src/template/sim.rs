use super::assets::{
    FrameEntity, load_asset_bundle, render_scene_frame, resolve_asset_references, write_rgba_preview,
};
use super::content::{build_room_asset_table, load_compiled_content};
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
    let scene = pick_scene(&content, scene_id.as_deref())?;

    fs::create_dir_all(&out_dir)?;
    let frames_dir = out_dir.join("frames");
    fs::create_dir_all(&frames_dir)?;

    let player = content
        .entities
        .iter()
        .find(|entity| entity.kind == "player")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing player entity"))?;
    let npc = content.entities.iter().find(|entity| entity.kind == "npc");

    let (mut player_x, mut player_y) = parse_spawn(&scene.player_spawn);
    let (base_npc_x, base_npc_y) = (player_x + 32, player_y);
    let mut npc_x = base_npc_x;
    let npc_y = base_npc_y;
    let mut npc_dir: i32 = 1;

    let mut frames = Vec::new();
    for (frame_index, raw) in input.chars().enumerate() {
        apply_dpad(raw, &mut player_x, &mut player_y);
        if npc.is_some() {
            let next = npc_x as i32 + npc_dir * 2;
            if next < base_npc_x as i32 - 12 || next > base_npc_x as i32 + 12 {
                npc_dir *= -1;
            }
            npc_x = (npc_x as i32 + npc_dir * 2).max(0) as u32;
        }

        let mut entities = vec![FrameEntity {
            entity_id: player.id.clone(),
            x: player_x,
            y: player_y,
            frame: (frame_index % 4) as u8,
        }];
        if let Some(npc) = npc {
            entities.push(FrameEntity {
                entity_id: npc.id.clone(),
                x: npc_x,
                y: npc_y,
                frame: ((frame_index + 1) % 4) as u8,
            });
        }
        let (rgba, width, height) = render_scene_frame(scene, &assets, &resolution, &entities)?;
        let output_file = format!("frame_{frame_index:03}.png");
        write_rgba_preview(&frames_dir.join(&output_file), width, height, &rgba)?;
        frames.push(SimulationFrame {
            frame_index,
            input: raw.to_string(),
            player_x,
            player_y,
            npc_x,
            npc_y,
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
        .find(|scene| scene.kind == "gameplay")
        .or_else(|| content.scenes.first())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no scenes found"))
}

fn apply_dpad(ch: char, x: &mut u32, y: &mut u32) {
    match ch {
        'L' | 'l' => *x = x.saturating_sub(4),
        'R' | 'r' => *x = (*x + 4).min(108),
        'U' | 'u' => *y = y.saturating_sub(4),
        'D' | 'd' => *y = (*y + 4).min(92),
        _ => {}
    }
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
    use super::{apply_dpad, render_simulation_summary, SimulationReport, SimulationFrame};

    #[test]
    fn dpad_moves_player_in_expected_direction() {
        let mut x = 20;
        let mut y = 20;
        apply_dpad('R', &mut x, &mut y);
        apply_dpad('D', &mut x, &mut y);
        assert_eq!((x, y), (24, 24));
        apply_dpad('L', &mut x, &mut y);
        apply_dpad('U', &mut x, &mut y);
        assert_eq!((x, y), (20, 20));
    }

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
