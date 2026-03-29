use super::assets::{AssetBundle, AssetResolution, PaletteAsset};
use super::content::CompiledContent;
use super::engine::EngineBuildPlan;
use super::{GameManifest, TemplateKind};
use crate::rommap;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ROM_SIZE: usize = 0x10000;
const ROM_BANK: u8 = 0x80;

#[derive(Debug, Clone)]
pub(crate) struct RomArtifact {
    pub rom_path: PathBuf,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RomSpec {
    title: String,
    boot_scene_id: String,
    gameplay_scene_id: String,
    backdrop_rgb: [u8; 3],
    player_spawn: (u8, u8),
    npc_spawn: (u8, u8),
    player_speed: u8,
    npc_speed: u8,
    has_npc: bool,
}

pub(crate) fn build_bootable_rom(
    out_dir: &Path,
    manifest: &GameManifest,
    content: &CompiledContent,
    assets: &AssetBundle,
    _resolution: &AssetResolution,
    engine_plan: &EngineBuildPlan,
) -> io::Result<RomArtifact> {
    if manifest.template != TemplateKind::SingleScreenAction {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "bootable ROM emission currently supports only `single-screen-action`",
        ));
    }

    let spec = build_rom_spec(manifest, content, assets, engine_plan)?;
    let bytes = emit_rom_image(&spec, assets, content)?;
    let rom_path = out_dir.join(format!("{}.sfc", manifest.name));
    fs::write(&rom_path, bytes)?;
    Ok(RomArtifact {
        rom_path,
        summary: render_rom_summary(&spec, manifest),
    })
}

