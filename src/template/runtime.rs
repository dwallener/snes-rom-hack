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
    pub runtime_state: Vec<StateField<'a>>,
    pub joypad: JoypadPlan<'a>,
    pub dma_descriptors: Vec<DmaDescriptor<'a>>,
    pub room_loader: RoomLoaderPlan<'a>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StateField<'a> {
    pub name: &'a str,
    pub addr: &'a str,
    pub width: &'a str,
    pub owner: &'a str,
    pub purpose: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JoypadPlan<'a> {
    pub latch_phase: &'a str,
    pub current_state_addr: &'a str,
    pub previous_state_addr: &'a str,
    pub pressed_edge_addr: &'a str,
    pub released_edge_addr: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DmaDescriptor<'a> {
    pub queue_addr: &'a str,
    pub entry_size: u8,
    pub fields: Vec<StateField<'a>>,
    pub flush_phase: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RoomLoaderPlan<'a> {
    pub room_table_addr: &'a str,
    pub title_room_id: &'a str,
    pub startup_sequence: Vec<&'a str>,
    pub asset_steps: Vec<&'a str>,
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
            runtime_state: vec![
                StateField {
                    name: "frame_counter",
                    addr: "$7E:0000",
                    width: "u16",
                    owner: "nmi",
                    purpose: "increments once per completed frame",
                },
                StateField {
                    name: "scene_id",
                    addr: "$7E:0200",
                    width: "u8",
                    owner: "scene",
                    purpose: "active room or title scene identifier",
                },
                StateField {
                    name: "next_scene_id",
                    addr: "$7E:0201",
                    width: "u8",
                    owner: "scene",
                    purpose: "pending room transition target",
                },
                StateField {
                    name: "player_state_base",
                    addr: "$7E:0600",
                    width: "struct",
                    owner: "entity",
                    purpose: "player position, velocity, facing, animation, health",
                },
                StateField {
                    name: "oam_count",
                    addr: "$7E:1200",
                    width: "u8",
                    owner: "render",
                    purpose: "number of composed sprites in shadow OAM",
                },
            ],
            joypad: JoypadPlan {
                latch_phase: "nmi",
                current_state_addr: "$7E:0002",
                previous_state_addr: "$7E:0004",
                pressed_edge_addr: "$7E:0006",
                released_edge_addr: "$7E:0008",
            },
            dma_descriptors: vec![
                DmaDescriptor {
                    queue_addr: "$7E:0100",
                    entry_size: 8,
                    fields: vec![
                        StateField {
                            name: "kind",
                            addr: "+0",
                            width: "u8",
                            owner: "render/scene",
                            purpose: "vram | cgram | oam",
                        },
                        StateField {
                            name: "source_addr",
                            addr: "+1",
                            width: "u24",
                            owner: "render/scene",
                            purpose: "WRAM or ROM source pointer",
                        },
                        StateField {
                            name: "dest_addr",
                            addr: "+4",
                            width: "u16",
                            owner: "render/scene",
                            purpose: "VRAM/CGRAM/OAM destination offset",
                        },
                        StateField {
                            name: "size_bytes",
                            addr: "+6",
                            width: "u16",
                            owner: "render/scene",
                            purpose: "transfer size capped by DMA budget",
                        },
                    ],
                    flush_phase: "nmi",
                },
            ],
            room_loader: RoomLoaderPlan {
                room_table_addr: "$84:8000",
                title_room_id: "0",
                startup_sequence: vec![
                    "cold_boot clears WRAM state and installs blank palettes",
                    "load_scene(title_room_id)",
                    "scene loader stages background tiles, tilemap, palette, and HUD assets",
                    "nmi flushes queued transfers before enabling gameplay loop",
                ],
                asset_steps: vec![
                    "lookup room header in room table",
                    "queue background tile DMA into bg_tiles",
                    "queue tilemap DMA into bg_map",
                    "queue palette DMA into cgram",
                    "install player/enemy sprite page ids for render module",
                ],
            },
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
            runtime_state: vec![],
            joypad: JoypadPlan {
                latch_phase: "nmi",
                current_state_addr: "$7E:0000",
                previous_state_addr: "$7E:0002",
                pressed_edge_addr: "$7E:0004",
                released_edge_addr: "$7E:0006",
            },
            dma_descriptors: vec![],
            room_loader: RoomLoaderPlan {
                room_table_addr: "$84:8000",
                title_room_id: "0",
                startup_sequence: vec!["template-defined startup flow"],
                asset_steps: vec!["template-defined asset staging"],
            },
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

    out.push_str("\nRuntime State\n");
    for field in &runtime.runtime_state {
        out.push_str(&format!(
            "- {} {} {} {} {}\n",
            field.name, field.addr, field.width, field.owner, field.purpose
        ));
    }

    out.push_str("\nJoypad\n");
    out.push_str(&format!(
        "- latch_phase: {}\n- current_state: {}\n- previous_state: {}\n- pressed_edge: {}\n- released_edge: {}\n",
        runtime.joypad.latch_phase,
        runtime.joypad.current_state_addr,
        runtime.joypad.previous_state_addr,
        runtime.joypad.pressed_edge_addr,
        runtime.joypad.released_edge_addr
    ));

    out.push_str("\nDMA Queue\n");
    for descriptor in &runtime.dma_descriptors {
        out.push_str(&format!(
            "- queue {} entry_size={} flush_phase={}\n",
            descriptor.queue_addr, descriptor.entry_size, descriptor.flush_phase
        ));
        for field in &descriptor.fields {
            out.push_str(&format!(
                "  - {} {} {} {}\n",
                field.name, field.addr, field.width, field.purpose
            ));
        }
    }

    out.push_str("\nRoom Loader\n");
    out.push_str(&format!(
        "- room_table: {}\n- title_room_id: {}\n",
        runtime.room_loader.room_table_addr, runtime.room_loader.title_room_id
    ));
    out.push_str("  startup_sequence:\n");
    for step in &runtime.room_loader.startup_sequence {
        out.push_str(&format!("  - {}\n", step));
    }
    out.push_str("  asset_steps:\n");
    for step in &runtime.room_loader.asset_steps {
        out.push_str(&format!("  - {}\n", step));
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
            out.push_str(&format!("{entrypoint}:\n"));
            for line in stub_lines(entrypoint) {
                out.push_str(&format!("    ; {line}\n"));
            }
            out.push_str("    rts\n\n");
        }
    }
    out.push_str("; Runtime state anchors\n");
    for field in &runtime.runtime_state {
        out.push_str(&format!(
            "; state {} {} {} {}\n",
            field.name, field.addr, field.width, field.purpose
        ));
    }
    out.push_str("\n; Joypad state\n");
    out.push_str(&format!(
        "; current={} previous={} pressed={} released={}\n",
        runtime.joypad.current_state_addr,
        runtime.joypad.previous_state_addr,
        runtime.joypad.pressed_edge_addr,
        runtime.joypad.released_edge_addr
    ));
    out.push_str("; DMA queue descriptors\n");
    for descriptor in &runtime.dma_descriptors {
        out.push_str(&format!(
            "; queue {} entry_size={} flush={}\n",
            descriptor.queue_addr, descriptor.entry_size, descriptor.flush_phase
        ));
    }
    out
}

