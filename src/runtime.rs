use crate::disasm65816::{BasicBlock, CfgEdge};
use crate::mapper::{format_pc, pc_to_lorom, snes_to_lorom};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

#[derive(Clone, Debug, Deserialize)]
pub struct RuntimeCfg {
    pub blocks: Vec<BasicBlock>,
    pub edges: Vec<CfgEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotatedRuntimeEvent {
    pub line_number: usize,
    pub source: String,
    pub kind: String,
    pub launch_kind: Option<String>,
    pub pc_raw: String,
    pub pc_offset: Option<usize>,
    pub pc_snes: Option<String>,
    pub label: Option<String>,
    pub nearest_label: Option<String>,
    pub subroutine: Option<String>,
    pub block_start: Option<String>,
    pub block_end: Option<String>,
    pub address: Option<String>,
    pub value: Option<i64>,
    pub frame: Option<i64>,
    pub scanline: Option<i64>,
    pub cycle: Option<i64>,
    pub mask: Option<String>,
    pub channel: Option<i64>,
    pub dma_source: Option<String>,
    pub dma_bbus: Option<String>,
    pub dma_size: Option<String>,
    pub dma_hdma_table: Option<String>,
    pub dma_control: Option<String>,
    pub region: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LabelActivity {
    pub name: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterActivity {
    pub address: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineActivity {
    pub name: String,
    pub total_events: usize,
    pub dma_events: usize,
    pub vram_events: usize,
    pub cgram_events: usize,
    pub oam_events: usize,
    pub sound_events: usize,
    pub wram_stage_events: usize,
    pub queue_write_events: usize,
    pub other_ppu_events: usize,
    pub register_writes: usize,
    pub frames: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTransferEpisode {
    pub start_frame: i64,
    pub end_frame: i64,
    pub total_events: usize,
    pub dma_events: usize,
    pub vram_events: usize,
    pub cgram_events: usize,
    pub oam_events: usize,
    pub sound_events: usize,
    pub registers: Vec<RegisterActivity>,
    pub routines: Vec<LabelActivity>,
    pub primary_routine: Option<String>,
    pub producer_candidate: Option<String>,
    pub queue_writer_candidate: Option<String>,
    pub queue_writer_labels: Vec<LabelActivity>,
    pub staging_writer_candidate: Option<String>,
    pub staging_writer_labels: Vec<LabelActivity>,
    pub replacement_targets: Vec<String>,
    pub staging_buffers: Vec<String>,
    pub transfers: Vec<TransferDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferDescriptor {
    pub launch_kind: String,
    pub source: String,
    pub source_space: String,
    pub destination: String,
    pub size: String,
    pub count: usize,
    pub pipeline: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCorrelationReport {
    pub event_count: usize,
    pub resolved_pc_count: usize,
    pub unresolved_pc_count: usize,
    pub block_resolved_count: usize,
    pub ignored_line_count: usize,
    pub cfg_edge_count: usize,
    pub events_by_kind: BTreeMap<String, usize>,
    pub top_labels: Vec<LabelActivity>,
    pub top_registers: Vec<RegisterActivity>,
    pub top_routines: Vec<RoutineActivity>,
    pub top_episodes: Vec<RuntimeTransferEpisode>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RuntimeCorrelationResult {
    pub events: Vec<AnnotatedRuntimeEvent>,
    pub report: RuntimeCorrelationReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedRuntimeEvent {
    source: String,
    kind: String,
    launch_kind: Option<String>,
    pc: Option<u32>,
    address: Option<u32>,
    value: Option<i64>,
    frame: Option<i64>,
    scanline: Option<i64>,
    cycle: Option<i64>,
    mask: Option<String>,
    channel: Option<i64>,
    dma_source: Option<String>,
    dma_bbus: Option<String>,
    dma_size: Option<String>,
    dma_hdma_table: Option<String>,
    dma_control: Option<String>,
    region: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct RoutineAccumulator {
    total_events: usize,
    dma_events: usize,
    vram_events: usize,
    cgram_events: usize,
    oam_events: usize,
    sound_events: usize,
    wram_stage_events: usize,
    queue_write_events: usize,
    other_ppu_events: usize,
    register_writes: usize,
    frames: BTreeMap<i64, ()>,
}

#[derive(Clone, Debug, Default)]
struct EpisodeAccumulator {
    start_frame: i64,
    end_frame: i64,
    total_events: usize,
    dma_events: usize,
    vram_events: usize,
    cgram_events: usize,
    oam_events: usize,
    sound_events: usize,
    registers: BTreeMap<String, usize>,
    routines: BTreeMap<String, usize>,
    transfers: BTreeMap<(String, String, String, String, String, String), usize>,
}

pub fn correlate_runtime_lines(
    labels_by_pc: &BTreeMap<usize, String>,
    cfg: &RuntimeCfg,
    lines: &[String],
) -> io::Result<RuntimeCorrelationResult> {
    let mut events = Vec::new();
    let mut events_by_kind = BTreeMap::new();
    let mut label_counts = BTreeMap::new();
    let mut register_counts = BTreeMap::new();
    let mut routine_counts = BTreeMap::<String, RoutineAccumulator>::new();
    let mut resolved_pc_count = 0usize;
    let mut block_resolved_count = 0usize;
    let mut ignored_line_count = 0usize;

    let non_empty_line_indexes = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
        .collect::<Vec<_>>();
    let last_non_empty_line = non_empty_line_indexes.last().copied();

    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed = match parse_runtime_json_line(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                if Some(index) == last_non_empty_line && looks_like_truncated_json(line, &error) {
                    ignored_line_count += 1;
                    continue;
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: {error}", index + 1),
                ));
            }
        };

        let pc_offset = parsed.pc.and_then(snes24_to_lorom_pc);
        let pc_snes = pc_offset.map(|pc| pc_to_lorom(pc).format_snes());
        let label = pc_offset.and_then(|pc| labels_by_pc.get(&pc).cloned());
        let nearest_label = pc_offset.and_then(|pc| {
            labels_by_pc
                .range(..=pc)
                .next_back()
                .map(|(_, label)| label.clone())
        });
        let subroutine = pc_offset.and_then(|pc| {
            labels_by_pc
                .range(..=pc)
                .rev()
                .find(|(_, label)| is_subroutine_like(label))
                .map(|(_, label)| label.clone())
        });
        let block = pc_offset.and_then(|pc| {
            cfg.blocks
                .iter()
                .find(|block| block.start_pc <= pc && pc <= block.end_pc)
        });
        let block_start = block.map(|block| pc_to_lorom(block.start_pc).format_snes());
        let block_end = block.map(|block| pc_to_lorom(block.end_pc).format_snes());
        let address = parsed.address.map(format_runtime_address);

        if pc_offset.is_some() {
            resolved_pc_count += 1;
        }
        if block.is_some() {
            block_resolved_count += 1;
        }
        *events_by_kind.entry(parsed.kind.clone()).or_insert(0usize) += 1;
        if let Some(name) = label.clone().or_else(|| subroutine.clone()) {
            *label_counts.entry(name).or_insert(0usize) += 1;
        }
        if let Some(address) = address.clone() {
            *register_counts.entry(address).or_insert(0usize) += 1;
        }
        if let Some(name) = label
            .clone()
            .or_else(|| subroutine.clone())
            .or_else(|| nearest_label.clone())
            .or_else(|| pc_snes.clone())
        {
            let bucket = classify_runtime_event(&parsed.kind, parsed.address);
            let entry = routine_counts.entry(name).or_default();
            entry.total_events += 1;
            if is_register_write_kind(&parsed.kind) {
                entry.register_writes += 1;
            }
            match bucket {
                RegisterClass::Dma => entry.dma_events += 1,
                RegisterClass::Vram => entry.vram_events += 1,
                RegisterClass::Cgram => entry.cgram_events += 1,
                RegisterClass::Oam => entry.oam_events += 1,
                RegisterClass::ApuIo => entry.sound_events += 1,
                RegisterClass::WramStage => entry.wram_stage_events += 1,
                RegisterClass::QueueWrite => entry.queue_write_events += 1,
                RegisterClass::OtherPpu => entry.other_ppu_events += 1,
                RegisterClass::None => {}
            }
            if let Some(frame) = parsed.frame {
                entry.frames.insert(frame, ());
            }
        }

        let pc_raw = parsed
            .pc
            .map(|pc| format!("0x{pc:06X}"))
            .unwrap_or_else(|| "null".to_string());
        events.push(AnnotatedRuntimeEvent {
            line_number: index + 1,
            source: parsed.source,
            kind: parsed.kind,
            launch_kind: parsed.launch_kind,
            pc_raw,
            pc_offset,
            pc_snes,
            label,
            nearest_label,
            subroutine,
            block_start,
            block_end,
            address,
            value: parsed.value,
            frame: parsed.frame,
            scanline: parsed.scanline,
            cycle: parsed.cycle,
            mask: parsed.mask,
            channel: parsed.channel,
            dma_source: parsed.dma_source,
            dma_bbus: parsed.dma_bbus,
            dma_size: parsed.dma_size,
            dma_hdma_table: parsed.dma_hdma_table,
            dma_control: parsed.dma_control,
            region: parsed.region,
        });
    }

    Ok(RuntimeCorrelationResult {
        report: RuntimeCorrelationReport {
            event_count: events.len(),
            resolved_pc_count,
            unresolved_pc_count: events.len().saturating_sub(resolved_pc_count),
            block_resolved_count,
            ignored_line_count,
            cfg_edge_count: cfg.edges.len(),
            events_by_kind,
            top_labels: top_label_counts(label_counts),
            top_registers: top_register_counts(register_counts),
            top_routines: top_routine_counts(routine_counts),
            top_episodes: top_transfer_episodes(&events),
        },
        events,
    })
}

fn sorted_counts(map: BTreeMap<String, usize>) -> Vec<(String, usize)> {
    let mut items = map
        .into_iter()
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
    });
    items.truncate(16);
    items
}

fn top_label_counts(map: BTreeMap<String, usize>) -> Vec<LabelActivity> {
    sorted_counts(map)
        .into_iter()
        .map(|(name, count)| LabelActivity { name, count })
        .collect()
}

fn top_register_counts(map: BTreeMap<String, usize>) -> Vec<RegisterActivity> {
    sorted_counts(map)
        .into_iter()
        .map(|(address, count)| RegisterActivity { address, count })
        .collect()
}

fn top_routine_counts(map: BTreeMap<String, RoutineAccumulator>) -> Vec<RoutineActivity> {
    let mut items = map.into_iter().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
            .total_events
            .cmp(&left.1.total_events)
            .then_with(|| right.1.dma_events.cmp(&left.1.dma_events))
            .then_with(|| left.0.cmp(&right.0))
    });
    items.truncate(16);
    items.into_iter()
        .map(|(name, counts)| RoutineActivity {
            name,
            total_events: counts.total_events,
            dma_events: counts.dma_events,
            vram_events: counts.vram_events,
            cgram_events: counts.cgram_events,
            oam_events: counts.oam_events,
            sound_events: counts.sound_events,
            wram_stage_events: counts.wram_stage_events,
            queue_write_events: counts.queue_write_events,
            other_ppu_events: counts.other_ppu_events,
            register_writes: counts.register_writes,
            frames: counts.frames.into_keys().collect(),
        })
        .collect()
}

fn top_transfer_episodes(events: &[AnnotatedRuntimeEvent]) -> Vec<RuntimeTransferEpisode> {
    let mut episodes = Vec::<EpisodeAccumulator>::new();
    for event in events {
        let Some(frame) = event.frame else {
            continue;
        };
        let class = classify_runtime_event(&event.kind, event.address.as_deref().and_then(parse_u32));
        if matches!(class, RegisterClass::None | RegisterClass::WramStage) {
            continue;
        }

        let start_new = episodes
            .last()
            .map(|episode| frame > episode.end_frame + 1)
            .unwrap_or(true);
        if start_new {
            episodes.push(EpisodeAccumulator {
                start_frame: frame,
                end_frame: frame,
                ..EpisodeAccumulator::default()
            });
        }

        let episode = episodes.last_mut().expect("episode exists");
        episode.end_frame = frame;
        episode.total_events += 1;
        match class {
            RegisterClass::Dma => episode.dma_events += 1,
            RegisterClass::Vram => episode.vram_events += 1,
            RegisterClass::Cgram => episode.cgram_events += 1,
            RegisterClass::Oam => episode.oam_events += 1,
            RegisterClass::ApuIo => episode.sound_events += 1,
            RegisterClass::WramStage
            | RegisterClass::QueueWrite
            | RegisterClass::OtherPpu
            | RegisterClass::None => {}
        }
        if let Some(address) = &event.address {
            *episode.registers.entry(address.clone()).or_insert(0) += 1;
        }
        if let Some(name) = event
            .label
            .clone()
            .or_else(|| event.subroutine.clone())
            .or_else(|| event.nearest_label.clone())
        {
            *episode.routines.entry(name).or_insert(0) += 1;
        }
        if let Some(transfer) = transfer_key_for_event(event) {
            *episode.transfers.entry(transfer).or_insert(0) += 1;
        }
    }

    let mut out = episodes
        .into_iter()
        .map(|episode| {
            let routines = top_label_counts(episode.routines.clone());
            let registers = top_register_counts(episode.registers);
            let transfers = top_transfer_descriptors(episode.transfers.clone());
            let staging_buffers = derive_staging_buffers(&transfers);
            let staging_writer_labels = top_label_counts(find_staging_writer_counts(
                events,
                episode.start_frame,
                episode.end_frame,
                &transfers,
            ));
            let queue_writer_labels = top_label_counts(find_queue_writer_counts(
                events,
                episode.start_frame,
                episode.end_frame,
            ));
            RuntimeTransferEpisode {
                start_frame: episode.start_frame,
                end_frame: episode.end_frame,
                total_events: episode.total_events,
                dma_events: episode.dma_events,
                vram_events: episode.vram_events,
                cgram_events: episode.cgram_events,
                oam_events: episode.oam_events,
                sound_events: episode.sound_events,
                primary_routine: routines.first().map(|item| item.name.clone()),
                producer_candidate: select_producer_candidate(&routines),
                queue_writer_candidate: select_producer_candidate(&queue_writer_labels),
                queue_writer_labels,
                staging_writer_candidate: select_producer_candidate(&staging_writer_labels),
                staging_writer_labels,
                replacement_targets: derive_replacement_targets(&transfers),
                staging_buffers,
                routines,
                registers,
                transfers,
            }
        })
        .collect::<Vec<_>>();
    out.sort_by(|left, right| {
        right
            .total_events
            .cmp(&left.total_events)
            .then_with(|| left.start_frame.cmp(&right.start_frame))
    });
    out.truncate(12);
    out
}

fn find_queue_writer_counts(
    events: &[AnnotatedRuntimeEvent],
    start_frame: i64,
    end_frame: i64,
) -> BTreeMap<String, usize> {
    let lookback_start = start_frame.saturating_sub(60);
    let mut counts = BTreeMap::new();
    for event in events {
        if event.kind != "asset_queue_write" {
            continue;
        }
        let Some(frame) = event.frame else {
            continue;
        };
        if frame < lookback_start || frame > end_frame {
            continue;
        }
        if let Some(name) = event
            .label
            .clone()
            .or_else(|| event.subroutine.clone())
            .or_else(|| event.nearest_label.clone())
            .or_else(|| event.pc_snes.clone())
        {
            *counts.entry(name).or_insert(0) += 1;
        }
    }
    counts
}

fn find_staging_writer_counts(
    events: &[AnnotatedRuntimeEvent],
    start_frame: i64,
    end_frame: i64,
    transfers: &[TransferDescriptor],
) -> BTreeMap<String, usize> {
    if transfers.is_empty() {
        return BTreeMap::new();
    }
    let lookback_start = start_frame.saturating_sub(30);
    let mut counts = BTreeMap::new();
    for event in events {
        if event.kind != "wram_stage_write" {
            continue;
        }
        let Some(frame) = event.frame else {
            continue;
        };
        if frame < lookback_start || frame > end_frame {
            continue;
        }
        if !matches_event_transfer_range(event, transfers) {
            continue;
        }
        if let Some(name) = event
            .label
            .clone()
            .or_else(|| event.subroutine.clone())
            .or_else(|| event.nearest_label.clone())
            .or_else(|| event.pc_snes.clone())
        {
            *counts.entry(name).or_insert(0) += 1;
        }
    }
    counts
}

fn matches_event_transfer_range(
    event: &AnnotatedRuntimeEvent,
    transfers: &[TransferDescriptor],
) -> bool {
    let Some(event_address) = event.address.as_deref().and_then(parse_u32) else {
        return false;
    };
    transfers.iter().any(|transfer| {
        if transfer.source_space != "wram" {
            return false;
        }
        let Some(start) = parse_u32(&transfer.source) else {
            return false;
        };
        let Some(size) = parse_u32(&transfer.size) else {
            return false;
        };
        let end = start.saturating_add(size.max(1));
        start <= event_address && event_address < end
    })
}

fn derive_replacement_targets(transfers: &[TransferDescriptor]) -> Vec<String> {
    let mut targets = BTreeMap::<String, ()>::new();
    for transfer in transfers {
        let target = match transfer.destination.as_str() {
            "cgram" => "palette",
            "vram" => "graphics",
            "oam" => "sprite_attr",
            "bbus_$2140" | "bbus_$2141" | "bbus_$2142" | "bbus_$2143" => "sound",
            _ => continue,
        };
        targets.insert(target.to_string(), ());
    }
    targets.into_keys().collect()
}

fn derive_staging_buffers(transfers: &[TransferDescriptor]) -> Vec<String> {
    let mut buffers = BTreeMap::<String, ()>::new();
    for transfer in transfers {
        if transfer.source_space == "wram" {
            buffers.insert(transfer.source.clone(), ());
        }
    }
    buffers.into_keys().collect()
}

fn top_transfer_descriptors(
    transfers: BTreeMap<(String, String, String, String, String, String), usize>,
) -> Vec<TransferDescriptor> {
    let mut items = transfers.into_iter().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    items.truncate(8);
    items.into_iter()
        .map(
            |((launch_kind, source, source_space, destination, size, pipeline), count)| {
                TransferDescriptor {
                    launch_kind,
                    source,
                    source_space,
                    destination,
                    size,
                    count,
                    pipeline,
                }
            },
        )
        .collect()
}

fn transfer_key_for_event(
    event: &AnnotatedRuntimeEvent,
) -> Option<(String, String, String, String, String, String)> {
    if !matches!(event.kind.as_str(), "dma_channel" | "hdma_channel") {
        return None;
    }
    let launch_kind = event
        .launch_kind
        .clone()
        .unwrap_or_else(|| "DMA".to_string());
    let source = event
        .dma_source
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let source_space = classify_dma_source_space(&source);
    let destination = event
        .dma_bbus
        .as_deref()
        .map(classify_dma_destination)
        .unwrap_or_else(|| "unknown".to_string());
    let size = event
        .dma_size
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let pipeline = classify_transfer_pipeline(&source_space, &destination);
    Some((launch_kind, source, source_space, destination, size, pipeline))
}

fn select_producer_candidate(routines: &[LabelActivity]) -> Option<String> {
    routines
        .iter()
        .find(|item| !matches!(item.name.as_str(), "nmi_entry" | "irq_entry" | "reset_entry"))
        .or_else(|| routines.first())
        .map(|item| item.name.clone())
}

fn is_subroutine_like(label: &str) -> bool {
    label.starts_with("sub_")
        || matches!(
            label,
            "reset_entry" | "nmi_entry" | "irq_entry" | "abort_entry" | "cop_entry"
        )
}

fn snes24_to_lorom_pc(value: u32) -> Option<usize> {
    let bank = ((value >> 16) & 0xFF) as u8;
    let addr = (value & 0xFFFF) as u16;
    snes_to_lorom(bank, addr, usize::MAX)
}

fn classify_dma_source_space(source: &str) -> String {
    let Some(value) = parse_u32(source) else {
        return "unknown".to_string();
    };
    let bank = ((value >> 16) & 0xFF) as u8;
    match bank {
        0x7E | 0x7F => "wram".to_string(),
        0x80..=0xFF => "rom".to_string(),
        _ => {
            let addr = (value & 0xFFFF) as u16;
            if addr < 0x2000 {
                "lowmem".to_string()
            } else if addr >= 0x8000 {
                "rom_mirror_or_cart".to_string()
            } else {
                "unknown".to_string()
            }
        }
    }
}

fn classify_dma_destination(bbus: &str) -> String {
    match bbus.trim() {
        "$18" | "$19" => "vram".to_string(),
        "$22" => "cgram".to_string(),
        "$04" => "oam".to_string(),
        "$00" => "hdma_table_or_io".to_string(),
        other => format!("bbus_{other}"),
    }
}

fn classify_transfer_pipeline(source_space: &str, destination: &str) -> String {
    match (source_space, destination) {
        ("rom", "vram" | "cgram" | "oam") => "direct_rom_upload".to_string(),
        ("rom_mirror_or_cart", "vram" | "cgram" | "oam") => "direct_rom_upload".to_string(),
        ("wram", "vram" | "cgram" | "oam") => "wram_staged_upload".to_string(),
        ("lowmem", _) => "io_or_table_driven".to_string(),
        ("wram", _) => "wram_staged_transfer".to_string(),
        _ => "unknown".to_string(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegisterClass {
    None,
    Dma,
    Vram,
    Cgram,
    Oam,
    ApuIo,
    WramStage,
    QueueWrite,
    OtherPpu,
}

fn classify_runtime_register(address: u32) -> RegisterClass {
    match address {
        0x420B | 0x420C | 0x4300..=0x43FF => RegisterClass::Dma,
        0x2115..=0x2119 => RegisterClass::Vram,
        0x2121..=0x2122 => RegisterClass::Cgram,
        0x2102..=0x2104 => RegisterClass::Oam,
        0x2140..=0x2143 => RegisterClass::ApuIo,
        0x2100..=0x21FF => RegisterClass::OtherPpu,
        _ => RegisterClass::None,
    }
}

fn classify_runtime_event(kind: &str, address: Option<u32>) -> RegisterClass {
    if matches!(
        kind,
        "dma_reg"
            | "dma_start"
            | "dma_launch"
            | "dma_channel"
            | "hdma_enable"
            | "hdma_launch"
            | "hdma_channel"
            | "DmaRead"
            | "DmaWrite"
    ) {
        return RegisterClass::Dma;
    }
    if matches!(kind, "vram_reg") {
        return RegisterClass::Vram;
    }
    if matches!(kind, "cgram_reg") {
        return RegisterClass::Cgram;
    }
    if matches!(kind, "oam_reg") {
        return RegisterClass::Oam;
    }
    if matches!(kind, "apu_io_reg") {
        return RegisterClass::ApuIo;
    }
    if matches!(kind, "wram_stage_write") {
        return RegisterClass::WramStage;
    }
    if matches!(kind, "asset_queue_write") {
        return RegisterClass::QueueWrite;
    }
    address
        .map(classify_runtime_register)
        .unwrap_or(RegisterClass::None)
}

fn is_register_write_kind(kind: &str) -> bool {
    matches!(
        kind,
        "dma_reg"
            | "dma_start"
            | "dma_launch"
            | "dma_channel"
            | "hdma_enable"
            | "hdma_launch"
            | "hdma_channel"
            | "vram_reg"
            | "cgram_reg"
            | "oam_reg"
            | "apu_io_reg"
            | "wram_stage_write"
            | "asset_queue_write"
            | "register_write"
    )
}

fn parse_runtime_json_line(line: &str) -> Result<ParsedRuntimeEvent, String> {
    let value: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    let source = get_string(&value, &["source"]).unwrap_or_else(|| "unknown".to_string());
    let kind = get_string(&value, &["kind"])
        .or_else(|| get_string(&value, &["type"]))
        .unwrap_or_else(|| "event".to_string());
    let launch_kind = get_string(&value, &["launch_kind"]);
    let pc = get_u32(&value, &["pc", "program_counter"]);
    let address = get_u32(&value, &["address", "op_addr"]);
    let value_field = get_i64(&value, &["value", "op_value"]);
    let frame = get_i64(&value, &["frame", "frameCount"]);
    let scanline = get_i64(&value, &["scanline"]);
    let cycle = get_i64(&value, &["cycle"]);
    let mask = get_string(&value, &["mask"]);
    let channel = get_i64(&value, &["channel"]);
    let dma_source = get_string(&value, &["src"]);
    let dma_bbus = get_string(&value, &["bbus"]);
    let dma_size = get_string(&value, &["size"]);
    let dma_hdma_table = get_string(&value, &["hdma_table"]);
    let dma_control = get_string(&value, &["ctrl"]);
    let region = get_string(&value, &["region"]);

    Ok(ParsedRuntimeEvent {
        source,
        kind,
        launch_kind,
        pc,
        address,
        value: value_field,
        frame,
        scanline,
        cycle,
        mask,
        channel,
        dma_source,
        dma_bbus,
        dma_size,
        dma_hdma_table,
        dma_control,
        region,
    })
}

fn looks_like_truncated_json(line: &str, error: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && !trimmed.ends_with('}')
        && (error.contains("EOF while parsing")
            || error.contains("unterminated")
            || error.contains("expected"))
}

fn get_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    })
}

fn get_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => parse_integer(text),
        _ => None,
    })
}