fn build_rom_spec(
    manifest: &GameManifest,
    content: &CompiledContent,
    assets: &AssetBundle,
    engine_plan: &EngineBuildPlan,
) -> io::Result<RomSpec> {
    let gameplay_scene = content
        .scenes
        .iter()
        .find(|scene| scene.kind == "gameplay")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing gameplay scene"))?;
    let boot_scene_id = if content
        .scenes
        .iter()
        .any(|scene| scene.id == engine_plan.boot_scene && scene.kind == "gameplay")
    {
        engine_plan.boot_scene.clone()
    } else {
        gameplay_scene.id.clone()
    };
    let boot_scene = content
        .scenes
        .iter()
        .find(|scene| scene.id == boot_scene_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing selected boot scene"))?;
    let player = content
        .entities
        .iter()
        .find(|entity| entity.kind == "player")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing player entity"))?;
    let npc = content.entities.iter().find(|entity| entity.kind == "npc");
    let background_palette = assets
        .palettes
        .iter()
        .find(|palette| palette.name == boot_scene.palette)
        .or_else(|| assets.palettes.iter().find(|palette| palette.name == "default"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing default palette asset"))?;
    let backdrop_rgb = palette_rgb(&background_palette.preset, 1);
    let player_spawn = parse_spawn(&boot_scene.player_spawn);
    let npc_spawn = (
        player_spawn.0.saturating_add(32).min(232),
        player_spawn.1,
    );

    Ok(RomSpec {
        title: truncate_title(&manifest.title),
        boot_scene_id,
        gameplay_scene_id: gameplay_scene.id.clone(),
        backdrop_rgb,
        player_spawn,
        npc_spawn,
        player_speed: player.speed.max(1).min(4) as u8,
        npc_speed: npc.map(|entity| entity.speed.max(1).min(3) as u8).unwrap_or(0),
        has_npc: npc.is_some(),
    })
}

fn emit_rom_image(spec: &RomSpec, assets: &AssetBundle, content: &CompiledContent) -> io::Result<Vec<u8>> {
    let player = content
        .entities
        .iter()
        .find(|entity| entity.kind == "player")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing player entity"))?;
    let player_palette = assets
        .palettes
        .iter()
        .find(|palette| palette.name == player.palette)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing player palette"))?;
    let npc_palette = content
        .entities
        .iter()
        .find(|entity| entity.kind == "npc")
        .and_then(|entity| assets.palettes.iter().find(|palette| palette.name == entity.palette))
        .cloned()
        .unwrap_or_else(|| player_palette.clone());

    let backdrop = rgb_to_bgr555(spec.backdrop_rgb);
    let player_palette_words = build_obj_palette_words(player_palette);
    let npc_palette_words = build_obj_palette_words(&npc_palette);
    let tile_words = build_ball_tiles_4bpp();

    let mut asm = RomAssembler::new();

    asm.label("reset");
    asm.byte(0x78);
    asm.byte(0xD8);
    asm.byte(0xA2);
    asm.byte(0xFF);
    asm.byte(0x9A);
    asm.stz_abs(0x4200);
    asm.lda_imm(0x80);
    asm.sta_abs(0x2100);
    asm.stz_abs(0x420C);
    asm.jsr("init_state");
    asm.jsr("upload_video");
    asm.jsr("clear_oam_direct");
    asm.jsr("upload_oam_direct");
    asm.jsr("init_ppu");
    asm.label("main_loop");
    asm.jsr("wait_vblank_start");
    asm.jsr("update_state");
    asm.jsr("upload_oam_direct");
    asm.jsr("wait_vblank_end");
    asm.jmp("main_loop");

    asm.label("init_state");
    asm.lda_imm(spec.player_spawn.0);
    asm.sta_abs(0x0002);
    asm.lda_imm(spec.player_spawn.1);
    asm.sta_abs(0x0003);
    asm.lda_imm(spec.npc_spawn.0);
    asm.sta_abs(0x0004);
    asm.lda_imm(spec.npc_spawn.1);
    asm.sta_abs(0x0005);
    asm.stz_abs(0x0006);
    asm.stz_abs(0x0007);
    asm.stz_abs(0x0008);
    asm.stz_abs(0x000C);
    asm.stz_abs(0x000D);
    asm.stz_abs(0x000E);
    asm.byte(0x60);

    asm.label("clear_oam_direct");
    asm.stz_abs(0x2102);
    asm.stz_abs(0x2103);
    asm.ldx_imm(0x80);
    asm.label("clear_oam_loop");
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.lda_imm(0xF0);
    asm.sta_abs(0x2104);
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.sta_abs(0x2104);
    asm.dex();
    asm.bne("clear_oam_loop");
    asm.ldx_imm(0x20);
    asm.label("clear_oam_hi_loop");
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.dex();
    asm.bne("clear_oam_hi_loop");
    asm.byte(0x60);

    asm.label("upload_video");
    asm.stz_abs(0x2121);
    asm.lda_imm((backdrop & 0xFF) as u8);
    asm.sta_abs(0x2122);
    asm.lda_imm((backdrop >> 8) as u8);
    asm.sta_abs(0x2122);
    dma_upload_cgram(&mut asm, 0x80, "player_palette", 0x20);
    dma_upload_cgram(&mut asm, 0x90, "npc_palette", 0x20);
    asm.lda_imm(0x80);
    asm.sta_abs(0x2115);
    asm.stz_abs(0x2116);
    asm.stz_abs(0x2117);
    asm.lda_imm(0x01);
    asm.sta_abs(0x4300);
    asm.lda_imm(0x18);
    asm.sta_abs(0x4301);
    asm.lda_label_lo("sprite_tiles");
    asm.sta_abs(0x4302);
    asm.lda_label_hi("sprite_tiles");
    asm.sta_abs(0x4303);
    asm.lda_imm(ROM_BANK);
    asm.sta_abs(0x4304);
    asm.lda_imm(tile_words.len() as u8);
    asm.sta_abs(0x4305);
    asm.stz_abs(0x4306);
    asm.lda_imm(0x01);
    asm.sta_abs(0x420B);
    asm.byte(0x60);

    asm.label("init_ppu");
    asm.stz_abs(0x2101);
    asm.stz_abs(0x2102);
    asm.stz_abs(0x2103);
    asm.stz_abs(0x2105);
    asm.lda_imm(0x10);
    asm.sta_abs(0x212C);
    asm.lda_imm(0x0F);
    asm.sta_abs(0x2100);
    asm.lda_imm(0x01);
    asm.sta_abs(0x4200);
    asm.byte(0x60);

    asm.label("wait_vblank_start");
    asm.label("wait_vblank_start_loop");
    asm.lda_abs(0x4212);
    asm.byte(0x10);
    asm.patch_rel8("wait_vblank_start_loop");
    asm.byte(0x60);

    asm.label("wait_vblank_end");
    asm.label("wait_vblank_end_loop");
    asm.lda_abs(0x4212);
    asm.byte(0x30);
    asm.patch_rel8("wait_vblank_end_loop");
    asm.byte(0x60);

    asm.label("update_state");
    asm.lda_abs(0x4218);
    asm.sta_abs(0x0009);
    asm.lda_abs(0x4219);
    asm.sta_abs(0x000B);
    asm.inc_abs(0x0007);
    apply_player_axis(&mut asm, 0x80, 0x0002, spec.player_speed, 232, true, 0, "skip_right");
    apply_player_axis(&mut asm, 0x40, 0x0002, spec.player_speed, 2, false, 1, "skip_left");
    apply_player_axis(&mut asm, 0x10, 0x0003, spec.player_speed, 2, false, 2, "skip_up");
    apply_player_axis(&mut asm, 0x20, 0x0003, spec.player_speed, 216, true, 3, "skip_down");
    apply_player_axis_alt(&mut asm, 0x01, 0x0002, spec.player_speed, 232, true, 0, "skip_right_alt");
    apply_player_axis_alt(&mut asm, 0x02, 0x0002, spec.player_speed, 2, false, 1, "skip_left_alt");
    apply_player_axis_alt(&mut asm, 0x08, 0x0003, spec.player_speed, 2, false, 2, "skip_up_alt");
    apply_player_axis_alt(&mut asm, 0x04, 0x0003, spec.player_speed, 216, true, 3, "skip_down_alt");
    asm.lda_abs(0x000C);
    asm.beq("skip_attack_cooldown");
    asm.dec_abs(0x000C);
    asm.label("skip_attack_cooldown");
    asm.lda_abs(0x000D);
    asm.beq("skip_attack_timer");
    asm.dec_abs(0x000D);
    asm.label("skip_attack_timer");
    asm.lda_abs(0x0008);
    asm.bne("skip_attack_and_npc");
    asm.lda_abs(0x000B);
    asm.and_imm(0x80);
    asm.beq("skip_attack_press");
    asm.label("attack_pressed");
    asm.lda_abs(0x000C);
    asm.bne("skip_attack_press");
    asm.lda_imm(8);
    asm.sta_abs(0x000C);
    asm.lda_imm(4);
    asm.sta_abs(0x000D);
    asm.label("skip_attack_press");
    asm.lda_abs(0x000D);
    asm.beq("skip_attack_hit");
    asm.jsr("try_hit_npc");
    asm.label("skip_attack_hit");
    if spec.has_npc {
        asm.lda_abs(0x0008);
        asm.bne("skip_attack_and_npc");
        asm.lda_abs(0x0006);
        asm.bne("npc_move_left");
        asm.lda_abs(0x0004);
        asm.cmp_imm(208);
        asm.bcs("npc_flip_left");
        asm.clc();
        asm.adc_imm(spec.npc_speed.max(1));
        asm.sta_abs(0x0004);
        asm.bra("npc_done");
        asm.label("npc_flip_left");
        asm.lda_imm(1);
        asm.sta_abs(0x0006);
        asm.bra("npc_done");
        asm.label("npc_move_left");
        asm.lda_abs(0x0004);
        asm.cmp_imm(32);
        asm.bcc("npc_flip_right");
        asm.sec();
        asm.sbc_imm(spec.npc_speed.max(1));
        asm.sta_abs(0x0004);
        asm.bra("npc_done");
        asm.label("npc_flip_right");
        asm.stz_abs(0x0006);
        asm.label("npc_done");
    }
    asm.label("skip_attack_and_npc");
    asm.byte(0x60);

    asm.label("upload_oam_direct");
    asm.lda_abs(0x0007);
    asm.byte(0x4A);
    asm.byte(0x4A);
    asm.byte(0x4A);
    asm.and_imm(0x03);
    asm.sta_abs(0x000A);
    asm.stz_abs(0x2102);
    asm.stz_abs(0x2103);
    asm.lda_abs(0x0002);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x0003);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x000A);
    asm.sta_abs(0x2104);
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    if spec.has_npc {
        asm.lda_abs(0x0008);
        asm.bne("hide_npc_sprite");
        asm.lda_abs(0x0004);
        asm.sta_abs(0x2104);
        asm.lda_abs(0x0005);
        asm.sta_abs(0x2104);
        asm.lda_abs(0x000A);
        asm.sta_abs(0x2104);
        asm.lda_imm(0x02);
        asm.sta_abs(0x2104);
        asm.bra("npc_sprite_done");
        asm.label("hide_npc_sprite");
        emit_hidden_sprite(&mut asm);
        asm.label("npc_sprite_done");
    }
    asm.lda_abs(0x000D);
    asm.beq("hide_attack_sprite");
    asm.lda_abs(0x000E);
    asm.cmp_imm(0);
    asm.beq("attack_face_right");
    asm.cmp_imm(1);
    asm.beq("attack_face_left");
    asm.cmp_imm(2);
    asm.beq("attack_face_up");
    asm.lda_abs(0x0002);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x0003);
    asm.clc();
    asm.adc_imm(12);
    asm.sta_abs(0x2104);
    asm.bra("attack_sprite_tile");
    asm.label("attack_face_up");
    asm.lda_abs(0x0002);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x0003);
    asm.sec();
    asm.sbc_imm(12);
    asm.sta_abs(0x2104);
    asm.bra("attack_sprite_tile");
    asm.label("attack_face_left");
    asm.lda_abs(0x0002);
    asm.sec();
    asm.sbc_imm(12);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x0003);
    asm.sta_abs(0x2104);
    asm.bra("attack_sprite_tile");
    asm.label("attack_face_right");
    asm.lda_abs(0x0002);
    asm.clc();
    asm.adc_imm(12);
    asm.sta_abs(0x2104);
    asm.lda_abs(0x0003);
    asm.sta_abs(0x2104);
    asm.label("attack_sprite_tile");
    asm.lda_imm(0x03);
    asm.sta_abs(0x2104);
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.bra("attack_sprite_done");
    asm.label("hide_attack_sprite");
    emit_hidden_sprite(&mut asm);
    asm.label("attack_sprite_done");
    asm.byte(0x60);

    asm.label("try_hit_npc");
    asm.lda_abs(0x0002);
    asm.sec();
    asm.sbc_abs(0x0004);
    asm.bpl("hit_x_abs");
    asm.eor_imm(0xFF);
    asm.clc();
    asm.adc_imm(1);
    asm.label("hit_x_abs");
    asm.cmp_imm(16);
    asm.bcs("no_npc_hit");
    asm.lda_abs(0x0003);
    asm.sec();
    asm.sbc_abs(0x0005);
    asm.bpl("hit_y_abs");
    asm.eor_imm(0xFF);
    asm.clc();
    asm.adc_imm(1);
    asm.label("hit_y_abs");
    asm.cmp_imm(16);
    asm.bcs("no_npc_hit");
    asm.lda_imm(1);
    asm.sta_abs(0x0008);
    asm.lda_imm(0xF0);
    asm.sta_abs(0x0005);
    asm.label("no_npc_hit");
    asm.byte(0x60);

    asm.label("irq");
    asm.byte(0x40);

    asm.align(16);
    asm.label("player_palette");
    asm.words(&player_palette_words);
    asm.label("npc_palette");
    asm.words(&npc_palette_words);
    asm.label("sprite_tiles");
    asm.bytes(&tile_words);

    let nmi_vector = asm.lookup("irq")?;
    let reset_vector = asm.lookup("reset")?;
    let irq_vector = asm.lookup("irq")?;
    let code = asm.finalize()?;
    let mut rom = vec![0u8; ROM_SIZE];
    if code.len() > 0x7FC0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated template ROM exceeds 32KB LoROM budget",
        ));
    }
    rom[..code.len()].copy_from_slice(&code);
    write_header(&mut rom, spec, nmi_vector, reset_vector, irq_vector)?;
    let (checksum, complement) = compute_checksum(&rom);
    rom[0x7FDC..0x7FDE].copy_from_slice(&complement.to_le_bytes());
    rom[0x7FDE..0x7FE0].copy_from_slice(&checksum.to_le_bytes());
    let temp_path = std::env::temp_dir().join("template-rom-verify.sfc");
    fs::write(&temp_path, &rom)?;
    let verify = rommap::load_rom(&temp_path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} (map_mode=0x{:02X} reset=0x{:04X} nmi=0x{:04X} checksum=0x{:04X} complement=0x{:04X})",
                error,
                rom[0x7FD5],
                reset_vector,
                nmi_vector,
                checksum,
                complement
            ),
        )
    })?;
    let _ = fs::remove_file(temp_path);
    if verify.info.mapping != rommap::MappingKind::LoRom {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated ROM header did not verify as LoROM",
        ));
    }
    Ok(rom)
}

