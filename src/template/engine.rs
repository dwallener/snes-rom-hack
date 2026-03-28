use super::assets::{AssetResolution, SceneLoadPackets};
use super::content::CompiledContent;
use super::runtime::RuntimeSkeleton;
use serde::Serialize;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EngineBuildPlan {
    pub boot_scene: String,
    pub joypad_map: Vec<JoypadBinding>,
    pub frame_steps: Vec<&'static str>,
    pub scene_packet_map: Vec<ScenePacketBinding>,
    pub entity_runtime: Vec<EntityRuntimeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JoypadBinding {
    pub button: &'static str,
    pub effect: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ScenePacketBinding {
    pub scene_id: String,
    pub packet_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EntityRuntimeBinding {
    pub entity_id: String,
    pub kind: String,
    pub sprite_page_id: u16,
    pub palette_id: u16,
    pub speed: u16,
    pub movement_rule: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimeEntityState {
    pub entity_id: String,
    pub x: u32,
    pub y: u32,
    pub frame: u8,
    pub facing: i8,
}

pub(crate) fn build_engine_plan(
    content: &CompiledContent,
    resolution: &AssetResolution,
    packets: &SceneLoadPackets,
    _runtime: &RuntimeSkeleton<'_>,
) -> io::Result<EngineBuildPlan> {
    let boot_scene = parse_boot_scene(&content.script.on_boot).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "on_boot script does not encode a load_scene target",
        )
    })?;

    let scene_packet_map = packets
        .packets
        .iter()
        .filter(|packet| packet.scene_id != "__entities__")
        .map(|packet| ScenePacketBinding {
            scene_id: packet.scene_id.clone(),
            packet_file: packet.output_file.clone(),
        })
        .collect::<Vec<_>>();

    let entity_runtime = content
        .entities
        .iter()
        .map(|entity| {
            let resolved = resolution
                .entities
                .iter()
                .find(|item| item.entity_id == entity.id)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("missing resolved entity `{}`", entity.id),
                    )
                })?;
            Ok(EntityRuntimeBinding {
                entity_id: entity.id.clone(),
                kind: entity.kind.clone(),
                sprite_page_id: resolved.sprite_page_id,
                palette_id: resolved.palette_id,
                speed: entity.speed,
                movement_rule: if entity.kind == "player" {
                    "dpad_4way"
                } else {
                    "horizontal_patrol"
                },
            })
        })
        .collect::<io::Result<Vec<_>>>()?;

    Ok(EngineBuildPlan {
        boot_scene,
        joypad_map: vec![
            JoypadBinding {
                button: "left",
                effect: "player.x -= speed",
            },
            JoypadBinding {
                button: "right",
                effect: "player.x += speed",
            },
            JoypadBinding {
                button: "up",
                effect: "player.y -= speed",
            },
            JoypadBinding {
                button: "down",
                effect: "player.y += speed",
            },
        ],
        frame_steps: vec![
            "poll joypad edges and held state",
            "apply player movement from dpad map",
            "step npc patrol movement",
            "load active scene packet if scene changed",
            "compose sprite placements into shadow OAM",
            "flush queued transfers during NMI",
        ],
        scene_packet_map,
        entity_runtime,
    })
}

pub(crate) fn render_engine_build_summary(plan: &EngineBuildPlan) -> String {
    let mut out = String::new();
    out.push_str("Engine Build Plan\n");
    out.push_str(&format!("boot_scene: {}\n\n", plan.boot_scene));
    out.push_str("Joypad Map\n");
    for binding in &plan.joypad_map {
        out.push_str(&format!("- {} => {}\n", binding.button, binding.effect));
    }
    out.push_str("\nScene Packets\n");
    for packet in &plan.scene_packet_map {
        out.push_str(&format!("- {} => {}\n", packet.scene_id, packet.packet_file));
    }
    out.push_str("\nEntities\n");
    for entity in &plan.entity_runtime {
        out.push_str(&format!(
            "- {} kind={} sprite={} palette={} speed={} movement={}\n",
            entity.entity_id,
            entity.kind,
            entity.sprite_page_id,
            entity.palette_id,
            entity.speed,
            entity.movement_rule
        ));
    }
    out.push_str("\nFrame Steps\n");
    for step in &plan.frame_steps {
        out.push_str(&format!("- {}\n", step));
    }
    out
}

