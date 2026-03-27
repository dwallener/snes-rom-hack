use snes_rom_hack::evidence::{combine_evidence, format_evidence_summary};
use snes_rom_hack::runtime::RuntimeCorrelationReport;
use snes_rom_hack::usage::UsageImportReport;
use std::collections::BTreeMap;

#[test]
fn combines_runtime_and_usage_into_ranked_routines() {
    let runtime = RuntimeCorrelationReport {
        event_count: 4,
        resolved_pc_count: 4,
        unresolved_pc_count: 0,
        block_resolved_count: 4,
        ignored_line_count: 0,
        cfg_edge_count: 2,
        events_by_kind: BTreeMap::new(),
        top_labels: Vec::new(),
        top_registers: Vec::new(),
        top_routines: vec![
            snes_rom_hack::runtime::RoutineActivity {
                name: "sub_80_9000".to_string(),
                total_events: 3,
                dma_events: 2,
                vram_events: 1,
                cgram_events: 0,
                oam_events: 0,
                sound_events: 0,
                wram_stage_events: 0,
                other_ppu_events: 0,
                register_writes: 3,
                frames: vec![1, 2],
            },
            snes_rom_hack::runtime::RoutineActivity {
                name: "sub_80_A000".to_string(),
                total_events: 1,
                dma_events: 0,
                vram_events: 0,
                cgram_events: 0,
                oam_events: 1,
                sound_events: 0,
                wram_stage_events: 0,
                other_ppu_events: 0,
                register_writes: 1,
                frames: vec![2],
            },
        ],
        top_episodes: Vec::new(),
    };
    let usage = UsageImportReport {
        rom_size: 10,
        usage_size: 10,
        observed_executed_bytes: 5,
        observed_data_bytes: 2,
        observed_unknown_to_code: 1,
        observed_unknown_to_data: 1,
        code_data_overlap_bytes: 0,
        top_routines: vec![
            snes_rom_hack::usage::UsageRoutineActivity {
                name: "sub_80_9000".to_string(),
                executed_bytes: 5,
                data_bytes: 2,
            },
            snes_rom_hack::usage::UsageRoutineActivity {
                name: "sub_80_B000".to_string(),
                executed_bytes: 3,
                data_bytes: 0,
            },
        ],
        warnings: Vec::new(),
    };

    let report = combine_evidence(&runtime, &usage);
    assert_eq!(report.top_routines[0].name, "sub_80_9000");
    assert!(report.top_routines[0].tags.contains(&"dma-heavy".to_string()));
    assert!(report.top_routines[0].tags.contains(&"runtime-and-usage".to_string()));
    assert!(report.top_routines.iter().any(|item| item.name == "sub_80_B000"));

    let summary = format_evidence_summary(&report);
    assert!(summary.contains("sub_80_9000"));
    assert!(summary.contains("dma-heavy"));
}