fn render_rom_summary(spec: &RomSpec, manifest: &GameManifest) -> String {
    let mut out = String::new();
    out.push_str("Template ROM Summary\n");
    out.push_str(&format!("title: {}\n", manifest.title));
    out.push_str("format: LoROM .sfc\n");
    out.push_str("controls: D-pad moves the player sprite\n");
    out.push_str("rendering: OBJ-only placeholder arena runtime\n");
    out.push_str(&format!("boot_scene: {}\n", spec.boot_scene_id));
    out.push_str(&format!("gameplay_scene: {}\n", spec.gameplay_scene_id));
    out.push_str(&format!(
        "player_spawn: {},{}\n",
        spec.player_spawn.0, spec.player_spawn.1
    ));
    if spec.has_npc {
        out.push_str(&format!(
            "npc_spawn: {},{} speed={}\n",
            spec.npc_spawn.0, spec.npc_spawn.1, spec.npc_speed
        ));
    } else {
        out.push_str("npc_spawn: none\n");
    }
    out.push_str("notes:\n");
    out.push_str("- current ROM emitter boots directly into the first gameplay-capable arena\n");
    out.push_str("- scene packets, title flow, and attack rules remain ahead of this runtime\n");
    out
}

fn dma_upload_cgram(asm: &mut RomAssembler, cgram_addr: u8, label: &str, size: u8) {
    asm.lda_imm(cgram_addr);
    asm.sta_abs(0x2121);
    asm.lda_imm(0x00);
    asm.sta_abs(0x4300);
    asm.lda_imm(0x22);
    asm.sta_abs(0x4301);
    asm.lda_label_lo(label);
    asm.sta_abs(0x4302);
    asm.lda_label_hi(label);
    asm.sta_abs(0x4303);
    asm.lda_imm(ROM_BANK);
    asm.sta_abs(0x4304);
    asm.lda_imm(size);
    asm.sta_abs(0x4305);
    asm.stz_abs(0x4306);
    asm.lda_imm(0x01);
    asm.sta_abs(0x420B);
}