fn get_u32(value: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::Number(number)) => number.as_u64().and_then(|item| u32::try_from(item).ok()),
        Some(Value::String(text)) => parse_u32(text),
        _ => None,
    })
}

fn parse_integer(text: &str) -> Option<i64> {
    if let Some(value) = parse_u32(text) {
        return Some(i64::from(value));
    }
    text.parse::<i64>().ok()
}

fn parse_u32(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = trimmed.strip_prefix('$') {
        if let Some((bank, addr)) = hex.split_once(':') {
            let bank = u8::from_str_radix(bank, 16).ok()?;
            let addr = u16::from_str_radix(addr, 16).ok()?;
            return Some((u32::from(bank) << 16) | u32::from(addr));
        }
        return u32::from_str_radix(hex, 16).ok();
    }
    if let Some((bank, addr)) = trimmed.split_once(':') {
        let bank = u8::from_str_radix(bank.trim_start_matches('$'), 16).ok()?;
        let addr = u16::from_str_radix(addr.trim_start_matches('$'), 16).ok()?;
        return Some((u32::from(bank) << 16) | u32::from(addr));
    }
    trimmed.parse::<u32>().ok()
}

fn format_runtime_address(address: u32) -> String {
    if address <= 0xFFFF {
        format!("${address:04X}")
    } else if address <= 0xFFFFFF {
        format!("${:02X}:{:04X}", (address >> 16) & 0xFF, address & 0xFFFF)
    } else {
        format!("0x{address:06X}")
    }
}