pub(crate) fn render_engine_frame_logic(plan: &EngineBuildPlan) -> String {
    let mut out = String::new();
    out.push_str("; Generated engine frame logic\n");
    out.push_str(&format!("; boot_scene = {}\n\n", plan.boot_scene));
    out.push_str("frame_main_loop:\n");
    out.push_str("    ; 1. poll joypad and cache held state\n");
    out.push_str("    jsr read_joypad_state\n");
    out.push_str("    ; 2. apply movement rules\n");
    for binding in &plan.joypad_map {
        out.push_str(&format!("    ; if {} held: {}\n", binding.button, binding.effect));
    }
    out.push_str("    jsr step_player_movement\n");
    out.push_str("    jsr step_npc_movement\n");
    out.push_str("    ; 3. scene packet decode on transition\n");
    for packet in &plan.scene_packet_map {
        out.push_str(&format!(
            "    ; scene {} loads packet {}\n",
            packet.scene_id, packet.packet_file
        ));
    }
    out.push_str("    jsr maybe_load_scene_packet\n");
    out.push_str("    ; 4. compose entity sprites\n");
    for entity in &plan.entity_runtime {
        out.push_str(&format!(
            "    ; entity {} uses sprite {} palette {} rule {}\n",
            entity.entity_id, entity.sprite_page_id, entity.palette_id, entity.movement_rule
        ));
    }
    out.push_str("    jsr compose_entity_sprites\n");
    out.push_str("    rts\n");
    out
}

pub(crate) fn initial_boot_scene(plan: &EngineBuildPlan) -> &str {
    &plan.boot_scene
}

pub(crate) fn apply_input_frame(
    plan: &EngineBuildPlan,
    input: char,
    scene_spawn: (u32, u32),
    states: &mut [RuntimeEntityState],
) {
    for state in states.iter_mut() {
        if let Some(binding) = plan
            .entity_runtime
            .iter()
            .find(|binding| binding.entity_id == state.entity_id)
        {
            match binding.movement_rule {
                "dpad_4way" => apply_player_rule(state, binding.speed, input),
                "horizontal_patrol" => apply_patrol_rule(state, scene_spawn.0 + 32, binding.speed),
                _ => {}
            }
        }
    }
}

fn apply_player_rule(state: &mut RuntimeEntityState, speed: u16, input: char) {
    let speed = u32::from(speed.max(1));
    match input {
        'L' | 'l' => {
            state.x = state.x.saturating_sub(speed * 2);
            state.facing = -1;
        }
        'R' | 'r' => {
            state.x = (state.x + speed * 2).min(108);
            state.facing = 1;
        }
        'U' | 'u' => state.y = state.y.saturating_sub(speed * 2),
        'D' | 'd' => state.y = (state.y + speed * 2).min(92),
        _ => {}
    }
    state.frame = state.frame.wrapping_add(1) % 4;
}

fn apply_patrol_rule(state: &mut RuntimeEntityState, center_x: u32, speed: u16) {
    let speed = i32::from(speed.max(1)) * 2;
    let mut next = state.x as i32 + i32::from(state.facing) * speed;
    let min_x = center_x as i32 - 12;
    let max_x = center_x as i32 + 12;
    if next < min_x || next > max_x {
        state.facing *= -1;
        next = state.x as i32 + i32::from(state.facing) * speed;
    }
    state.x = next.max(0) as u32;
    state.frame = state.frame.wrapping_add(1) % 4;
}

pub(crate) fn runtime_entity_states(
    plan: &EngineBuildPlan,
    player_spawn: (u32, u32),
) -> Vec<RuntimeEntityState> {
    plan.entity_runtime
        .iter()
        .map(|entity| RuntimeEntityState {
            entity_id: entity.entity_id.clone(),
            x: if entity.kind == "player" {
                player_spawn.0
            } else {
                player_spawn.0 + 32
            },
            y: player_spawn.1,
            frame: if entity.kind == "player" { 0 } else { 1 },
            facing: if entity.kind == "player" { 1 } else { 1 },
        })
        .collect()
}