fn emit_hidden_sprite(asm: &mut RomAssembler) {
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.lda_imm(0xF0);
    asm.sta_abs(0x2104);
    asm.lda_imm(0x00);
    asm.sta_abs(0x2104);
    asm.sta_abs(0x2104);
}

fn apply_player_axis(
    asm: &mut RomAssembler,
    mask: u8,
    coord_addr: u16,
    speed: u8,
    limit: u8,
    positive: bool,
    facing: u8,
    skip_label: &str,
) {
    asm.lda_abs(0x0009);
    asm.and_imm(mask);
    asm.beq(skip_label);
    asm.lda_abs(coord_addr);
    if positive {
        asm.cmp_imm(limit);
        asm.bcs(skip_label);
        asm.clc();
        asm.adc_imm(speed.max(1) * 2);
    } else {
        asm.cmp_imm(limit);
        asm.bcc(skip_label);
        asm.sec();
        asm.sbc_imm(speed.max(1) * 2);
    }
    asm.sta_abs(coord_addr);
    asm.lda_imm(facing);
    asm.sta_abs(0x000E);
    asm.label(skip_label);
}

fn apply_player_axis_alt(
    asm: &mut RomAssembler,
    mask: u8,
    coord_addr: u16,
    speed: u8,
    limit: u8,
    positive: bool,
    facing: u8,
    skip_label: &str,
) {
    asm.lda_abs(0x000B);
    asm.and_imm(mask);
    asm.beq(skip_label);
    asm.lda_abs(coord_addr);
    if positive {
        asm.cmp_imm(limit);
        asm.bcs(skip_label);
        asm.clc();
        asm.adc_imm(speed.max(1) * 2);
    } else {
        asm.cmp_imm(limit);
        asm.bcc(skip_label);
        asm.sec();
        asm.sbc_imm(speed.max(1) * 2);
    }
    asm.sta_abs(coord_addr);
    asm.lda_imm(facing);
    asm.sta_abs(0x000E);
    asm.label(skip_label);
}