pub fn format_runtime_summary(result: &RuntimeCorrelationResult) -> String {
    let mut out = String::new();
    out.push_str("; Runtime Correlation Summary\n");
    out.push_str(&format!(
        "; events={} resolved_pc={} unresolved_pc={} block_resolved={} ignored_lines={} cfg_edges={}\n",
        result.report.event_count,
        result.report.resolved_pc_count,
        result.report.unresolved_pc_count,
        result.report.block_resolved_count,
        result.report.ignored_line_count,
        result.report.cfg_edge_count
    ));
    if !result.report.events_by_kind.is_empty() {
        out.push_str("\n; Event kinds\n");
        for (kind, count) in &result.report.events_by_kind {
            out.push_str(&format!("; {kind}: {count}\n"));
        }
    }
    if !result.report.top_labels.is_empty() {
        out.push_str("\n; Hot labels\n");
        for item in &result.report.top_labels {
            out.push_str(&format!("; {}: {}\n", item.name, item.count));
        }
    }
    if !result.report.top_registers.is_empty() {
        out.push_str("\n; Hot registers\n");
        for item in &result.report.top_registers {
            out.push_str(&format!("; {}: {}\n", item.address, item.count));
        }
    }
    if !result.report.top_routines.is_empty() {
        out.push_str("\n; Hot routines\n");
        for item in &result.report.top_routines {
            out.push_str(&format!(
                "; {}: total={} dma={} vram={} cgram={} oam={} sound={} wram_stage={} queue_write={} ppu_other={} reg_writes={} frames={:?}\n",
                item.name,
                item.total_events,
                item.dma_events,
                item.vram_events,
                item.cgram_events,
                item.oam_events,
                item.sound_events,
                item.wram_stage_events,
                item.queue_write_events,
                item.other_ppu_events,
                item.register_writes,
                item.frames
            ));
        }
    }
    if !result.report.top_episodes.is_empty() {
        out.push_str("\n; Transfer episodes\n");
        for item in &result.report.top_episodes {
            out.push_str(&format!(
                "; frames={}..{} total={} dma={} vram={} cgram={} oam={} sound={} primary={} producer={} queue_writer={} staging_writer={} targets={:?} staging={:?} regs={:?}\n",
                item.start_frame,
                item.end_frame,
                item.total_events,
                item.dma_events,
                item.vram_events,
                item.cgram_events,
                item.oam_events,
                item.sound_events,
                item.primary_routine.as_deref().unwrap_or("n/a"),
                item.producer_candidate.as_deref().unwrap_or("n/a"),
                item.queue_writer_candidate.as_deref().unwrap_or("n/a"),
                item.staging_writer_candidate.as_deref().unwrap_or("n/a"),
                item.replacement_targets,
                item.staging_buffers,
                item.registers
                    .iter()
                    .map(|item| format!("{}:{}", item.address, item.count))
                    .collect::<Vec<_>>()
            ));
            if !item.staging_writer_labels.is_empty() {
                out.push_str(&format!(
                    ";   staging_writers={:?}\n",
                    item.staging_writer_labels
                        .iter()
                        .map(|writer| format!("{}:{}", writer.name, writer.count))
                        .collect::<Vec<_>>()
                ));
            }
            if !item.queue_writer_labels.is_empty() {
                out.push_str(&format!(
                    ";   queue_writers={:?}\n",
                    item.queue_writer_labels
                        .iter()
                        .map(|writer| format!("{}:{}", writer.name, writer.count))
                        .collect::<Vec<_>>()
                ));
            }
            if !item.transfers.is_empty() {
                out.push_str(&format!(
                    ";   transfers={:?}\n",
                    item.transfers
                        .iter()
                        .map(|transfer| format!(
                            "{} {} {} -> {} {} x{} {}",
                            transfer.launch_kind,
                            transfer.source_space,
                            transfer.source,
                            transfer.destination,
                            transfer.size,
                            transfer.count,
                            transfer.pipeline
                        ))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    out
}

pub fn load_labels_by_pc(text: &str) -> io::Result<BTreeMap<usize, String>> {
    let labels_by_snes: BTreeMap<String, String> =
        serde_json::from_str(text).map_err(io::Error::other)?;
    let mut labels_by_pc = BTreeMap::new();
    for (snes, label) in labels_by_snes {
        let Some(value) = parse_u32(&snes) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid label address `{snes}`"),
            ));
        };
        let Some(pc) = snes24_to_lorom_pc(value) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("label address `{snes}` does not map into LoROM"),
            ));
        };
        labels_by_pc.insert(pc, label);
    }
    Ok(labels_by_pc)
}