fn parse_boot_scene(raw: &str) -> Option<String> {
    raw.strip_prefix("load_scene ").map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        EngineBuildPlan, EntityRuntimeBinding, apply_input_frame, build_engine_plan,
        render_engine_build_summary, render_engine_frame_logic, runtime_entity_states,
    };
    use crate::template::assets::{AssetResolution, ResolvedEntityAssetRecord, ResolvedRoomAssetRecord, SceneLoadPacket, SceneLoadPackets};
    use crate::template::content::{CompiledContent, EntityDef, SceneDef, ScriptDef};
    use crate::template::runtime::default_runtime_skeleton;
    use crate::template::TemplateKind;

    #[test]
    fn engine_plan_extracts_boot_scene_and_entities() {
        let content = CompiledContent {
            template: TemplateKind::SingleScreenAction,
            game: "demo".to_string(),
            title_scene: "title_room".to_string(),
            scenes: vec![SceneDef {
                id: "room_000".to_string(),
                kind: "gameplay".to_string(),
                background: "bg_main".to_string(),
                palette: "default".to_string(),
                music: "stage_01".to_string(),
                player_spawn: "8,8".to_string(),
                enemy_set: "room_000_enemies".to_string(),
                next_scene: "room_001".to_string(),
                source_file: "room_000.toml".to_string(),
            }],
            entities: vec![EntityDef {
                id: "player".to_string(),
                kind: "player".to_string(),
                sprite_page: "ball_player".to_string(),
                palette: "player_ball".to_string(),
                hitbox: "8,8,16,16".to_string(),
                speed: 2,
                jump: 4,
                attack: "basic".to_string(),
                source_file: "player.toml".to_string(),
            }],
            script: ScriptDef {
                on_boot: "load_scene title_room".to_string(),
                on_game_over: "load_scene title_room".to_string(),
                on_room_clear: "load_scene room_001".to_string(),
                source_file: "main.toml".to_string(),
            },
        };
        let resolution = AssetResolution {
            rooms: vec![ResolvedRoomAssetRecord {
                scene_id: "room_000".to_string(),
                background_id: 0,
                palette_id: 0,
                music_id: 0,
                next_scene: "room_001".to_string(),
                background_vram_slot: "bg_tiles".to_string(),
            }],
            entities: vec![ResolvedEntityAssetRecord {
                entity_id: "player".to_string(),
                sprite_page_id: 1,
                sprite_vram_slot: "sprite_tiles".to_string(),
                palette_id: 2,
            }],
        };
        let packets = SceneLoadPackets {
            packets: vec![SceneLoadPacket {
                scene_id: "room_000".to_string(),
                output_file: "scene_00_room_000.bin".to_string(),
                commands: vec![],
            }],
        };
        let runtime = default_runtime_skeleton(TemplateKind::SingleScreenAction);
        let plan = build_engine_plan(&content, &resolution, &packets, &runtime).expect("plan");
        assert_eq!(plan.boot_scene, "title_room");
        assert_eq!(plan.entity_runtime.len(), 1);
        let summary = render_engine_build_summary(&plan);
        assert!(summary.contains("boot_scene: title_room"));
        let asm = render_engine_frame_logic(&plan);
        assert!(asm.contains("frame_main_loop"));
    }

    #[test]
    fn applies_generated_player_and_npc_rules() {
        let plan = EngineBuildPlan {
            boot_scene: "title_room".to_string(),
            joypad_map: vec![],
            frame_steps: vec![],
            scene_packet_map: vec![],
            entity_runtime: vec![
                EntityRuntimeBinding {
                    entity_id: "player".to_string(),
                    kind: "player".to_string(),
                    sprite_page_id: 1,
                    palette_id: 2,
                    speed: 2,
                    movement_rule: "dpad_4way",
                },
                EntityRuntimeBinding {
                    entity_id: "npc_ball".to_string(),
                    kind: "npc".to_string(),
                    sprite_page_id: 0,
                    palette_id: 1,
                    speed: 1,
                    movement_rule: "horizontal_patrol",
                },
            ],
        };
        let mut states = runtime_entity_states(&plan, (32, 32));
        apply_input_frame(&plan, 'R', (32, 32), &mut states);
        let player = states.iter().find(|s| s.entity_id == "player").unwrap();
        let npc = states.iter().find(|s| s.entity_id == "npc_ball").unwrap();
        assert_eq!(player.x, 36);
        assert_ne!(npc.x, 64);
    }
}
