use super::{TemplateKind, template_kind_name};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimeSkeleton<'a> {
    pub template: TemplateKind,
    pub status: &'a str,
    pub engine_modules: Vec<RuntimeModule<'a>>,
    pub wram_regions: Vec<MemorySlice<'a>>,
    pub vram_slots: Vec<VideoSlot<'a>>,
    pub frame_schedule: Vec<FramePhase<'a>>,
    pub scene_flow: Vec<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RuntimeModule<'a> {
    pub name: &'a str,
    pub bank_region: &'a str,
    pub responsibility: &'a str,
    pub entrypoints: Vec<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MemorySlice<'a> {
    pub name: &'a str,
    pub start: &'a str,
    pub end: &'a str,
    pub owner: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct VideoSlot<'a> {
    pub name: &'a str,
    pub target: &'a str,
    pub start: &'a str,
    pub end: &'a str,
    pub usage: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FramePhase<'a> {
    pub phase: &'a str,
    pub responsibilities: Vec<&'a str>,
}

pub(crate) fn default_runtime_skeleton(template: TemplateKind) -> RuntimeSkeleton<'static> {
    match template {
        TemplateKind::SingleScreenAction => RuntimeSkeleton {
            template,
            status: "layout-defined",
            engine_modules: vec![
                RuntimeModule {
                    name: "boot",
                    bank_region: "$80:8000-$80:87FF",
                    responsibility: "reset vector, CPU/PPU/APU init, initial scene boot",
                    entrypoints: vec!["reset", "cold_boot"],
                },
                RuntimeModule {
                    name: "nmi",
                    bank_region: "$80:8800-$80:8BFF",
                    responsibility: "joypad latch, DMA queue flush, OAM/CGRAM/VRAM commit, frame counters",
                    entrypoints: vec!["nmi_entry", "flush_dma_queue"],
                },
                RuntimeModule {
                    name: "scene",
                    bank_region: "$80:8C00-$80:97FF",
                    responsibility: "room load, tilemap decode, asset list install, room transitions",
                    entrypoints: vec!["load_scene", "apply_room_assets", "enter_room"],
                },
                RuntimeModule {
                    name: "entity",
                    bank_region: "$80:9800-$80:A3FF",
                    responsibility: "player/enemy update loop, collision checks, attack resolution",
                    entrypoints: vec!["update_entities", "update_player", "update_enemy"],
                },
                RuntimeModule {
                    name: "render",
                    bank_region: "$80:A400-$80:ABFF",
                    responsibility: "metasprite composition, HUD composition, OAM staging",
                    entrypoints: vec!["compose_oam", "compose_hud"],
                },
                RuntimeModule {
                    name: "audio",
                    bank_region: "$80:AC00-$80:AFFF",
                    responsibility: "music and sfx command queueing to APU",
                    entrypoints: vec!["queue_music", "queue_sfx"],
                },
            ],
            wram_regions: vec![
                MemorySlice {
                    name: "zero_page_state",
                    start: "$7E:0000",
                    end: "$7E:00FF",
                    owner: "fast state, pointers, frame counters",
                },
                MemorySlice {
                    name: "dma_queue",
                    start: "$7E:0100",
                    end: "$7E:01FF",
                    owner: "VRAM/CGRAM/OAM upload descriptors",
                },
                MemorySlice {
                    name: "scene_state",
                    start: "$7E:0200",
                    end: "$7E:05FF",
                    owner: "room id, transitions, tile and palette state",
                },
                MemorySlice {
                    name: "entity_state",
                    start: "$7E:0600",
                    end: "$7E:11FF",
                    owner: "player/enemy state tables and collision scratch",
                },
                MemorySlice {
                    name: "oam_staging",
                    start: "$7E:1200",
                    end: "$7E:143F",
                    owner: "shadow OAM buffer",
                },
                MemorySlice {
                    name: "asset_staging",
                    start: "$7F:8000",
                    end: "$7F:BFFF",
                    owner: "room graphics/tile staging before DMA",
                },
            ],
            vram_slots: vec![
                VideoSlot {
                    name: "bg_tiles",
                    target: "vram",
                    start: "$0000",
                    end: "$2FFF",
                    usage: "static room background tiles",
                },
                VideoSlot {
                    name: "bg_map",
                    target: "vram",
                    start: "$3000",
                    end: "$37FF",
                    usage: "single room tilemap",
                },
                VideoSlot {
                    name: "sprite_tiles",
                    target: "vram",
                    start: "$4000",
                    end: "$67FF",
                    usage: "player, enemy, pickup, and FX sprites",
                },
                VideoSlot {
                    name: "hud_tiles",
                    target: "vram",
                    start: "$6800",
                    end: "$6FFF",
                    usage: "score, lives, and HUD overlays",
                },
            ],
            frame_schedule: vec![
                FramePhase {
                    phase: "main_loop",
                    responsibilities: vec![
                        "poll input snapshot from previous NMI",
                        "update scene and entities",
                        "enqueue sprite and background changes",
                        "queue audio commands",
                    ],
                },
                FramePhase {
                    phase: "nmi",
                    responsibilities: vec![
                        "latch joypad",
                        "flush VRAM queue within DMA budget",
                        "flush CGRAM and OAM queues",
                        "publish frame-complete counters",
                    ],
                },
            ],
            scene_flow: vec![
                "boot -> title",
                "title -> room_load",
                "room_load -> gameplay",
                "gameplay -> room_clear | player_death | pause",
                "player_death -> title | respawn",
            ],
        },
        other => RuntimeSkeleton {
            template: other,
            status: "layout-defined",
            engine_modules: vec![RuntimeModule {
                name: "boot",
                bank_region: "$80:8000-$80:87FF",
                responsibility: "reset vector and template runtime bootstrap",
                entrypoints: vec!["reset", "cold_boot"],
            }],
            wram_regions: vec![MemorySlice {
                name: "template_state",
                start: "$7E:0000",
                end: "$7E:1FFF",
                owner: "template-defined runtime state",
            }],
            vram_slots: vec![VideoSlot {
                name: "template_vram",
                target: "vram",
                start: "$0000",
                end: "$7FFF",
                usage: "template-managed graphics layout",
            }],
            frame_schedule: vec![FramePhase {
                phase: "main_loop",
                responsibilities: vec!["template-defined update and render staging"],
            }],
            scene_flow: vec![
                "boot -> title",
                "title -> gameplay",
                "gameplay -> template-defined transitions",
            ],
        },
    }
}

