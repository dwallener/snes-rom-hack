use snes_rom_hack::disasm65816::{BasicBlock, CfgEdge};
use snes_rom_hack::runtime::{RuntimeCfg, correlate_runtime_lines, load_labels_by_pc};
use std::collections::BTreeMap;

fn cfg_fixture() -> RuntimeCfg {
    RuntimeCfg {
        blocks: vec![
            BasicBlock {
                start_pc: 0x00,
                end_pc: 0x05,
                outgoing_edges: Vec::new(),
            },
            BasicBlock {
                start_pc: 0x10,
                end_pc: 0x15,
                outgoing_edges: Vec::new(),
            },
        ],
        edges: vec![CfgEdge {
            from_pc: 0x00,
            to_pc: Some(0x10),
            edge_type: "call".to_string(),
        }],
    }
}

#[test]
fn loads_labels_and_resolves_lua_probe_event() {
    let labels = load_labels_by_pc(
        r#"{
  "$80:8000": "reset_entry",
  "$80:8010": "sub_80_8010"
}"#,
    )
    .unwrap();
    let lines = vec![r#"{"source":"mesen2_lua","kind":"dma_start","frame":3,"scanline":20,"pc":"0x808012","address":"0x420B","value":1}"#.to_string()];

    let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
    assert_eq!(result.report.event_count, 1);
    assert_eq!(result.report.block_resolved_count, 1);
    assert_eq!(result.events[0].subroutine.as_deref(), Some("sub_80_8010"));
    assert_eq!(result.events[0].address.as_deref(), Some("$420B"));
}

#[test]
fn resolves_event_dumper_line_to_exact_label() {
    let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
    let lines = vec![
        r#"{"type":"DmaRead","pc":"0x808010","scanline":17,"cycle":88,"op_addr":"0x002118","op_value":52}"#
            .to_string(),
    ];

    let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
    assert_eq!(result.report.events_by_kind.get("DmaRead"), Some(&1usize));
    assert_eq!(result.events[0].label.as_deref(), Some("sub_80_8010"));
    assert_eq!(result.events[0].pc_snes.as_deref(), Some("$80:8010"));
}
