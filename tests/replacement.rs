use snes_rom_hack::replacement::{derive_replacement_report, format_replacement_report};
use snes_rom_hack::runtime::{
    LabelActivity, RegisterActivity, RuntimeCorrelationReport, RuntimeTransferEpisode,
    TransferDescriptor,
};
use std::collections::BTreeMap;

#[test]
fn groups_runtime_episodes_into_replacement_categories() {
    let runtime = RuntimeCorrelationReport {
        event_count: 10,
        resolved_pc_count: 10,
        unresolved_pc_count: 0,
        block_resolved_count: 10,
        ignored_line_count: 0,
        cfg_edge_count: 1,
        events_by_kind: BTreeMap::new(),
        top_labels: Vec::new(),
        top_registers: Vec::new(),
        top_routines: Vec::new(),
        top_episodes: vec![RuntimeTransferEpisode {
            start_frame: 10,
            end_frame: 20,
            total_events: 5,
            dma_events: 5,
            vram_events: 1,
            cgram_events: 1,
            oam_events: 0,
            sound_events: 0,
            registers: vec![RegisterActivity {
                address: "$4300".to_string(),
                count: 3,
            }],
            routines: vec![LabelActivity {
                name: "nmi_entry".to_string(),
                count: 5,
            }],
            primary_routine: Some("nmi_entry".to_string()),
            producer_candidate: Some("loc_80_8F1E".to_string()),
            queue_writer_candidate: Some("sub_80_89F0".to_string()),
            queue_writer_labels: vec![LabelActivity {
                name: "sub_80_89F0".to_string(),
                count: 2,
            }],
            staging_writer_candidate: Some("sub_80_9ABC".to_string()),
            staging_writer_labels: vec![LabelActivity {
                name: "sub_80_9ABC".to_string(),
                count: 4,
            }],
            replacement_targets: vec!["graphics".to_string(), "palette".to_string()],
            staging_buffers: vec!["$7E:2000".to_string(), "$7F:8000".to_string()],
            transfers: vec![
                TransferDescriptor {
                    launch_kind: "DMA".to_string(),
                    source: "$7F:8000".to_string(),
                    source_space: "wram".to_string(),
                    destination: "vram".to_string(),
                    size: "$1000".to_string(),
                    count: 1,
                    pipeline: "wram_staged_upload".to_string(),
                },
                TransferDescriptor {
                    launch_kind: "DMA".to_string(),
                    source: "$7E:2000".to_string(),
                    source_space: "wram".to_string(),
                    destination: "cgram".to_string(),
                    size: "$0200".to_string(),
                    count: 1,
                    pipeline: "wram_staged_upload".to_string(),
                },
            ],
        }],
    };

    let report = derive_replacement_report(&runtime);
    assert_eq!(report.graphics_candidates.len(), 1);
    assert_eq!(report.palette_candidates.len(), 1);
    assert_eq!(
        report.graphics_candidates[0].producer.as_deref(),
        Some("loc_80_8F1E")
    );
    assert_eq!(
        report.graphics_candidates[0].queue_writer.as_deref(),
        Some("sub_80_89F0")
    );
    assert_eq!(
        report.graphics_candidates[0].staging_writer.as_deref(),
        Some("sub_80_9ABC")
    );
    assert!(report.graphics_candidates[0]
        .notes
        .contains(&"uses WRAM staging buffers".to_string()));

    let summary = format_replacement_report(&report);
    assert!(summary.contains("Graphics"));
    assert!(summary.contains("$7F:8000"));
}
