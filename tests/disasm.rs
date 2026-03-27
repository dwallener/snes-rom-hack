use snes_rom_hack::disasm65816::{DecodeState, analyze_rom, decode_instruction};
use snes_rom_hack::mapper::{lorom_vector_target_to_pc, pc_to_lorom, snes_to_lorom};
use snes_rom_hack::rommap::{load_rom, strip_copier_header};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn synthetic_lorom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x10000];
    let header = 0x7FC0usize;
    let title = b"SYNTHETIC TEST ROM   ";
    rom[header..header + title.len()].copy_from_slice(title);
    rom[header + 0x15] = 0x20;
    rom[header + 0x16] = 0x00;
    rom[header + 0x17] = 0x09;
    rom[header + 0x18] = 0x00;
    rom[header + 0x19] = 0x01;
    rom[header + 0x1A] = 0x00;
    rom[header + 0x1B] = 0x00;
    rom[header + 0x1C..header + 0x20].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
    rom[header + 0x34..header + 0x36].copy_from_slice(&0x8080u16.to_le_bytes());
    rom[header + 0x3A..header + 0x3C].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[header + 0x3C..header + 0x3E].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[header + 0x3E..header + 0x40].copy_from_slice(&0x8090u16.to_le_bytes());
    rom[0x0000..0x000D].copy_from_slice(&[
        0x78, // sei
        0xD8, // cld
        0x20, 0x10, 0x80, // jsr $8010
        0x80, 0x03, // bra +3 -> $800A
        0xEA, // nop
        0xEA, // nop
        0xEA, // nop
        0x4C, 0x20, 0x80, // jmp $8020
    ]);
    rom[0x0010..0x0015].copy_from_slice(&[
        0xA9, 0x34, // lda #$34
        0x60, // rts
        0xEA, 0xEA,
    ]);
    rom[0x0020..0x0027].copy_from_slice(&[
        0x0A, // asl a
        0xAA, // tax
        0x7C, 0x30, 0x80, // jmp ($8030,x)
        0xEA, 0xEA,
    ]);
    rom[0x0030..0x0034].copy_from_slice(&[
        0x40, 0x80, // -> $8040
        0x50, 0x80, // -> $8050
    ]);
    rom[0x0040..0x0042].copy_from_slice(&[0xEA, 0x60]);
    rom[0x0050..0x0052].copy_from_slice(&[0xEA, 0x60]);
    rom[0x0080..0x0083].copy_from_slice(&[0x40, 0xEA, 0xEA]); // nmi
    rom[0x0090..0x0093].copy_from_slice(&[0x40, 0xEA, 0xEA]); // irq
    rom
}

fn jump_table_fixture() -> Vec<u8> {
    let mut rom = synthetic_lorom();
    rom[0x0000..0x0004].copy_from_slice(&[
        0x20, 0x10, 0x80, // jsr $8010
        0x60, // rts
    ]);
    rom[0x0010..0x0016].copy_from_slice(&[
        0x0A, // asl a
        0xAA, // tax
        0x7C, 0x30, 0x80, // jmp ($8030,x)
        0x60, // rts, unreachable if dispatch is correct
    ]);
    rom[0x0030..0x0034].copy_from_slice(&[
        0x40, 0x80, // -> $8040
        0x50, 0x80, // -> $8050
    ]);
    rom[0x0040..0x0042].copy_from_slice(&[0xEA, 0x60]);
    rom[0x0050..0x0052].copy_from_slice(&[0xEA, 0x60]);
    rom
}

fn unresolved_indirect_fixture() -> Vec<u8> {
    let mut rom = synthetic_lorom();
    rom[0x0000..0x0004].copy_from_slice(&[
        0x20, 0x10, 0x80, // jsr $8010
        0x60, // rts
    ]);
    rom[0x0010..0x0014].copy_from_slice(&[
        0x6C, 0x34, 0x12, // jmp ($1234)
        0x60, // rts, unreachable
    ]);
    rom
}

fn state_join_fixture() -> Vec<u8> {
    let mut rom = synthetic_lorom();
    rom[0x0000..0x000e].copy_from_slice(&[
        0x18, // clc
        0xFB, // xce
        0xF0, 0x05, // beq path_sep
        0xC2, 0x20, // rep #$20
        0x4C, 0x10, 0x80, // jmp shared
        0xE2, 0x20, // sep #$20
        0x4C, 0x10, 0x80, // jmp shared
    ]);
    rom[0x0010..0x0013].copy_from_slice(&[
        0xEA, // nop
        0xEA, // nop
        0x60, // rts
    ]);
    rom
}

fn referenced_data_fixture() -> Vec<u8> {
    let mut rom = synthetic_lorom();
    rom[0x0000..0x0006].copy_from_slice(&[
        0xAD, 0x40, 0x80, // lda $8040
        0x60, // rts
        0xEA,
        0xEA,
    ]);
    rom[0x0040..0x0044].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    rom
}

fn temp_rom_path() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("snes_rom_hack_test_{suffix}.sfc"))
}