pub fn load_runtime_cfg(text: &str) -> io::Result<RuntimeCfg> {
    serde_json::from_str(text).map_err(io::Error::other)
}

pub fn extract_runtime_seed_pcs(lines: &[String]) -> io::Result<Vec<usize>> {
    let mut pcs = BTreeSet::new();
    let non_empty_line_indexes = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
        .collect::<Vec<_>>();
    let last_non_empty_line = non_empty_line_indexes.last().copied();

    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed = match parse_runtime_json_line(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                if Some(index) == last_non_empty_line && looks_like_truncated_json(line, &error) {
                    continue;
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: {error}", index + 1),
                ));
            }
        };
        if let Some(pc) = parsed.pc.and_then(snes24_to_lorom_pc) {
            pcs.insert(pc);
        }
    }

    Ok(pcs.into_iter().collect())
}

pub fn format_event_debug(event: &AnnotatedRuntimeEvent) -> String {
    let label = event
        .label
        .clone()
        .or_else(|| event.subroutine.clone())
        .or_else(|| event.nearest_label.clone())
        .unwrap_or_else(|| "unresolved".to_string());
    format!(
        "line={} kind={} pc={} label={} block={}..{} addr={} value={:?}",
        event.line_number,
        event.kind,
        event.pc_snes
            .clone()
            .unwrap_or_else(|| format_pc(event.pc_offset.unwrap_or_default())),
        label,
        event.block_start.clone().unwrap_or_else(|| "n/a".to_string()),
        event.block_end.clone().unwrap_or_else(|| "n/a".to_string()),
        event.address.clone().unwrap_or_else(|| "n/a".to_string()),
        event.value
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
            edges: Vec::new(),
        }
    }

    #[test]
    fn correlates_lua_probe_line_to_subroutine() {
        let labels = BTreeMap::from([
            (0x00usize, "reset_entry".to_string()),
            (0x10usize, "sub_80_8010".to_string()),
        ]);
        let lines = vec![r#"{"source":"mesen2_lua","kind":"dma_start","frame":12,"scanline":42,"pc":"0x808012","address":"0x420B","value":1}"#.to_string()];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.report.event_count, 1);
        assert_eq!(result.report.resolved_pc_count, 1);
        assert_eq!(result.events[0].subroutine.as_deref(), Some("sub_80_8010"));
        assert_eq!(result.events[0].address.as_deref(), Some("$420B"));
        assert_eq!(result.events[0].channel, None);
    }

    #[test]
    fn parses_event_dumper_shape() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![r#"{"type":"DmaRead","pc":"0x808010","scanline":17,"cycle":88,"op_addr":"0x002118","op_value":52}"#.to_string()];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.report.events_by_kind.get("DmaRead"), Some(&1usize));
        assert_eq!(result.events[0].label.as_deref(), Some("sub_80_8010"));
        assert_eq!(result.events[0].address.as_deref(), Some("$2118"));
        assert_eq!(result.events[0].launch_kind, None);
    }

    #[test]
    fn parses_mesen2_lua_probe_shape() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![
            r#"{"source":"mesen2_lua","event":"dma_launch","kind":"dma_launch","frame":12,"scanline":42,"pc":"0x808010","mask":"0x01"}"#.to_string(),
            r#"{"source":"mesen2_lua","event":"register_write","kind":"vram_reg","frame":12,"scanline":43,"pc":"0x808012","address":"0x2118","value":170}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.report.events_by_kind.get("dma_launch"), Some(&1usize));
        assert_eq!(result.report.events_by_kind.get("vram_reg"), Some(&1usize));
        assert_eq!(result.events[1].subroutine.as_deref(), Some("sub_80_8010"));
        assert_eq!(result.events[1].address.as_deref(), Some("$2118"));
        assert_eq!(result.report.top_routines.len(), 1);
        assert_eq!(result.report.top_routines[0].name, "sub_80_8010");
        assert_eq!(result.report.top_routines[0].total_events, 2);
        assert_eq!(result.report.top_routines[0].dma_events, 1);
        assert_eq!(result.report.top_routines[0].vram_events, 1);
        assert_eq!(result.report.top_routines[0].sound_events, 0);
        assert_eq!(result.report.top_routines[0].wram_stage_events, 0);
        assert_eq!(result.report.top_routines[0].queue_write_events, 0);
        assert_eq!(result.events[0].mask.as_deref(), Some("0x01"));
    }

    #[test]
    fn routine_summary_buckets_dma_and_oam() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![
            r#"{"source":"mesen2_lua","kind":"dma_reg","frame":7,"scanline":10,"pc":"0x808010","address":"0x4302","value":64}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"dma_start","frame":7,"scanline":11,"pc":"0x808011","address":"0x420B","value":1}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"oam_reg","frame":8,"scanline":12,"pc":"0x808012","address":"0x2104","value":2}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        let routine = &result.report.top_routines[0];
        assert_eq!(routine.name, "sub_80_8010");
        assert_eq!(routine.total_events, 3);
        assert_eq!(routine.dma_events, 2);
        assert_eq!(routine.oam_events, 1);
        assert_eq!(routine.sound_events, 0);
        assert_eq!(routine.wram_stage_events, 0);
        assert_eq!(routine.queue_write_events, 0);
        assert_eq!(routine.register_writes, 3);
        assert_eq!(routine.frames, vec![7, 8]);
    }

    #[test]
    fn preserves_dma_channel_metadata() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![
            r#"{"source":"mesen2_lua","event":"dma_channel","kind":"dma_channel","launch_kind":"DMA","frame":12,"scanline":43,"pc":"0x808012","channel":4,"src":"$7E:1234","bbus":"$18","size":"$0040","hdma_table":"$0000","ctrl":"$01"}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.events[0].launch_kind.as_deref(), Some("DMA"));
        assert_eq!(result.events[0].channel, Some(4));
        assert_eq!(result.events[0].dma_source.as_deref(), Some("$7E:1234"));
        assert_eq!(result.events[0].dma_bbus.as_deref(), Some("$18"));
        assert_eq!(result.events[0].dma_size.as_deref(), Some("$0040"));
    }

    #[test]
    fn buckets_apu_io_as_sound_activity() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![
            r#"{"source":"mesen2_lua","kind":"apu_io_reg","frame":3,"scanline":20,"pc":"0x808012","address":"0x2140","value":127}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.events[0].address.as_deref(), Some("$2140"));
        assert_eq!(result.report.top_routines[0].sound_events, 1);
    }

    #[test]
    fn ignores_truncated_final_json_line() {
        let labels = BTreeMap::from([(0x10usize, "sub_80_8010".to_string())]);
        let lines = vec![
            r#"{"source":"mesen2_lua","kind":"dma_start","frame":7,"scanline":11,"pc":"0x808011","address":"0x420B","value":1}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"oam_reg","frame":8,"scanline":12,"pc":"0x808012","address":"0x2104","value":"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.report.event_count, 1);
        assert_eq!(result.report.ignored_line_count, 1);
    }

    #[test]
    fn groups_transfer_episodes_and_prefers_non_nmi_producer() {
        let labels = BTreeMap::from([
            (0x10usize, "nmi_entry".to_string()),
            (0x12usize, "loc_80_8012".to_string()),
        ]);
        let lines = vec![
            r#"{"source":"mesen2_lua","kind":"dma_start","frame":10,"scanline":11,"pc":"0x808010","address":"0x420B","value":1}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"dma_reg","frame":10,"scanline":12,"pc":"0x808012","address":"0x4302","value":64}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"vram_reg","frame":11,"scanline":13,"pc":"0x808010","address":"0x2118","value":2}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"dma_start","frame":20,"scanline":11,"pc":"0x808010","address":"0x420B","value":1}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        assert_eq!(result.report.top_episodes.len(), 2);
        let first = &result.report.top_episodes[0];
        assert_eq!(first.start_frame, 10);
        assert_eq!(first.end_frame, 11);
        assert_eq!(first.total_events, 3);
        assert_eq!(first.primary_routine.as_deref(), Some("nmi_entry"));
        assert_eq!(first.producer_candidate.as_deref(), Some("loc_80_8012"));
        assert_eq!(first.queue_writer_candidate, None);
        assert_eq!(first.staging_writer_candidate, None);
        assert!(first.replacement_targets.is_empty());
        assert!(first.staging_buffers.is_empty());
    }

    #[test]
    fn extracts_runtime_seed_pcs_from_trace_lines() {
        let lines = vec![
            r#"{"source":"mesen2_lua","kind":"wram_stage_write","frame":9,"pc":"0x808012","address":"$7F:8000","value":85}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"dma_channel","frame":10,"pc":"0x808010","src":"$7F:8000","bbus":"$18","size":"$1000"}"#.to_string(),
        ];

        let pcs = extract_runtime_seed_pcs(&lines).unwrap();
        assert_eq!(pcs, vec![0x10, 0x12]);
    }

    #[test]
    fn attributes_staging_writers_to_matching_wram_buffers() {
        let labels = BTreeMap::from([
            (0x10usize, "nmi_entry".to_string()),
            (0x12usize, "loc_80_8012".to_string()),
        ]);
        let lines = vec![
            r#"{"source":"mesen2_lua","event":"wram_write","kind":"wram_stage_write","region":"graphics_stage","frame":9,"scanline":20,"pc":"0x808012","address":"$7F:8000","value":85}"#.to_string(),
            r#"{"source":"mesen2_lua","event":"dma_channel","kind":"dma_channel","launch_kind":"DMA","frame":10,"scanline":22,"pc":"0x808010","channel":0,"src":"$7F:8000","bbus":"$18","size":"$1000","ctrl":"$01"}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"vram_reg","frame":10,"scanline":23,"pc":"0x808010","address":"0x2118","value":2}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        let episode = &result.report.top_episodes[0];
        assert_eq!(episode.staging_buffers, vec!["$7F:8000".to_string()]);
        assert_eq!(episode.staging_writer_candidate.as_deref(), Some("loc_80_8012"));
        assert_eq!(episode.staging_writer_labels[0].name, "loc_80_8012");
        let helper = result
            .report
            .top_routines
            .iter()
            .find(|routine| routine.name == "loc_80_8012")
            .expect("helper routine present");
        assert_eq!(helper.wram_stage_events, 1);
    }

    #[test]
    fn attributes_queue_writers_to_transfer_episodes() {
        let labels = BTreeMap::from([
            (0x10usize, "nmi_entry".to_string()),
            (0x12usize, "sub_80_8012".to_string()),
        ]);
        let lines = vec![
            r#"{"source":"mesen2_lua","event":"queue_write","kind":"asset_queue_write","region":"asset_queue","frame":9,"scanline":20,"pc":"0x808012","address":"0x0442","value":85}"#.to_string(),
            r#"{"source":"mesen2_lua","event":"dma_channel","kind":"dma_channel","launch_kind":"DMA","frame":10,"scanline":22,"pc":"0x808010","channel":0,"src":"$7F:8000","bbus":"$18","size":"$1000","ctrl":"$01"}"#.to_string(),
            r#"{"source":"mesen2_lua","kind":"vram_reg","frame":10,"scanline":23,"pc":"0x808010","address":"0x2118","value":2}"#.to_string(),
        ];

        let result = correlate_runtime_lines(&labels, &cfg_fixture(), &lines).unwrap();
        let episode = &result.report.top_episodes[0];
        assert_eq!(episode.queue_writer_candidate.as_deref(), Some("sub_80_8012"));
        assert_eq!(episode.queue_writer_labels[0].name, "sub_80_8012");
        let helper = result
            .report
            .top_routines
            .iter()
            .find(|routine| routine.name == "sub_80_8012")
            .expect("queue writer routine present");
        assert_eq!(helper.queue_write_events, 1);
    }
}