fn build_obj_palette_words(asset: &PaletteAsset) -> Vec<u16> {
    let colors = palette_colors(&asset.preset);
    let mut out = vec![0u16; 16];
    for (index, color) in colors.iter().take(4).enumerate() {
        out[index] = rgb_to_bgr555(*color);
    }
    out[0] = 0;
    out
}

fn build_ball_tiles_4bpp() -> Vec<u8> {
    let mut out = Vec::new();
    for frame in 0..4u8 {
        let (x_radius, y_radius) = match frame {
            0 => (2.6, 2.6),
            1 => (3.2, 2.2),
            2 => (2.8, 2.4),
            _ => (2.4, 3.0),
        };
        let pixels = breathing_ball_frame_8x8(x_radius, y_radius);
        out.extend_from_slice(&encode_4bpp_tile(&pixels));
    }
    out
}

fn breathing_ball_frame_8x8(x_radius: f32, y_radius: f32) -> [u8; 64] {
    let mut out = [0u8; 64];
    let cx = 3.5f32;
    let cy = 3.5f32;
    for y in 0..8usize {
        for x in 0..8usize {
            let dx = (x as f32 - cx) / x_radius;
            let dy = (y as f32 - cy) / y_radius;
            let dist = dx * dx + dy * dy;
            out[y * 8 + x] = if dist <= 1.0 {
                if dx < -0.15 && dy < -0.15 {
                    2
                } else if dy > 0.38 {
                    3
                } else {
                    1
                }
            } else {
                0
            };
        }
    }
    out
}

fn encode_4bpp_tile(pixels: &[u8; 64]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for y in 0..8usize {
        let mut p0 = 0u8;
        let mut p1 = 0u8;
        let mut p2 = 0u8;
        let mut p3 = 0u8;
        for x in 0..8usize {
            let bit = 7 - x;
            let value = pixels[y * 8 + x];
            p0 |= (value & 0x01) << bit;
            p1 |= ((value >> 1) & 0x01) << bit;
            p2 |= ((value >> 2) & 0x01) << bit;
            p3 |= ((value >> 3) & 0x01) << bit;
        }
        out[y * 2] = p0;
        out[y * 2 + 1] = p1;
        out[16 + y * 2] = p2;
        out[16 + y * 2 + 1] = p3;
    }
    out
}

fn palette_colors(preset: &str) -> Vec<[u8; 3]> {
    match preset {
        "ball_player" => vec![
            [0, 0, 0],
            [231, 82, 82],
            [255, 196, 196],
            [142, 32, 48],
        ],
        "ball_npc" => vec![
            [0, 0, 0],
            [76, 182, 98],
            [210, 255, 216],
            [30, 94, 56],
        ],
        _ => vec![
            [0, 0, 0],
            [96, 120, 168],
            [192, 210, 240],
            [44, 56, 88],
        ],
    }
}

fn palette_rgb(preset: &str, index: usize) -> [u8; 3] {
    palette_colors(preset)
        .get(index)
        .copied()
        .unwrap_or([0, 0, 0])
}

fn rgb_to_bgr555(rgb: [u8; 3]) -> u16 {
    let r = (u16::from(rgb[0]) >> 3) & 0x1F;
    let g = (u16::from(rgb[1]) >> 3) & 0x1F;
    let b = (u16::from(rgb[2]) >> 3) & 0x1F;
    r | (g << 5) | (b << 10)
}

fn parse_spawn(raw: &str) -> (u8, u8) {
    let Some((x, y)) = raw.split_once(',') else {
        return (32, 32);
    };
    let x = x.trim().parse::<u16>().unwrap_or(8).saturating_mul(4).min(232);
    let y = y.trim().parse::<u16>().unwrap_or(8).saturating_mul(4).min(216);
    (x as u8, y as u8)
}

fn truncate_title(title: &str) -> String {
    title.chars().take(21).collect()
}