fn write_fixture_rom(bytes: &[u8]) -> PathBuf {
    let path = temp_rom_path();
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn strips_copier_header() {
    let mut raw = vec![0xAA; 512];
    raw.extend_from_slice(&synthetic_lorom());
    let (bytes, had_header) = strip_copier_header(&raw);
    assert!(had_header);
    assert_eq!(bytes.len(), synthetic_lorom().len());
}

#[test]
fn lorom_mapping_round_trip() {
    for &pc in &[0usize, 0x1234, 0x7FFF, 0x8000, 0x8ABC] {
        let snes = pc_to_lorom(pc);
        let mapped = snes_to_lorom(snes.bank, snes.addr, 0x100000);
        assert_eq!(mapped, Some(pc));
    }
    assert_eq!(snes_to_lorom(0x80, 0x1234, 0x100000), None);
}

#[test]
fn parses_vectors_from_synthetic_rom() {
    let rom_path = write_fixture_rom(&synthetic_lorom());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    assert_eq!(loaded.info.mapping.name(), "LoROM");
    assert_eq!(loaded.info.reset_vector, Some(0x8000));
    assert_eq!(loaded.info.nmi_vector, Some(0x8000));
    assert_eq!(loaded.info.irq_vector, Some(0x8090));
    assert_eq!(lorom_vector_target_to_pc(0x8000, loaded.info.size), Some(0));
}

#[test]
fn decodes_stateful_immediate_width() {
    let bytes = [0xC2, 0x20, 0xA9, 0x34, 0x12];
    let rep = decode_instruction(
        &bytes,
        0,
        &DecodeState {
            emulation: Some(false),
            m_flag: Some(true),
            x_flag: Some(true),
        },
    );
    assert_eq!(rep.mnemonic, "rep");
    let lda = decode_instruction(&bytes, rep.length, rep.state_out.as_ref().unwrap());
    assert_eq!(lda.mnemonic, "lda");
    assert_eq!(lda.length, 3);
}

#[test]
fn computes_branch_target() {
    let bytes = [0xD0, 0x02, 0xEA, 0xEA, 0x60];
    let instruction = decode_instruction(&bytes, 0, &DecodeState::reset_state());
    assert_eq!(instruction.mnemonic, "bne");
    assert_eq!(instruction.target_pc, Some(4));
    assert_eq!(instruction.fallthrough_pc, Some(2));
}

#[test]
fn recursive_traversal_finds_code_and_cfg() {
    let rom_path = write_fixture_rom(&synthetic_lorom());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    let result = analyze_rom(&loaded.info, &loaded.bytes);
    assert!(result.instructions.contains_key(&0));
    assert!(result.instructions.contains_key(&0x10));
    assert!(result.cfg_edges.iter().any(|edge| edge.edge_type == "call"));
    assert!(result.labels.values().any(|label| label == "reset_entry"));
}

#[test]
fn detects_jump_table_candidate() {
    let rom_path = write_fixture_rom(&jump_table_fixture());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    let result = analyze_rom(&loaded.info, &loaded.bytes);
    assert_eq!(result.jump_tables.len(), 1);
    let candidate = &result.jump_tables[0];
    assert_eq!(candidate.table_pc, 0x30);
    assert_eq!(candidate.targets.len(), 2);
    assert_eq!(candidate.targets, vec![0x40, 0x50]);
    assert!(result.instructions.contains_key(&0x10));
    assert!(result.instructions.contains_key(&0x40));
    assert!(result.instructions.contains_key(&0x50));
    assert_eq!(
        result
            .cfg_edges
            .iter()
            .filter(|edge| edge.edge_type == "unresolved_indirect")
            .count(),
        0
    );
    assert_eq!(result.unresolved_transfers.len(), 0);
}

#[test]
fn unresolved_indirect_jump_is_not_fabricated() {
    let rom_path = write_fixture_rom(&unresolved_indirect_fixture());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    let result = analyze_rom(&loaded.info, &loaded.bytes);
    assert!(
        result
            .cfg_edges
            .iter()
            .any(|edge| edge.edge_type == "unresolved_indirect" && edge.to_pc.is_none())
    );
    assert!(
        result
            .unresolved_transfers
            .iter()
            .any(|item| item.contains("unresolved indirect transfer"))
    );
    assert!(!result.instructions.contains_key(&0x1234));
}

#[test]
fn state_join_with_conflicting_widths_continues() {
    let rom_path = write_fixture_rom(&state_join_fixture());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    let result = analyze_rom(&loaded.info, &loaded.bytes);
    assert!(result.instructions.contains_key(&0x10));
    assert!(result.instructions.contains_key(&0x12));
    assert!(
        result
            .cfg_edges
            .iter()
            .any(|edge| edge.from_pc == 0x0006 && edge.to_pc == Some(0x0010))
    );
    assert!(
        result
            .cfg_edges
            .iter()
            .any(|edge| edge.from_pc == 0x000B && edge.to_pc == Some(0x0010))
    );
    assert!(
        result
            .cfg_edges
            .iter()
            .filter(|edge| edge.to_pc == Some(0x0010))
            .count()
            >= 2
    );
}

#[test]
fn marks_referenced_rom_data() {
    let rom_path = write_fixture_rom(&referenced_data_fixture());
    let loaded = load_rom(&rom_path).unwrap();
    fs::remove_file(&rom_path).unwrap();

    let result = analyze_rom(&loaded.info, &loaded.bytes);
    assert_eq!(result.classification[0x40], "referenced_data");
    assert!(result
        .data_regions
        .iter()
        .any(|region| region.start_pc <= 0x40
            && region.end_pc >= 0x40
            && region.reason == "referenced_data"));
}
