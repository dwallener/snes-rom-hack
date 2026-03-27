use snes_rom_hack::asset_paths::{derive_asset_paths, format_asset_paths};
use snes_rom_hack::evidence::{CombinedEvidenceReport, CombinedRoutineEvidence};
use snes_rom_hack::runtime::AnnotatedRuntimeEvent;

fn sample_event(kind: &str) -> AnnotatedRuntimeEvent {
    AnnotatedRuntimeEvent {
        line_number: 1,
        source: "mesen2_lua".to_string(),
        kind: kind.to_string(),
        launch_kind: Some("DMA".to_string()),
        pc_raw: "0x808A72".to_string(),
        pc_offset: Some(0x0A72),
        pc_snes: Some("$80:8A72".to_string()),
        label: Some("sub_80_8A72".to_string()),
        nearest_label: Some("sub_80_8A72".to_string()),
        subroutine: Some("sub_80_8A72".to_string()),
        block_start: Some("$80:8A72".to_string()),
        block_end: Some("$80:8A84".to_string()),
        address: Some("$2118".to_string()),
        value: Some(0x80),
        frame: Some(12),
        scanline: Some(40),
        cycle: None,
        mask: Some("0x10".to_string()),
        channel: Some(4),
        dma_source: Some("$7E:1234".to_string()),
        dma_bbus: Some("$18".to_string()),
        dma_size: Some("$0040".to_string()),
        dma_hdma_table: None,
        dma_control: Some("$01".to_string()),
    }
}

#[test]
fn derives_asset_path_candidates_from_runtime_and_evidence() {
    let events = vec![sample_event("dma_channel"), sample_event("vram_reg")];
    let evidence = CombinedEvidenceReport {
        top_routines: vec![CombinedRoutineEvidence {
            name: "sub_80_8A72".to_string(),
            score: 42,
            runtime_events: 4,
            dma_events: 2,
            vram_events: 1,
            cgram_events: 0,
            oam_events: 0,
            sound_events: 0,
            other_ppu_events: 0,
            register_writes: 3,
            runtime_frames: vec![12],
            observed_executed_bytes: 9,
            observed_data_bytes: 2,
            tags: vec!["dma-heavy".to_string(), "ppu-upload".to_string()],
        }],
        warnings: Vec::new(),
    };

    let report = derive_asset_paths(&events, &evidence);
    assert_eq!(report.candidates.len(), 1);
    let candidate = &report.candidates[0];
    assert_eq!(candidate.routine, "sub_80_8A72");
    assert_eq!(candidate.score, 42);
    assert_eq!(candidate.dma_channels, vec![4]);
    assert_eq!(candidate.dma_sources, vec!["$7E:1234".to_string()]);
    assert_eq!(candidate.vram_registers, vec!["$2118".to_string()]);

    let text = format_asset_paths(&report);
    assert!(text.contains("sub_80_8A72"));
    assert!(text.contains("$7E:1234"));
}

#[test]
fn derives_sound_asset_candidates_from_apu_ports() {
    let mut event = sample_event("apu_io_reg");
    event.address = Some("$2140".to_string());
    event.label = Some("sub_80_9000".to_string());
    event.nearest_label = Some("sub_80_9000".to_string());
    event.subroutine = Some("sub_80_9000".to_string());

    let evidence = CombinedEvidenceReport {
        top_routines: vec![CombinedRoutineEvidence {
            name: "sub_80_9000".to_string(),
            score: 19,
            runtime_events: 2,
            dma_events: 0,
            vram_events: 0,
            cgram_events: 0,
            oam_events: 0,
            sound_events: 2,
            other_ppu_events: 0,
            register_writes: 2,
            runtime_frames: vec![7],
            observed_executed_bytes: 6,
            observed_data_bytes: 0,
            tags: vec!["sound-upload".to_string(), "runtime-and-usage".to_string()],
        }],
        warnings: Vec::new(),
    };

    let report = derive_asset_paths(&[event], &evidence);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].apu_registers, vec!["$2140".to_string()]);
}