fn stub_lines(entrypoint: &str) -> &'static [&'static str] {
    match entrypoint {
        "reset" => &[
            "disable interrupts and switch to native mode",
            "initialize stack, direct page, and bank registers",
            "jump to cold_boot",
        ],
        "cold_boot" => &[
            "clear WRAM state regions",
            "initialize PPU to forced blank",
            "prepare title room load and seed DMA queue",
        ],
        "nmi_entry" => &[
            "save CPU state",
            "latch joypad and derive pressed/released edges",
            "flush pending DMA queue entries within frame budget",
            "publish frame counter and restore CPU state",
        ],
        "flush_dma_queue" => &[
            "walk queue descriptors at $7E:0100",
            "dispatch VRAM/CGRAM/OAM transfers by descriptor kind",
            "stop when budget or queue end is reached",
        ],
        "load_scene" => &[
            "lookup room header and asset list",
            "reset room-local entity state",
            "queue background, tilemap, and palette uploads",
        ],
        "apply_room_assets" => &[
            "assign VRAM slots for room assets",
            "queue sprite page ids for player and enemies",
        ],
        "enter_room" => &[
            "set active scene id and transition state",
            "arm gameplay loop for next frame",
        ],
        "update_entities" => &[
            "advance player and enemy state machines",
            "resolve tile and sprite collisions",
        ],
        "update_player" => &[
            "consume joypad edges and held inputs",
            "apply movement, jump, and attack rules",
        ],
        "update_enemy" => &[
            "step simple room-local AI",
            "queue deaths, hits, and pickups",
        ],
        "compose_oam" => &[
            "convert active entities into metasprite entries",
            "write shadow OAM buffer and sprite count",
        ],
        "compose_hud" => &[
            "format score, lives, and room status tiles",
            "queue HUD tile changes if needed",
        ],
        "queue_music" => &[
            "emit track-change command if scene requests it",
            "avoid duplicate track restarts",
        ],
        "queue_sfx" => &[
            "pack one-shot effect ids for APU handoff",
            "coalesce repeated effects if channel budget is low",
        ],
        _ => &["TODO"],
    }
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
        assert!(text.contains("latch joypad and derive pressed/released edges"));
    }
}