fn write_header(
    rom: &mut [u8],
    spec: &RomSpec,
    nmi_vector: u16,
    reset_vector: u16,
    irq_vector: u16,
) -> io::Result<()> {
    if rom.len() < ROM_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "ROM buffer too small"));
    }
    let title = spec.title.as_bytes();
    let title_len = title.len().min(21);
    rom[0x7FC0..0x7FC0 + 21].fill(b' ');
    rom[0x7FC0..0x7FC0 + title_len].copy_from_slice(&title[..title_len]);
    rom[0x7FD5] = 0x20;
    rom[0x7FD6] = 0x00;
    rom[0x7FD7] = 0x09;
    rom[0x7FD8] = 0x00;
    rom[0x7FD9] = 0x01;
    rom[0x7FDA] = 0x00;
    rom[0x7FDB] = 0x00;
    rom[0x7FFA..0x7FFC].copy_from_slice(&nmi_vector.to_le_bytes());
    rom[0x7FFC..0x7FFE].copy_from_slice(&reset_vector.to_le_bytes());
    rom[0x7FFE..0x8000].copy_from_slice(&irq_vector.to_le_bytes());
    Ok(())
}

fn compute_checksum(bytes: &[u8]) -> (u16, u16) {
    let checksum = bytes
        .iter()
        .fold(0u16, |acc, byte| acc.wrapping_add(u16::from(*byte)));
    (checksum, !checksum)
}

#[derive(Debug, Clone)]
struct RomAssembler {
    bytes: Vec<u8>,
    labels: BTreeMap<String, u16>,
    abs16_patches: Vec<(usize, String)>,
    lo8_patches: Vec<(usize, String)>,
    hi8_patches: Vec<(usize, String)>,
    rel8_patches: Vec<(usize, String)>,
}