pub(crate) fn render_runtime_summary(runtime: &RuntimeSkeleton<'_>) -> String {
    let mut out = String::new();
    out.push_str("Template Runtime Layout\n");
    out.push_str(&format!("template: {}\n", template_kind_name(runtime.template)));
    out.push_str(&format!("status: {}\n\n", runtime.status));

    out.push_str("Engine Modules\n");
    for module in &runtime.engine_modules {
        out.push_str(&format!(
            "- {} [{}] {}\n",
            module.name, module.bank_region, module.responsibility
        ));
        out.push_str(&format!("  entrypoints: {}\n", module.entrypoints.join(", ")));
    }

    out.push_str("\nWRAM Regions\n");
    for region in &runtime.wram_regions {
        out.push_str(&format!(
            "- {} {}..{} {}\n",
            region.name, region.start, region.end, region.owner
        ));
    }

    out.push_str("\nVRAM Slots\n");
    for slot in &runtime.vram_slots {
        out.push_str(&format!(
            "- {} {} {}..{} {}\n",
            slot.name, slot.target, slot.start, slot.end, slot.usage
        ));
    }

    out.push_str("\nFrame Schedule\n");
    for phase in &runtime.frame_schedule {
        out.push_str(&format!("- {}\n", phase.phase));
        for responsibility in &phase.responsibilities {
            out.push_str(&format!("  - {}\n", responsibility));
        }
    }

    out.push_str("\nScene Flow\n");
    for flow in &runtime.scene_flow {
        out.push_str(&format!("- {}\n", flow));
    }

    out
}

pub(crate) fn render_engine_stub(runtime: &RuntimeSkeleton<'_>) -> String {
    let mut out = String::new();
    out.push_str("; Template runtime stub\n");
    out.push_str(&format!(
        "; template = {}\n; status = {}\n\n",
        template_kind_name(runtime.template),
        runtime.status
    ));
    out.push_str("; Planned engine modules\n");
    for module in &runtime.engine_modules {
        out.push_str(&format!(
            "; module {} [{}] {}\n",
            module.name, module.bank_region, module.responsibility
        ));
        for entrypoint in &module.entrypoints {
            out.push_str(&format!("{entrypoint}:\n    ; TODO\n    rts\n\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{default_runtime_skeleton, render_engine_stub, render_runtime_summary};
    use crate::template::TemplateKind;

    #[test]
    fn single_screen_runtime_lists_nmi_and_scene_modules() {
        let runtime = default_runtime_skeleton(TemplateKind::SingleScreenAction);
        let text = render_runtime_summary(&runtime);
        assert!(text.contains("nmi"));
        assert!(text.contains("scene"));
        assert!(text.contains("asset_staging"));
    }

    #[test]
    fn engine_stub_includes_reset_and_nmi_entrypoints() {
        let runtime = default_runtime_skeleton(TemplateKind::SingleScreenAction);
        let text = render_engine_stub(&runtime);
        assert!(text.contains("reset:"));
        assert!(text.contains("nmi_entry:"));
    }
}
