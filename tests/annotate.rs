use snes_rom_hack::annotate::{annotate_disasm_with_evidence, build_hot_routines};
use snes_rom_hack::evidence::{CombinedEvidenceReport, CombinedRoutineEvidence};
use std::collections::BTreeMap;

fn sample_evidence() -> CombinedEvidenceReport {
    CombinedEvidenceReport {
        top_routines: vec![CombinedRoutineEvidence {
            name: "reset_entry".to_string(),
            score: 25,
            runtime_events: 2,
            dma_events: 1,
            vram_events: 1,
            cgram_events: 0,
            oam_events: 0,
            sound_events: 0,
            other_ppu_events: 0,
            register_writes: 2,
            runtime_frames: vec![12],
            observed_executed_bytes: 7,
            observed_data_bytes: 1,
            tags: vec![
                "dma-heavy".to_string(),
                "ppu-upload".to_string(),
                "runtime-and-usage".to_string(),
            ],
        }],
        warnings: Vec::new(),
    }
}

#[test]
fn annotates_matching_label_in_disassembly() {
    let disasm = "; header\n\nreset_entry:\n$80:8000  78          sei\nloc_80_8001:\n$80:8001  d8          cld\n";
    let annotated = annotate_disasm_with_evidence(disasm, &sample_evidence());
    assert!(annotated.contains("; evidence score=25 runtime=2 dma=1 vram=1"));
    assert!(annotated.contains("reset_entry:\n$80:8000"));
    assert!(!annotated.contains("loc_80_8001:\n; evidence"));
}

#[test]
fn builds_hot_routine_records_with_addresses() {
    let labels = BTreeMap::from([
        ("$80:8000".to_string(), "reset_entry".to_string()),
        ("$80:9000".to_string(), "sub_80_9000".to_string()),
    ]);
    let routines = build_hot_routines(&labels, &sample_evidence());
    assert_eq!(routines.len(), 1);
    assert_eq!(routines[0].label, "reset_entry");
    assert_eq!(routines[0].snes_address, "$80:8000");
    assert!(routines[0].tags.contains(&"dma-heavy".to_string()));
}
