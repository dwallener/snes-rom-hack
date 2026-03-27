use snes_rom_hack::usage::{format_usage_summary, import_usage_map};
use std::collections::BTreeMap;

#[test]
fn usage_map_marks_unknown_bytes_as_observed() {
    let rom = vec![0u8; 8];
    let usage = vec![0, 1, 2, 3, 0, 0, 0, 0];
    let labels = BTreeMap::from([(0usize, "reset_entry".to_string())]);
    let classification = vec![
        "header".to_string(),
        "unknown".to_string(),
        "unknown".to_string(),
        "code".to_string(),
        "vector".to_string(),
        "referenced_data".to_string(),
        "unknown".to_string(),
        "unknown".to_string(),
    ];

    let result = import_usage_map(&rom, &usage, &labels, &classification).unwrap();
    assert_eq!(result.merged_classification[0], "header");
    assert_eq!(result.merged_classification[1], "observed_code");
    assert_eq!(result.merged_classification[2], "observed_data");
    assert_eq!(result.merged_classification[3], "code_data_overlap");
    assert_eq!(result.report.observed_unknown_to_code, 1);
    assert_eq!(result.report.observed_unknown_to_data, 1);
    assert_eq!(result.report.code_data_overlap_bytes, 1);
}

#[test]
fn usage_map_summarizes_routine_activity() {
    let rom = vec![0u8; 6];
    let usage = vec![1, 1, 2, 1, 0, 0];
    let labels = BTreeMap::from([
        (0usize, "reset_entry".to_string()),
        (3usize, "sub_80_8003".to_string()),
    ]);
    let classification = vec!["unknown".to_string(); 6];

    let result = import_usage_map(&rom, &usage, &labels, &classification).unwrap();
    assert_eq!(result.report.top_routines.len(), 2);
    assert_eq!(result.report.top_routines[0].name, "reset_entry");
    assert_eq!(result.report.top_routines[0].executed_bytes, 2);
    assert_eq!(result.report.top_routines[0].data_bytes, 1);
    assert_eq!(result.report.top_routines[1].name, "sub_80_8003");
    assert_eq!(result.report.top_routines[1].executed_bytes, 1);

    let text = format_usage_summary(&result.report);
    assert!(text.contains("Hot routines"));
    assert!(text.contains("reset_entry"));
}

#[test]
fn usage_map_rejects_size_mismatch() {
    let rom = vec![0u8; 4];
    let usage = vec![0u8; 5];
    let labels = BTreeMap::new();
    let classification = vec!["unknown".to_string(); 4];

    let error = import_usage_map(&rom, &usage, &labels, &classification).unwrap_err();
    assert!(error
        .to_string()
        .contains("usage map size 5 does not match normalized ROM size 4"));
}

#[test]
fn bizhawk_snes_cdl_cartrom_is_accepted() {
    use snes_rom_hack::usage::run_usage_map_import_cli;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("snes_rom_hack_{name}_{suffix}"))
    }

    fn encode_leb128(mut value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return out;
            }
        }
    }

    fn encode_string(text: &str) -> Vec<u8> {
        let mut out = encode_leb128(text.len() as u32);
        out.extend_from_slice(text.as_bytes());
        out
    }

    let mut rom = vec![0u8; 0x10000];
    let header = 0x7FC0usize;
    rom[header..header + 19].copy_from_slice(b"SYNTHETIC TEST ROM ");
    rom[header + 0x15] = 0x20;
    rom[header + 0x16] = 0x00;
    rom[header + 0x17] = 0x09;
    rom[header + 0x18] = 0x00;
    rom[header + 0x19] = 0x01;
    rom[header + 0x1C..header + 0x20].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
    rom[header + 0x34..header + 0x36].copy_from_slice(&0x8080u16.to_le_bytes());
    rom[header + 0x3A..header + 0x3C].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[header + 0x3C..header + 0x3E].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[header + 0x3E..header + 0x40].copy_from_slice(&0x8090u16.to_le_bytes());
    rom[0x0000..0x000D].copy_from_slice(&[
        0x78, 0xD8, 0x20, 0x10, 0x80, 0x80, 0x03, 0xEA, 0xEA, 0xEA, 0x4C, 0x20, 0x80,
    ]);

    let rom_path = temp_path("usage_test_rom.sfc");
    let labels_path = temp_path("usage_test_labels.json");
    let code_map_path = temp_path("usage_test_code_map.json");
    let cdl_path = temp_path("usage_test.cdl");
    let out_dir = temp_path("usage_out");

    fs::write(&rom_path, &rom).unwrap();
    fs::write(
        &labels_path,
        r#"{"$80:8000":"reset_entry","$80:8003":"sub_80_8003"}"#,
    )
    .unwrap();
    fs::write(
        &code_map_path,
        serde_json::json!({"classification": vec!["unknown"; rom.len()], "likely_data_regions": []})
            .to_string(),
    )
    .unwrap();

    let mut cdl = Vec::new();
    cdl.extend_from_slice(&encode_string("BIZHAWK-CDL-2"));
    cdl.extend_from_slice(&encode_string("SNES           "));
    cdl.extend_from_slice(&0u32.to_le_bytes());
    cdl.extend_from_slice(&1u32.to_le_bytes());
    cdl.extend_from_slice(&encode_string("CARTROM"));
    cdl.extend_from_slice(&(rom.len() as u32).to_le_bytes());
    let mut cartrom = vec![0u8; rom.len()];
    cartrom[0] = 0x01;
    cartrom[1] = 0x02;
    cartrom[2] = 0x04;
    cartrom[3] = 0x08;
    cdl.extend_from_slice(&cartrom);
    fs::write(&cdl_path, cdl).unwrap();

    run_usage_map_import_cli(&[
        "--rom".to_string(),
        rom_path.display().to_string(),
        "--input".to_string(),
        cdl_path.display().to_string(),
        "--labels".to_string(),
        labels_path.display().to_string(),
        "--code-map".to_string(),
        code_map_path.display().to_string(),
        "--out".to_string(),
        out_dir.display().to_string(),
        "--format".to_string(),
        "bizhawk-cdl-snes".to_string(),
    ])
    .unwrap();

    let summary = fs::read_to_string(out_dir.join("usage_summary.txt")).unwrap();
    assert!(summary.contains("observed_execute=2"));
    assert!(summary.contains("observed_data=2"));
}