impl RomAssembler {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: BTreeMap::new(),
            abs16_patches: Vec::new(),
            lo8_patches: Vec::new(),
            hi8_patches: Vec::new(),
            rel8_patches: Vec::new(),
        }
    }

    fn addr(&self) -> u16 {
        0x8000u16 + self.bytes.len() as u16
    }

    fn label(&mut self, name: &str) {
        self.labels.insert(name.to_string(), self.addr());
    }

    fn align(&mut self, align: usize) {
        while self.bytes.len() % align != 0 {
            self.bytes.push(0);
        }
    }

    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn bytes(&mut self, values: &[u8]) {
        self.bytes.extend_from_slice(values);
    }

    fn words(&mut self, values: &[u16]) {
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn lda_imm(&mut self, value: u8) {
        self.byte(0xA9);
        self.byte(value);
    }

    fn lda_abs(&mut self, addr: u16) {
        self.byte(0xAD);
        self.word(addr);
    }

    fn sta_abs(&mut self, addr: u16) {
        self.byte(0x8D);
        self.word(addr);
    }

    fn sta_abs_x(&mut self, addr: u16) {
        self.byte(0x9D);
        self.word(addr);
    }

    fn stz_abs(&mut self, addr: u16) {
        self.byte(0x9C);
        self.word(addr);
    }

    fn inc_abs(&mut self, addr: u16) {
        self.byte(0xEE);
        self.word(addr);
    }

    fn dec_abs(&mut self, addr: u16) {
        self.byte(0xCE);
        self.word(addr);
    }

    fn ldx_imm(&mut self, value: u8) {
        self.byte(0xA2);
        self.byte(value);
    }

    fn ldx_imm16(&mut self, value: u16) {
        self.byte(0xA2);
        self.word(value);
    }

    fn cpx_imm16(&mut self, value: u16) {
        self.byte(0xE0);
        self.word(value);
    }

    fn inx(&mut self) {
        self.byte(0xE8);
    }

    fn dex(&mut self) {
        self.byte(0xCA);
    }

    fn rep(&mut self, mask: u8) {
        self.byte(0xC2);
        self.byte(mask);
    }

    fn sep(&mut self, mask: u8) {
        self.byte(0xE2);
        self.byte(mask);
    }

    fn cmp_imm(&mut self, value: u8) {
        self.byte(0xC9);
        self.byte(value);
    }

    fn and_imm(&mut self, value: u8) {
        self.byte(0x29);
        self.byte(value);
    }

    fn eor_imm(&mut self, value: u8) {
        self.byte(0x49);
        self.byte(value);
    }

    fn adc_imm(&mut self, value: u8) {
        self.byte(0x69);
        self.byte(value);
    }

    fn sbc_abs(&mut self, addr: u16) {
        self.byte(0xED);
        self.word(addr);
    }

    fn sbc_imm(&mut self, value: u8) {
        self.byte(0xE9);
        self.byte(value);
    }

    fn clc(&mut self) {
        self.byte(0x18);
    }

    fn sec(&mut self) {
        self.byte(0x38);
    }

    fn jsr(&mut self, label: &str) {
        self.byte(0x20);
        self.patch_abs16(label);
    }

    fn jmp(&mut self, label: &str) {
        self.byte(0x4C);
        self.patch_abs16(label);
    }

    fn bra(&mut self, label: &str) {
        self.byte(0x80);
        self.patch_rel8(label);
    }

    fn beq(&mut self, label: &str) {
        self.byte(0xF0);
        self.patch_rel8(label);
    }

    fn bne(&mut self, label: &str) {
        self.byte(0xD0);
        self.patch_rel8(label);
    }

    fn bcc(&mut self, label: &str) {
        self.byte(0x90);
        self.patch_rel8(label);
    }

    fn bcs(&mut self, label: &str) {
        self.byte(0xB0);
        self.patch_rel8(label);
    }

    fn bpl(&mut self, label: &str) {
        self.byte(0x10);
        self.patch_rel8(label);
    }

    fn lda_label_lo(&mut self, label: &str) {
        self.byte(0xA9);
        self.patch_lo8(label);
    }

    fn lda_label_hi(&mut self, label: &str) {
        self.byte(0xA9);
        self.patch_hi8(label);
    }

    fn word(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn patch_abs16(&mut self, label: &str) {
        let pos = self.bytes.len();
        self.bytes.extend_from_slice(&[0, 0]);
        self.abs16_patches.push((pos, label.to_string()));
    }

    fn patch_lo8(&mut self, label: &str) {
        let pos = self.bytes.len();
        self.bytes.push(0);
        self.lo8_patches.push((pos, label.to_string()));
    }

    fn patch_hi8(&mut self, label: &str) {
        let pos = self.bytes.len();
        self.bytes.push(0);
        self.hi8_patches.push((pos, label.to_string()));
    }

    fn patch_rel8(&mut self, label: &str) {
        let pos = self.bytes.len();
        self.bytes.push(0);
        self.rel8_patches.push((pos, label.to_string()));
    }

    fn finalize(mut self) -> io::Result<Vec<u8>> {
        for (pos, label) in &self.abs16_patches {
            let addr = self.lookup(label)?;
            self.bytes[*pos..*pos + 2].copy_from_slice(&addr.to_le_bytes());
        }
        for (pos, label) in &self.lo8_patches {
            self.bytes[*pos] = (self.lookup(label)? & 0xFF) as u8;
        }
        for (pos, label) in &self.hi8_patches {
            self.bytes[*pos] = (self.lookup(label)? >> 8) as u8;
        }
        for (pos, label) in &self.rel8_patches {
            let target = self.lookup(label)? as i32;
            let origin = 0x8000i32 + *pos as i32 + 1;
            let delta = target - origin;
            if !(-128..=127).contains(&delta) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("branch to `{label}` is out of 8-bit range"),
                ));
            }
            self.bytes[*pos] = (delta as i8) as u8;
        }
        Ok(self.bytes)
    }

    fn lookup(&self, label: &str) -> io::Result<u16> {
        self.labels.get(label).copied().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing assembler label `{label}`"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_bootable_rom, palette_colors, parse_spawn, rgb_to_bgr555,
    };
    use crate::template::assets::{
        AssetBundle, AssetResolution, AudioAsset, BackgroundAsset, PaletteAsset, SceneLoadPackets,
        SpritePageAsset,
    };
    use crate::template::content::{CompiledContent, EntityDef, SceneDef, ScriptDef};
    use crate::template::engine::{build_engine_plan, EngineBuildPlan};
    use crate::template::runtime::default_runtime_skeleton;
    use crate::template::{GameManifest, TemplateKind};
    use std::fs;

    #[test]
    fn parse_spawn_scales_coordinates() {
        assert_eq!(parse_spawn("8,8"), (32, 32));
    }

    #[test]
    fn rgb_conversion_produces_bgr555() {
        assert_eq!(rgb_to_bgr555([255, 0, 0]), 0x001F);
        assert_eq!(rgb_to_bgr555([0, 255, 0]), 0x03E0);
        assert_eq!(rgb_to_bgr555([0, 0, 255]), 0x7C00);
        assert_eq!(palette_colors("ball_player").len(), 4);
    }

    #[test]
    fn build_outputs_valid_lorom_image() {
        let root = std::env::temp_dir().join(format!("template-rom-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("mkdir");

        let manifest = GameManifest {
            name: "arena-demo".to_string(),
            template: TemplateKind::SingleScreenAction,
            title: "Arena Demo".to_string(),
            region: "ntsc".to_string(),
            version: "0.1.0".to_string(),
        };
        let content = CompiledContent {
            template: TemplateKind::SingleScreenAction,
            game: "arena-demo".to_string(),
            title_scene: "title_room".to_string(),
            scenes: vec![
                SceneDef {
                    id: "title_room".to_string(),
                    kind: "title".to_string(),
                    background: "bg_title".to_string(),
                    palette: "default".to_string(),
                    music: "title_theme".to_string(),
                    player_spawn: "12,14".to_string(),
                    enemy_set: "none".to_string(),
                    next_scene: "room_000".to_string(),
                    source_file: "title_room.toml".to_string(),
                },
                SceneDef {
                    id: "room_000".to_string(),
                    kind: "gameplay".to_string(),
                    background: "bg_main".to_string(),
                    palette: "default".to_string(),
                    music: "stage_01".to_string(),
                    player_spawn: "8,8".to_string(),
                    enemy_set: "arena_enemies".to_string(),
                    next_scene: "room_000".to_string(),
                    source_file: "room_000.toml".to_string(),
                },
            ],
            entities: vec![
                EntityDef {
                    id: "player".to_string(),
                    kind: "player".to_string(),
                    sprite_page: "ball_player".to_string(),
                    palette: "player_ball".to_string(),
                    hitbox: "8,8,16,16".to_string(),
                    speed: 2,
                    jump: 0,
                    attack: "basic".to_string(),
                    source_file: "player.toml".to_string(),
                },
                EntityDef {
                    id: "npc_ball".to_string(),
                    kind: "npc".to_string(),
                    sprite_page: "ball_npc".to_string(),
                    palette: "npc_ball".to_string(),
                    hitbox: "8,8,16,16".to_string(),
                    speed: 1,
                    jump: 0,
                    attack: "touch".to_string(),
                    source_file: "npc_ball.toml".to_string(),
                },
            ],
            script: ScriptDef {
                on_boot: "load_scene title_room".to_string(),
                on_game_over: "load_scene title_room".to_string(),
                on_room_clear: "load_scene room_000".to_string(),
                source_file: "main.toml".to_string(),
            },
        };
        let assets = AssetBundle {
            template: TemplateKind::SingleScreenAction,
            backgrounds: vec![
                BackgroundAsset {
                    id: 1,
                    name: "bg_title".to_string(),
                    source: "bg_title.png".to_string(),
                    palette: "default".to_string(),
                    vram_slot: "bg_tiles".to_string(),
                    source_file: "bg_title.toml".to_string(),
                },
                BackgroundAsset {
                    id: 2,
                    name: "bg_main".to_string(),
                    source: "bg_main.png".to_string(),
                    palette: "default".to_string(),
                    vram_slot: "bg_tiles".to_string(),
                    source_file: "bg_main.toml".to_string(),
                },
            ],
            palettes: vec![
                PaletteAsset {
                    id: 1,
                    name: "default".to_string(),
                    source: "default.pal".to_string(),
                    cgram_slot: "palette0".to_string(),
                    preset: "background_basic".to_string(),
                    source_file: "default.toml".to_string(),
                },
                PaletteAsset {
                    id: 2,
                    name: "player_ball".to_string(),
                    source: "player_ball.pal".to_string(),
                    cgram_slot: "palette4".to_string(),
                    preset: "ball_player".to_string(),
                    source_file: "player_ball.toml".to_string(),
                },
                PaletteAsset {
                    id: 3,
                    name: "npc_ball".to_string(),
                    source: "npc_ball.pal".to_string(),
                    cgram_slot: "palette5".to_string(),
                    preset: "ball_npc".to_string(),
                    source_file: "npc_ball.toml".to_string(),
                },
            ],
            sprite_pages: vec![
                SpritePageAsset {
                    id: 1,
                    name: "ball_player".to_string(),
                    source: "ball_player.gen".to_string(),
                    palette: "player_ball".to_string(),
                    vram_slot: "sprite_tiles".to_string(),
                    generator: "breathing_ball".to_string(),
                    frame_count: 4,
                    source_file: "ball_player.toml".to_string(),
                },
                SpritePageAsset {
                    id: 2,
                    name: "ball_npc".to_string(),
                    source: "ball_npc.gen".to_string(),
                    palette: "npc_ball".to_string(),
                    vram_slot: "sprite_tiles".to_string(),
                    generator: "breathing_ball".to_string(),
                    frame_count: 4,
                    source_file: "ball_npc.toml".to_string(),
                },
            ],
            audio_tracks: vec![
                AudioAsset {
                    id: 1,
                    name: "title_theme".to_string(),
                    source: "title_theme.spc".to_string(),
                    kind: "music".to_string(),
                    source_file: "title_theme.toml".to_string(),
                },
                AudioAsset {
                    id: 2,
                    name: "stage_01".to_string(),
                    source: "stage_01.spc".to_string(),
                    kind: "music".to_string(),
                    source_file: "stage_01.toml".to_string(),
                },
            ],
        };
        let resolution = AssetResolution {
            rooms: Vec::new(),
            entities: Vec::new(),
        };
        let runtime = default_runtime_skeleton(TemplateKind::SingleScreenAction);
        let plan = build_engine_plan(
            &content,
            &resolution,
            &SceneLoadPackets { packets: Vec::new() },
            &runtime,
        )
        .unwrap_or_else(|_| EngineBuildPlan {
            boot_scene: "title_room".to_string(),
            joypad_map: Vec::new(),
            frame_steps: Vec::new(),
            scene_packet_map: Vec::new(),
            entity_runtime: Vec::new(),
        });

        let artifact = build_bootable_rom(&root, &manifest, &content, &assets, &resolution, &plan)
            .expect("emit rom");
        let loaded = crate::rommap::load_rom(&artifact.rom_path).expect("parse rom");
        assert_eq!(loaded.info.mapping, crate::rommap::MappingKind::LoRom);
        assert_eq!(loaded.info.reset_vector, Some(0x8000));
        assert!(artifact.summary.contains("Template ROM Summary"));
        let _ = fs::remove_dir_all(root);
    }
}
