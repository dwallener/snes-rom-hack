use crate::mapper::{CpuAddress, format_pc, pc_to_lorom, snes_to_lorom};
use crate::rommap::{RomInfo, vector_targets};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
pub struct DecodeState {
    pub emulation: Option<bool>,
    pub m_flag: Option<bool>,
    pub x_flag: Option<bool>,
}

impl DecodeState {
    pub fn reset_state() -> Self {
        Self {
            emulation: Some(true),
            m_flag: Some(true),
            x_flag: Some(true),
        }
        .normalized()
    }

    fn accumulator_is_8bit(&self) -> Option<bool> {
        if self.emulation == Some(true) {
            Some(true)
        } else {
            self.m_flag
        }
    }

    fn index_is_8bit(&self) -> Option<bool> {
        if self.emulation == Some(true) {
            Some(true)
        } else {
            self.x_flag
        }
    }

    fn normalized(mut self) -> Self {
        if self.emulation == Some(true) {
            self.m_flag = Some(true);
            self.x_flag = Some(true);
        }
        self
    }

    fn merge(&self, other: &Self) -> Self {
        DecodeState {
            emulation: merge_flag(self.emulation, other.emulation),
            m_flag: merge_flag(self.m_flag, other.m_flag),
            x_flag: merge_flag(self.x_flag, other.x_flag),
        }
        .normalized()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum FlowType {
    Fallthrough,
    Branch,
    Jump,
    Call,
    Return,
    Stop,
}

#[derive(Clone, Debug, Serialize)]
pub struct Instruction {
    pub pc_offset: usize,
    pub snes_bank: u8,
    pub snes_addr: u16,
    pub bytes_: Vec<u8>,
    pub mnemonic: String,
    pub operand: String,
    pub length: usize,
    pub flow_type: FlowType,
    pub target_pc: Option<usize>,
    pub fallthrough_pc: Option<usize>,
    pub state_in: DecodeState,
    pub state_out: Option<DecodeState>,
    pub confidence: String,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgEdge {
    pub from_pc: usize,
    pub to_pc: Option<usize>,
    pub edge_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BasicBlock {
    pub start_pc: usize,
    pub end_pc: usize,
    pub outgoing_edges: Vec<CfgEdge>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JumpTableCandidate {
    pub source_pc: usize,
    pub table_pc: usize,
    pub table_addr: CpuAddress,
    pub entry_width: usize,
    pub targets: Vec<usize>,
    pub confidence: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataRegion {
    pub start_pc: usize,
    pub end_pc: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnalysisCounts {
    pub reachable_code_bytes: usize,
    pub untouched_bytes: usize,
    pub basic_blocks: usize,
    pub subroutines: usize,
    pub unresolved_indirect_jumps: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DisassemblyResult {
    pub instructions: BTreeMap<usize, Instruction>,
    pub blocks: Vec<BasicBlock>,
    pub cfg_edges: Vec<CfgEdge>,
    pub labels: BTreeMap<usize, String>,
    pub jump_tables: Vec<JumpTableCandidate>,
    pub data_regions: Vec<DataRegion>,
    pub counts: AnalysisCounts,
    pub warnings: Vec<String>,
    pub classification: Vec<String>,
    pub unresolved_transfers: Vec<String>,
}

#[derive(Clone)]
struct TraversalContext {
    instructions: BTreeMap<usize, Instruction>,
    labels: BTreeMap<usize, String>,
    cfg_edges: Vec<CfgEdge>,
    warnings: Vec<String>,
    state_at_pc: HashMap<usize, DecodeState>,
}

#[derive(Clone)]
struct IndirectAnalysis {
    source_pc: usize,
    source_mnemonic: String,
    targets: Vec<usize>,
    jump_table: Option<JumpTableCandidate>,
    unresolved_reason: Option<String>,
    state_for_targets: DecodeState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AddressingMode {
    Implied,
    Accumulator,
    ImmediateM,
    ImmediateX,
    Immediate8,
    Direct,
    DirectX,
    DirectY,
    DirectIndirect,
    DirectIndirectLong,
    DirectIndirectX,
    DirectIndirectY,
    DirectIndirectLongY,
    StackRelative,
    StackRelativeIndirectY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    AbsoluteLong,
    AbsoluteLongX,
    AbsoluteIndirect,
    AbsoluteIndexedIndirect,
    AbsoluteIndirectLong,
    Relative8,
    Relative16,
    BlockMove,
}

#[derive(Clone, Copy)]
struct OpcodeMeta {
    mnemonic: &'static str,
    mode: AddressingMode,
    flow: FlowKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FlowKind {
    Normal,
    BranchCond,
    BranchAlways,
    JumpAbs,
    JumpLong,
    JumpIndirect,
    CallAbs,
    CallLong,
    CallIndirect,
    Return,
    Stop,
}

const fn op(mnemonic: &'static str, mode: AddressingMode, flow: FlowKind) -> OpcodeMeta {
    OpcodeMeta {
        mnemonic,
        mode,
        flow,
    }
}

const OPCODES: [OpcodeMeta; 256] = [
    op("brk", AddressingMode::Immediate8, FlowKind::Stop),
    op("ora", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("cop", AddressingMode::Immediate8, FlowKind::Stop),
    op("ora", AddressingMode::StackRelative, FlowKind::Normal),
    op("tsb", AddressingMode::Direct, FlowKind::Normal),
    op("ora", AddressingMode::Direct, FlowKind::Normal),
    op("asl", AddressingMode::Direct, FlowKind::Normal),
    op("ora", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("php", AddressingMode::Implied, FlowKind::Normal),
    op("ora", AddressingMode::ImmediateM, FlowKind::Normal),
    op("asl", AddressingMode::Accumulator, FlowKind::Normal),
    op("phd", AddressingMode::Implied, FlowKind::Normal),
    op("tsb", AddressingMode::Absolute, FlowKind::Normal),
    op("ora", AddressingMode::Absolute, FlowKind::Normal),
    op("asl", AddressingMode::Absolute, FlowKind::Normal),
    op("ora", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bpl", AddressingMode::Relative8, FlowKind::BranchCond),
    op("ora", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("ora", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "ora",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("trb", AddressingMode::Direct, FlowKind::Normal),
    op("ora", AddressingMode::DirectX, FlowKind::Normal),
    op("asl", AddressingMode::DirectX, FlowKind::Normal),
    op("ora", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("clc", AddressingMode::Implied, FlowKind::Normal),
    op("ora", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("inc", AddressingMode::Accumulator, FlowKind::Normal),
    op("tcs", AddressingMode::Implied, FlowKind::Normal),
    op("trb", AddressingMode::Absolute, FlowKind::Normal),
    op("ora", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("asl", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("ora", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("jsr", AddressingMode::Absolute, FlowKind::CallAbs),
    op("and", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("jsl", AddressingMode::AbsoluteLong, FlowKind::CallLong),
    op("and", AddressingMode::StackRelative, FlowKind::Normal),
    op("bit", AddressingMode::Direct, FlowKind::Normal),
    op("and", AddressingMode::Direct, FlowKind::Normal),
    op("rol", AddressingMode::Direct, FlowKind::Normal),
    op("and", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("plp", AddressingMode::Implied, FlowKind::Normal),
    op("and", AddressingMode::ImmediateM, FlowKind::Normal),
    op("rol", AddressingMode::Accumulator, FlowKind::Normal),
    op("pld", AddressingMode::Implied, FlowKind::Normal),
    op("bit", AddressingMode::Absolute, FlowKind::Normal),
    op("and", AddressingMode::Absolute, FlowKind::Normal),
    op("rol", AddressingMode::Absolute, FlowKind::Normal),
    op("and", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bmi", AddressingMode::Relative8, FlowKind::BranchCond),
    op("and", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("and", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "and",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("bit", AddressingMode::DirectX, FlowKind::Normal),
    op("and", AddressingMode::DirectX, FlowKind::Normal),
    op("rol", AddressingMode::DirectX, FlowKind::Normal),
    op("and", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("sec", AddressingMode::Implied, FlowKind::Normal),
    op("and", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("dec", AddressingMode::Accumulator, FlowKind::Normal),
    op("tsc", AddressingMode::Implied, FlowKind::Normal),
    op("bit", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("and", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("rol", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("and", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("rti", AddressingMode::Implied, FlowKind::Return),
    op("eor", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("wdm", AddressingMode::Immediate8, FlowKind::Normal),
    op("eor", AddressingMode::StackRelative, FlowKind::Normal),
    op("mvp", AddressingMode::BlockMove, FlowKind::Normal),
    op("eor", AddressingMode::Direct, FlowKind::Normal),
    op("lsr", AddressingMode::Direct, FlowKind::Normal),
    op("eor", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("pha", AddressingMode::Implied, FlowKind::Normal),
    op("eor", AddressingMode::ImmediateM, FlowKind::Normal),
    op("lsr", AddressingMode::Accumulator, FlowKind::Normal),
    op("phk", AddressingMode::Implied, FlowKind::Normal),
    op("jmp", AddressingMode::Absolute, FlowKind::JumpAbs),
    op("eor", AddressingMode::Absolute, FlowKind::Normal),
    op("lsr", AddressingMode::Absolute, FlowKind::Normal),
    op("eor", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bvc", AddressingMode::Relative8, FlowKind::BranchCond),
    op("eor", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("eor", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "eor",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("mvn", AddressingMode::BlockMove, FlowKind::Normal),
    op("eor", AddressingMode::DirectX, FlowKind::Normal),
    op("lsr", AddressingMode::DirectX, FlowKind::Normal),
    op("eor", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("cli", AddressingMode::Implied, FlowKind::Normal),
    op("eor", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("phy", AddressingMode::Implied, FlowKind::Normal),
    op("tcd", AddressingMode::Implied, FlowKind::Normal),
    op("jml", AddressingMode::AbsoluteLong, FlowKind::JumpLong),
    op("eor", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("lsr", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("eor", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("rts", AddressingMode::Implied, FlowKind::Return),
    op("adc", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("per", AddressingMode::Relative16, FlowKind::Normal),
    op("adc", AddressingMode::StackRelative, FlowKind::Normal),
    op("stz", AddressingMode::Direct, FlowKind::Normal),
    op("adc", AddressingMode::Direct, FlowKind::Normal),
    op("ror", AddressingMode::Direct, FlowKind::Normal),
    op("adc", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("pla", AddressingMode::Implied, FlowKind::Normal),
    op("adc", AddressingMode::ImmediateM, FlowKind::Normal),
    op("ror", AddressingMode::Accumulator, FlowKind::Normal),
    op("rtl", AddressingMode::Implied, FlowKind::Return),
    op(
        "jmp",
        AddressingMode::AbsoluteIndirect,
        FlowKind::JumpIndirect,
    ),
    op("adc", AddressingMode::Absolute, FlowKind::Normal),
    op("ror", AddressingMode::Absolute, FlowKind::Normal),
    op("adc", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bvs", AddressingMode::Relative8, FlowKind::BranchCond),
    op("adc", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("adc", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "adc",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("stz", AddressingMode::DirectX, FlowKind::Normal),
    op("adc", AddressingMode::DirectX, FlowKind::Normal),
    op("ror", AddressingMode::DirectX, FlowKind::Normal),
    op("adc", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("sei", AddressingMode::Implied, FlowKind::Normal),
    op("adc", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("ply", AddressingMode::Implied, FlowKind::Normal),
    op("tdc", AddressingMode::Implied, FlowKind::Normal),
    op(
        "jmp",
        AddressingMode::AbsoluteIndexedIndirect,
        FlowKind::JumpIndirect,
    ),
    op("adc", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("ror", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("adc", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("bra", AddressingMode::Relative8, FlowKind::BranchAlways),
    op("sta", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("brl", AddressingMode::Relative16, FlowKind::BranchAlways),
    op("sta", AddressingMode::StackRelative, FlowKind::Normal),
    op("sty", AddressingMode::Direct, FlowKind::Normal),
    op("sta", AddressingMode::Direct, FlowKind::Normal),
    op("stx", AddressingMode::Direct, FlowKind::Normal),
    op("sta", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("dey", AddressingMode::Implied, FlowKind::Normal),
    op("bit", AddressingMode::ImmediateM, FlowKind::Normal),
    op("txa", AddressingMode::Implied, FlowKind::Normal),
    op("phb", AddressingMode::Implied, FlowKind::Normal),
    op("sty", AddressingMode::Absolute, FlowKind::Normal),
    op("sta", AddressingMode::Absolute, FlowKind::Normal),
    op("stx", AddressingMode::Absolute, FlowKind::Normal),
    op("sta", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bcc", AddressingMode::Relative8, FlowKind::BranchCond),
    op("sta", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("sta", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "sta",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("sty", AddressingMode::DirectX, FlowKind::Normal),
    op("sta", AddressingMode::DirectX, FlowKind::Normal),
    op("stx", AddressingMode::DirectY, FlowKind::Normal),
    op("sta", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("tya", AddressingMode::Implied, FlowKind::Normal),
    op("sta", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("txs", AddressingMode::Implied, FlowKind::Normal),
    op("txy", AddressingMode::Implied, FlowKind::Normal),
    op("stz", AddressingMode::Absolute, FlowKind::Normal),
    op("sta", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("stz", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("sta", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("ldy", AddressingMode::ImmediateX, FlowKind::Normal),
    op("lda", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("ldx", AddressingMode::ImmediateX, FlowKind::Normal),
    op("lda", AddressingMode::StackRelative, FlowKind::Normal),
    op("ldy", AddressingMode::Direct, FlowKind::Normal),
    op("lda", AddressingMode::Direct, FlowKind::Normal),
    op("ldx", AddressingMode::Direct, FlowKind::Normal),
    op("lda", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("tay", AddressingMode::Implied, FlowKind::Normal),
    op("lda", AddressingMode::ImmediateM, FlowKind::Normal),
    op("tax", AddressingMode::Implied, FlowKind::Normal),
    op("plb", AddressingMode::Implied, FlowKind::Normal),
    op("ldy", AddressingMode::Absolute, FlowKind::Normal),
    op("lda", AddressingMode::Absolute, FlowKind::Normal),
    op("ldx", AddressingMode::Absolute, FlowKind::Normal),
    op("lda", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bcs", AddressingMode::Relative8, FlowKind::BranchCond),
    op("lda", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("lda", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "lda",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("ldy", AddressingMode::DirectX, FlowKind::Normal),
    op("lda", AddressingMode::DirectX, FlowKind::Normal),
    op("ldx", AddressingMode::DirectY, FlowKind::Normal),
    op("lda", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("clv", AddressingMode::Implied, FlowKind::Normal),
    op("lda", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("tsx", AddressingMode::Implied, FlowKind::Normal),
    op("tyx", AddressingMode::Implied, FlowKind::Normal),
    op("ldy", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("lda", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("ldx", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("lda", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("cpy", AddressingMode::ImmediateX, FlowKind::Normal),
    op("cmp", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("rep", AddressingMode::Immediate8, FlowKind::Normal),
    op("cmp", AddressingMode::StackRelative, FlowKind::Normal),
    op("cpy", AddressingMode::Direct, FlowKind::Normal),
    op("cmp", AddressingMode::Direct, FlowKind::Normal),
    op("dec", AddressingMode::Direct, FlowKind::Normal),
    op("cmp", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("iny", AddressingMode::Implied, FlowKind::Normal),
    op("cmp", AddressingMode::ImmediateM, FlowKind::Normal),
    op("dex", AddressingMode::Implied, FlowKind::Normal),
    op("wai", AddressingMode::Implied, FlowKind::Stop),
    op("cpy", AddressingMode::Absolute, FlowKind::Normal),
    op("cmp", AddressingMode::Absolute, FlowKind::Normal),
    op("dec", AddressingMode::Absolute, FlowKind::Normal),
    op("cmp", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("bne", AddressingMode::Relative8, FlowKind::BranchCond),
    op("cmp", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("cmp", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "cmp",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("pei", AddressingMode::Direct, FlowKind::Normal),
    op("cmp", AddressingMode::DirectX, FlowKind::Normal),
    op("dec", AddressingMode::DirectX, FlowKind::Normal),
    op("cmp", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("cld", AddressingMode::Implied, FlowKind::Normal),
    op("cmp", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("phx", AddressingMode::Implied, FlowKind::Normal),
    op("stp", AddressingMode::Implied, FlowKind::Stop),
    op(
        "jmp",
        AddressingMode::AbsoluteIndirectLong,
        FlowKind::JumpIndirect,
    ),
    op("cmp", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("dec", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("cmp", AddressingMode::AbsoluteLongX, FlowKind::Normal),
    op("cpx", AddressingMode::ImmediateX, FlowKind::Normal),
    op("sbc", AddressingMode::DirectIndirectX, FlowKind::Normal),
    op("sep", AddressingMode::Immediate8, FlowKind::Normal),
    op("sbc", AddressingMode::StackRelative, FlowKind::Normal),
    op("cpx", AddressingMode::Direct, FlowKind::Normal),
    op("sbc", AddressingMode::Direct, FlowKind::Normal),
    op("inc", AddressingMode::Direct, FlowKind::Normal),
    op("sbc", AddressingMode::DirectIndirectLong, FlowKind::Normal),
    op("inx", AddressingMode::Implied, FlowKind::Normal),
    op("sbc", AddressingMode::ImmediateM, FlowKind::Normal),
    op("nop", AddressingMode::Implied, FlowKind::Normal),
    op("xba", AddressingMode::Implied, FlowKind::Normal),
    op("cpx", AddressingMode::Absolute, FlowKind::Normal),
    op("sbc", AddressingMode::Absolute, FlowKind::Normal),
    op("inc", AddressingMode::Absolute, FlowKind::Normal),
    op("sbc", AddressingMode::AbsoluteLong, FlowKind::Normal),
    op("beq", AddressingMode::Relative8, FlowKind::BranchCond),
    op("sbc", AddressingMode::DirectIndirectY, FlowKind::Normal),
    op("sbc", AddressingMode::DirectIndirect, FlowKind::Normal),
    op(
        "sbc",
        AddressingMode::StackRelativeIndirectY,
        FlowKind::Normal,
    ),
    op("pea", AddressingMode::Absolute, FlowKind::Normal),
    op("sbc", AddressingMode::DirectX, FlowKind::Normal),
    op("inc", AddressingMode::DirectX, FlowKind::Normal),
    op("sbc", AddressingMode::DirectIndirectLongY, FlowKind::Normal),
    op("sed", AddressingMode::Implied, FlowKind::Normal),
    op("sbc", AddressingMode::AbsoluteY, FlowKind::Normal),
    op("plx", AddressingMode::Implied, FlowKind::Normal),
    op("xce", AddressingMode::Implied, FlowKind::Normal),
    op(
        "jsr",
        AddressingMode::AbsoluteIndexedIndirect,
        FlowKind::CallIndirect,
    ),
    op("sbc", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("inc", AddressingMode::AbsoluteX, FlowKind::Normal),
    op("sbc", AddressingMode::AbsoluteLongX, FlowKind::Normal),
];

pub fn analyze_rom(info: &RomInfo, rom: &[u8]) -> DisassemblyResult {
    let mut context = TraversalContext {
        instructions: BTreeMap::new(),
        labels: BTreeMap::new(),
        cfg_edges: Vec::new(),
        warnings: info.warnings.clone(),
        state_at_pc: HashMap::new(),
    };
    let mut queue = VecDeque::new();
    let mut seen_entry = BTreeSet::new();

    for (label, _, pc_opt, addr) in vector_targets(info) {
        if let Some(pc) = pc_opt {
            context.labels.entry(pc).or_insert(label);
            if seen_entry.insert(pc) {
                queue.push_back((pc, DecodeState::reset_state()));
            }
        } else {
            context.warnings.push(format!(
                "vector target {} at {} does not map into ROM",
                label,
                addr.format_snes()
            ));
        }
    }

    traverse_worklist(rom, &mut context, &mut queue);

    let mut jump_tables = Vec::new();
    loop {
        let analyses = analyze_indirect_transfers(&context.instructions, rom, info.size);
        let mut seeded_any = false;
        for analysis in analyses {
            if let Some(candidate) = analysis.jump_table.clone() {
                if !jump_tables
                    .iter()
                    .any(|existing: &JumpTableCandidate| existing.source_pc == candidate.source_pc)
                {
                    context.labels.entry(candidate.table_pc).or_insert_with(|| {
                        let table_addr = pc_to_lorom(candidate.table_pc);
                        format!("jtbl_{:02X}_{:04X}", table_addr.bank, table_addr.addr)
                    });
                    jump_tables.push(candidate);
                }
            }
            if !analysis.targets.is_empty() {
                let label_kind = if analysis.source_mnemonic == "jsr" {
                    LabelKind::Subroutine
                } else {
                    LabelKind::Location
                };
                for target in analysis.targets {
                    let seeded = seed_discovered_target(
                        &mut context,
                        &mut queue,
                        target,
                        label_kind,
                        &analysis.state_for_targets,
                    );
                    seeded_any |= seeded;
                }
            }
        }
        if !seeded_any {
            break;
        }
        traverse_worklist(rom, &mut context, &mut queue);
    }

    let mut unresolved_transfers = Vec::new();
    for analysis in analyze_indirect_transfers(&context.instructions, rom, info.size) {
        if !analysis.targets.is_empty() {
            let edge_type = if analysis.source_mnemonic == "jsr" {
                "call"
            } else {
                "jump"
            };
            let label_kind = if analysis.source_mnemonic == "jsr" {
                LabelKind::Subroutine
            } else {
                LabelKind::Location
            };
            for target in analysis.targets {
                context.cfg_edges.push(CfgEdge {
                    from_pc: analysis.source_pc,
                    to_pc: Some(target),
                    edge_type: edge_type.to_string(),
                });
                context.labels.entry(target).or_insert_with(|| {
                    let addr = pc_to_lorom(target);
                    match label_kind {
                        LabelKind::Location => format!("loc_{:02X}_{:04X}", addr.bank, addr.addr),
                        LabelKind::Subroutine => format!("sub_{:02X}_{:04X}", addr.bank, addr.addr),
                    }
                });
            }
        } else {
            context.cfg_edges.push(CfgEdge {
                from_pc: analysis.source_pc,
                to_pc: None,
                edge_type: "unresolved_indirect".to_string(),
            });
            unresolved_transfers.push(format!(
                "{} {} {}",
                pc_to_lorom(analysis.source_pc).format_snes(),
                analysis.source_mnemonic,
                analysis
                    .unresolved_reason
                    .unwrap_or_else(|| "unresolved indirect transfer".to_string())
            ));
        }
    }

    let mut classification = vec!["unknown".to_string(); info.size];
    for index in crate::rommap::header_region(info) {
        classification[index] = "header".to_string();
    }
    for index in crate::rommap::vector_region(info) {
        classification[index] = "vector".to_string();
    }
    for instruction in context.instructions.values() {
        for index in
            instruction.pc_offset..(instruction.pc_offset + instruction.length).min(info.size)
        {
            classification[index] = "code".to_string();
        }
    }
    for candidate in &jump_tables {
        for index in candidate.table_pc
            ..(candidate.table_pc + candidate.targets.len() * candidate.entry_width).min(info.size)
        {
            if classification[index] == "unknown" {
                classification[index] = "jump_table".to_string();
            }
        }
    }
    for reference_pc in collect_rom_data_references(&context.instructions, info.size) {
        if classification[reference_pc] == "unknown" {
            classification[reference_pc] = "referenced_data".to_string();
        }
    }

    let data_regions = collect_data_regions(&classification);
    let blocks = build_basic_blocks(&context.instructions, &context.cfg_edges);
    let subroutines = context
        .labels
        .values()
        .filter(|name| name.starts_with("sub_"))
        .count();
    let reachable_code_bytes = classification
        .iter()
        .filter(|item| item.as_str() == "code")
        .count();
    let untouched_bytes = classification
        .iter()
        .filter(|item| item.as_str() == "unknown")
        .count();
    let unresolved_indirect_jumps = context
        .cfg_edges
        .iter()
        .filter(|edge| edge.edge_type == "unresolved_indirect")
        .count();

    let basic_blocks_count = blocks.len();
    DisassemblyResult {
        instructions: context.instructions,
        blocks,
        cfg_edges: context.cfg_edges,
        labels: context.labels,
        jump_tables,
        data_regions,
        counts: AnalysisCounts {
            reachable_code_bytes,
            untouched_bytes,
            basic_blocks: basic_blocks_count,
            subroutines,
            unresolved_indirect_jumps,
        },
        warnings: context.warnings,
        classification,
        unresolved_transfers,
    }
}

#[derive(Clone, Copy)]
enum LabelKind {
    Location,
    Subroutine,
}

fn traverse_worklist(
    rom: &[u8],
    context: &mut TraversalContext,
    queue: &mut VecDeque<(usize, DecodeState)>,
) {
    while let Some((start_pc, state)) = queue.pop_front() {
        let mut pc = start_pc;
        let mut state_here = state.clone();
        loop {
            if pc >= rom.len() {
                context
                    .warnings
                    .push(format!("decode reached out-of-ROM pc {}", format_pc(pc)));
                break;
            }
            if let Some(previous) = context.state_at_pc.get(&pc) {
                let merged = previous.merge(&state_here);
                if &merged == previous {
                    break;
                }
                context.warnings.push(format!(
                    "state merge at {} from {:?} and {:?} -> {:?}",
                    format_pc(pc),
                    previous,
                    state_here,
                    merged
                ));
                context.state_at_pc.insert(pc, merged.clone());
                queue.push_back((pc, merged));
                break;
            }

            let decoded = decode_instruction(rom, pc, &state_here);
            let flow = decoded.flow_type;
            context.state_at_pc.insert(pc, state_here.clone());

            if let Some(target) = decoded.target_pc {
                match flow {
                    FlowType::Branch => {
                        let edge_type = if decoded.mnemonic == "bra" || decoded.mnemonic == "brl" {
                            "jump"
                        } else {
                            "branch_taken"
                        };
                        let _ = record_flow_target(
                            context,
                            queue,
                            pc,
                            target,
                            edge_type,
                            LabelKind::Location,
                            &state_here,
                        );
                    }
                    FlowType::Call => {
                        let _ = record_flow_target(
                            context,
                            queue,
                            pc,
                            target,
                            "call",
                            LabelKind::Subroutine,
                            &state_here,
                        );
                    }
                    FlowType::Jump => {
                        let _ = record_flow_target(
                            context,
                            queue,
                            pc,
                            target,
                            "jump",
                            LabelKind::Location,
                            &state_here,
                        );
                    }
                    _ => {}
                }
            }

            if let Some(fallthrough) = decoded.fallthrough_pc {
                if flow == FlowType::Branch {
                    context.cfg_edges.push(CfgEdge {
                        from_pc: pc,
                        to_pc: Some(fallthrough),
                        edge_type: "fallthrough".to_string(),
                    });
                }
                state_here = decoded.state_out.clone().unwrap_or(state_here);
                context.instructions.insert(pc, decoded);
                pc = fallthrough;
                continue;
            }

            context.instructions.insert(pc, decoded);
            break;
        }
    }
}

fn record_flow_target(
    context: &mut TraversalContext,
    queue: &mut VecDeque<(usize, DecodeState)>,
    source_pc: usize,
    target: usize,
    edge_type: &str,
    label_kind: LabelKind,
    state: &DecodeState,
) -> bool {
    context.cfg_edges.push(CfgEdge {
        from_pc: source_pc,
        to_pc: Some(target),
        edge_type: edge_type.to_string(),
    });
    context.labels.entry(target).or_insert_with(|| {
        let addr = pc_to_lorom(target);
        match label_kind {
            LabelKind::Location => format!("loc_{:02X}_{:04X}", addr.bank, addr.addr),
            LabelKind::Subroutine => format!("sub_{:02X}_{:04X}", addr.bank, addr.addr),
        }
    });
    seed_or_merge_target(context, queue, target, state)
}

fn seed_discovered_target(
    context: &mut TraversalContext,
    queue: &mut VecDeque<(usize, DecodeState)>,
    target: usize,
    label_kind: LabelKind,
    state: &DecodeState,
) -> bool {
    context.labels.entry(target).or_insert_with(|| {
        let addr = pc_to_lorom(target);
        match label_kind {
            LabelKind::Location => format!("loc_{:02X}_{:04X}", addr.bank, addr.addr),
            LabelKind::Subroutine => format!("sub_{:02X}_{:04X}", addr.bank, addr.addr),
        }
    });
    seed_or_merge_target(context, queue, target, state)
}

fn seed_or_merge_target(
    context: &mut TraversalContext,
    queue: &mut VecDeque<(usize, DecodeState)>,
    target: usize,
    state: &DecodeState,
) -> bool {
    if let Some(previous) = context.state_at_pc.get(&target) {
        let merged = merge_states(previous, state);
        if &merged == previous {
            return false;
        }
        context.warnings.push(format!(
            "state merge at {} from {:?} and {:?} -> {:?}",
            format_pc(target),
            previous,
            state,
            merged
        ));
        context.state_at_pc.insert(target, merged.clone());
        queue.push_back((target, merged));
        return true;
    }
    queue.push_back((target, state.clone()));
    true
}

fn merge_states(existing: &DecodeState, incoming: &DecodeState) -> DecodeState {
    existing.merge(incoming)
}

fn merge_flag(left: Option<bool>, right: Option<bool>) -> Option<bool> {
    match (left, right) {
        (Some(a), Some(b)) if a == b => Some(a),
        (Some(_), Some(_)) => None,
        (None, _) | (_, None) => None,
    }
}

pub fn decode_instruction(rom: &[u8], pc: usize, state: &DecodeState) -> Instruction {
    let opcode = rom[pc];
    let meta = OPCODES[opcode as usize];
    let mut notes = Vec::new();
    let (operand_len, confidence) = operand_len(meta.mode, state, &mut notes);
    let length = 1 + operand_len;
    let end = (pc + length).min(rom.len());
    let bytes_ = rom[pc..end].to_vec();
    let addr = pc_to_lorom(pc);
    let operand_bytes = &rom[(pc + 1).min(rom.len())..end];
    let operand_text = format_operand(meta.mode, operand_bytes, pc, rom.len(), state);
    let mut state_out = Some(state.clone());

    if meta.mnemonic == "rep" && operand_bytes.len() == 1 {
        if let Some(next) = state_out.as_mut() {
            apply_rep_sep(next, operand_bytes[0], false);
        }
    }
    if meta.mnemonic == "sep" && operand_bytes.len() == 1 {
        if let Some(next) = state_out.as_mut() {
            apply_rep_sep(next, operand_bytes[0], true);
        }
    }
    if meta.mnemonic == "xce" {
        if let Some(next) = state_out.as_mut() {
            next.emulation = None;
            next.m_flag = None;
            next.x_flag = None;
            *next = next.clone().normalized();
        }
        notes.push("XCE makes mode width ambiguous without carry tracking".to_string());
    }

    let (flow_type, target_pc, fallthrough_pc) =
        compute_flow(meta, pc, operand_bytes, rom.len(), &operand_text);

    Instruction {
        pc_offset: pc,
        snes_bank: addr.bank,
        snes_addr: addr.addr,
        bytes_,
        mnemonic: meta.mnemonic.to_string(),
        operand: operand_text,
        length,
        flow_type,
        target_pc,
        fallthrough_pc,
        state_in: state.clone(),
        state_out,
        confidence: confidence.to_string(),
        notes,
    }
}

fn operand_len(
    mode: AddressingMode,
    state: &DecodeState,
    notes: &mut Vec<String>,
) -> (usize, &'static str) {
    match mode {
        AddressingMode::Implied | AddressingMode::Accumulator => (0, "high"),
        AddressingMode::Immediate8
        | AddressingMode::Direct
        | AddressingMode::DirectX
        | AddressingMode::DirectY
        | AddressingMode::DirectIndirect
        | AddressingMode::DirectIndirectLong
        | AddressingMode::DirectIndirectX
        | AddressingMode::DirectIndirectY
        | AddressingMode::DirectIndirectLongY
        | AddressingMode::StackRelative
        | AddressingMode::StackRelativeIndirectY
        | AddressingMode::Relative8 => (1, "high"),
        AddressingMode::ImmediateM => match state.accumulator_is_8bit() {
            Some(true) => (1, "high"),
            Some(false) => (2, "high"),
            None => {
                notes.push("ambiguous accumulator immediate width; assumed 8-bit".to_string());
                (1, "low")
            }
        },
        AddressingMode::ImmediateX => match state.index_is_8bit() {
            Some(true) => (1, "high"),
            Some(false) => (2, "high"),
            None => {
                notes.push("ambiguous index immediate width; assumed 8-bit".to_string());
                (1, "low")
            }
        },
        AddressingMode::Absolute
        | AddressingMode::AbsoluteX
        | AddressingMode::AbsoluteY
        | AddressingMode::AbsoluteIndirect
        | AddressingMode::AbsoluteIndexedIndirect
        | AddressingMode::AbsoluteIndirectLong
        | AddressingMode::Relative16 => (2, "high"),
        AddressingMode::AbsoluteLong
        | AddressingMode::AbsoluteLongX
        | AddressingMode::BlockMove => (3, "high"),
    }
}

fn format_operand(
    mode: AddressingMode,
    operand: &[u8],
    pc: usize,
    rom_size: usize,
    _state: &DecodeState,
) -> String {
    let word = || {
        operand
            .get(0..2)
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .unwrap_or(0)
    };
    let long = || {
        if operand.len() >= 3 {
            (operand[2], u16::from_le_bytes([operand[0], operand[1]]))
        } else {
            (0, 0)
        }
    };
    match mode {
        AddressingMode::Implied => String::new(),
        AddressingMode::Accumulator => "a".to_string(),
        AddressingMode::Immediate8 | AddressingMode::ImmediateM | AddressingMode::ImmediateX => {
            if operand.len() == 2 {
                format!("#${:04X}", word())
            } else {
                format!("#${:02X}", operand.first().copied().unwrap_or(0))
            }
        }
        AddressingMode::Direct => format!("${:02X}", operand.first().copied().unwrap_or(0)),
        AddressingMode::DirectX => format!("${:02X},x", operand.first().copied().unwrap_or(0)),
        AddressingMode::DirectY => format!("${:02X},y", operand.first().copied().unwrap_or(0)),
        AddressingMode::DirectIndirect => {
            format!("(${:02X})", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::DirectIndirectLong => {
            format!("[{:02X}]", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::DirectIndirectX => {
            format!("(${:02X},x)", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::DirectIndirectY => {
            format!("(${:02X}),y", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::DirectIndirectLongY => {
            format!("[{:02X}],y", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::StackRelative => {
            format!("${:02X},s", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::StackRelativeIndirectY => {
            format!("(${:02X},s),y", operand.first().copied().unwrap_or(0))
        }
        AddressingMode::Absolute => format!("${:04X}", word()),
        AddressingMode::AbsoluteX => format!("${:04X},x", word()),
        AddressingMode::AbsoluteY => format!("${:04X},y", word()),
        AddressingMode::AbsoluteLong => {
            let (bank, addr) = long();
            format!("${bank:02X}:{addr:04X}")
        }
        AddressingMode::AbsoluteLongX => {
            let (bank, addr) = long();
            format!("${bank:02X}:{addr:04X},x")
        }
        AddressingMode::AbsoluteIndirect => format!("(${:04X})", word()),
        AddressingMode::AbsoluteIndexedIndirect => format!("(${:04X},x)", word()),
        AddressingMode::AbsoluteIndirectLong => format!("[{:04X}]", word()),
        AddressingMode::Relative8 => {
            let disp = operand.first().copied().unwrap_or(0) as i8 as i32;
            let target = (pc as i32 + 2 + disp) as usize;
            if target < rom_size {
                format!(
                    "{} ; {}",
                    format!("${:04X}", target as u16),
                    pc_to_lorom(target).format_snes()
                )
            } else {
                format!("${:02X}", operand.first().copied().unwrap_or(0))
            }
        }
        AddressingMode::Relative16 => {
            let disp = word() as i16 as i32;
            let target = (pc as i32 + 3 + disp) as usize;
            if target < rom_size {
                format!(
                    "{} ; {}",
                    format!("${:04X}", target as u16),
                    pc_to_lorom(target).format_snes()
                )
            } else {
                format!("${:04X}", word())
            }
        }
        AddressingMode::BlockMove => {
            if operand.len() >= 2 {
                format!("${:02X},${:02X}", operand[0], operand[1])
            } else {
                String::new()
            }
        }
    }
}

fn compute_flow(
    meta: OpcodeMeta,
    pc: usize,
    operand: &[u8],
    rom_size: usize,
    operand_text: &str,
) -> (FlowType, Option<usize>, Option<usize>) {
    let next_pc = pc + 1 + operand.len();
    let target_from_word = |addr: u16| snes_to_lorom(pc_to_lorom(pc).bank, addr, rom_size);
    let target_from_long = |bank: u8, addr: u16| snes_to_lorom(bank, addr, rom_size);
    let word = || {
        operand
            .get(0..2)
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .unwrap_or(0)
    };
    let long = || {
        if operand.len() >= 3 {
            (operand[2], u16::from_le_bytes([operand[0], operand[1]]))
        } else {
            (0, 0)
        }
    };
    match meta.flow {
        FlowKind::Normal => (FlowType::Fallthrough, None, Some(next_pc)),
        FlowKind::Return => (FlowType::Return, None, None),
        FlowKind::Stop => (FlowType::Stop, None, None),
        FlowKind::BranchCond => {
            let disp = operand.first().copied().unwrap_or(0) as i8 as i32;
            let target = (pc as i32 + 2 + disp) as usize;
            (
                FlowType::Branch,
                (target < rom_size).then_some(target),
                Some(next_pc),
            )
        }
        FlowKind::BranchAlways => {
            let target = if meta.mode == AddressingMode::Relative16 {
                let disp = word() as i16 as i32;
                (pc as i32 + 3 + disp) as usize
            } else {
                let disp = operand.first().copied().unwrap_or(0) as i8 as i32;
                (pc as i32 + 2 + disp) as usize
            };
            (
                FlowType::Branch,
                (target < rom_size).then_some(target),
                None,
            )
        }
        FlowKind::JumpAbs => (FlowType::Jump, target_from_word(word()), None),
        FlowKind::JumpLong => {
            let (bank, addr) = long();
            (FlowType::Jump, target_from_long(bank, addr), None)
        }
        FlowKind::JumpIndirect => {
            let _ = operand_text;
            (FlowType::Jump, None, None)
        }
        FlowKind::CallAbs => (FlowType::Call, target_from_word(word()), Some(next_pc)),
        FlowKind::CallLong => {
            let (bank, addr) = long();
            (FlowType::Call, target_from_long(bank, addr), Some(next_pc))
        }
        FlowKind::CallIndirect => (FlowType::Call, None, Some(next_pc)),
    }
}

fn apply_rep_sep(state: &mut DecodeState, operand: u8, set: bool) {
    if state.emulation == Some(true) && !set {
        if operand & 0x10 != 0 {
            state.x_flag = Some(true);
        }
        if operand & 0x20 != 0 {
            state.m_flag = Some(true);
        }
        *state = state.clone().normalized();
        return;
    }
    if operand & 0x10 != 0 {
        state.x_flag = Some(set);
    }
    if operand & 0x20 != 0 {
        state.m_flag = Some(set);
    }
    *state = state.clone().normalized();
}

fn analyze_indirect_transfers(
    instructions: &BTreeMap<usize, Instruction>,
    rom: &[u8],
    rom_size: usize,
) -> Vec<IndirectAnalysis> {
    let mut result = Vec::new();
    let ordered = instructions.keys().copied().collect::<Vec<_>>();
    for (index, pc) in ordered.iter().copied().enumerate() {
        let instruction = &instructions[&pc];
        let is_indirect = matches!(instruction.mnemonic.as_str(), "jmp" | "jsr")
            && (instruction.operand.contains('(') || instruction.operand.contains('['));
        if !is_indirect {
            continue;
        }

        let state_for_targets = instruction
            .state_out
            .clone()
            .unwrap_or_else(|| instruction.state_in.clone());
        let mut analysis = IndirectAnalysis {
            source_pc: pc,
            source_mnemonic: instruction.mnemonic.clone(),
            targets: Vec::new(),
            jump_table: None,
            unresolved_reason: Some("unresolved indirect transfer".to_string()),
            state_for_targets,
        };

        if let Some(base) = parse_absolute_operand(&instruction.operand) {
            if instruction.operand.starts_with("($") && instruction.operand.ends_with(",x)") {
                if let Some(candidate) = detect_indexed_jump_table(
                    &ordered,
                    index,
                    instructions,
                    rom,
                    rom_size,
                    pc,
                    base,
                ) {
                    analysis.targets = candidate.targets.clone();
                    analysis.jump_table = Some(candidate);
                    analysis.unresolved_reason = None;
                }
            } else if instruction.operand.starts_with("($") && instruction.operand.ends_with(')') {
                if let Some(target) =
                    resolve_indirect_pointer_same_bank(instruction.snes_bank, base, rom, rom_size)
                {
                    analysis.targets.push(target);
                    analysis.unresolved_reason = None;
                }
            } else if instruction.operand.starts_with("[$") && instruction.operand.ends_with(']') {
                if let Some(target) = resolve_indirect_pointer_long(base, rom, rom_size) {
                    analysis.targets.push(target);
                    analysis.unresolved_reason = None;
                }
            }
        }

        result.push(analysis);
    }
    result
}

fn detect_indexed_jump_table(
    ordered: &[usize],
    index: usize,
    instructions: &BTreeMap<usize, Instruction>,
    rom: &[u8],
    rom_size: usize,
    source_pc: usize,
    base: u16,
) -> Option<JumpTableCandidate> {
    let source = &instructions[&source_pc];
    let bank = source.snes_bank;
    let table_pc = snes_to_lorom(bank, base, rom_size)?;
    if !has_index_dispatch_setup(ordered, index, instructions) {
        return None;
    }

    let mut targets = Vec::new();
    for slot in 0..16usize {
        let entry_pc = table_pc + slot * 2;
        if entry_pc + 1 >= rom.len() {
            break;
        }
        let addr = u16::from_le_bytes([rom[entry_pc], rom[entry_pc + 1]]);
        if addr < 0x8000 {
            break;
        }
        let Some(target) = snes_to_lorom(bank, addr, rom_size) else {
            break;
        };
        if !looks_like_code_start(rom, target) {
            break;
        }
        targets.push(target);
    }

    if targets.len() < 2 {
        return None;
    }

    Some(JumpTableCandidate {
        source_pc,
        table_pc,
        table_addr: pc_to_lorom(table_pc),
        entry_width: 2,
        confidence: if targets.len() >= 4 { "medium" } else { "low" }.to_string(),
        targets,
    })
}

fn has_index_dispatch_setup(
    ordered: &[usize],
    index: usize,
    instructions: &BTreeMap<usize, Instruction>,
) -> bool {
    if index == 0 {
        return false;
    }
    let prev = &instructions[&ordered[index - 1]];
    if prev.mnemonic == "tax" {
        return true;
    }
    if index >= 2 {
        let prev2 = &instructions[&ordered[index - 2]];
        return prev.mnemonic == "tax" && prev2.mnemonic == "asl" && prev2.operand == "a";
    }
    false
}

fn resolve_indirect_pointer_same_bank(
    bank: u8,
    base: u16,
    rom: &[u8],
    rom_size: usize,
) -> Option<usize> {
    let pointer_pc = snes_to_lorom(bank, base, rom_size)?;
    if pointer_pc + 1 >= rom.len() {
        return None;
    }
    let addr = u16::from_le_bytes([rom[pointer_pc], rom[pointer_pc + 1]]);
    let target = snes_to_lorom(bank, addr, rom_size)?;
    looks_like_code_start(rom, target).then_some(target)
}

fn resolve_indirect_pointer_long(base: u16, rom: &[u8], rom_size: usize) -> Option<usize> {
    let pointer_pc = snes_to_lorom(0x80, base, rom_size)?;
    if pointer_pc + 2 >= rom.len() {
        return None;
    }
    let bank = rom[pointer_pc + 2];
    let addr = u16::from_le_bytes([rom[pointer_pc], rom[pointer_pc + 1]]);
    let target = snes_to_lorom(bank, addr, rom_size)?;
    looks_like_code_start(rom, target).then_some(target)
}

fn looks_like_code_start(rom: &[u8], pc: usize) -> bool {
    rom.get(pc)
        .is_some_and(|opcode| !matches!(*opcode, 0x00 | 0x02 | 0x42 | 0xDB | 0xFF))
}

fn collect_rom_data_references(
    instructions: &BTreeMap<usize, Instruction>,
    rom_size: usize,
) -> BTreeSet<usize> {
    let mut refs = BTreeSet::new();
    for instruction in instructions.values() {
        let Some(opcode) = instruction.bytes_.first().copied() else {
            continue;
        };
        let operand = &instruction.bytes_[1..];
        let meta = OPCODES[opcode as usize];
        match meta.mode {
            AddressingMode::Absolute
            | AddressingMode::AbsoluteX
            | AddressingMode::AbsoluteY
            | AddressingMode::AbsoluteIndirect
            | AddressingMode::AbsoluteIndexedIndirect
            | AddressingMode::AbsoluteIndirectLong => {
                if operand.len() >= 2 {
                    let addr = u16::from_le_bytes([operand[0], operand[1]]);
                    if let Some(pc) = snes_to_lorom(instruction.snes_bank, addr, rom_size) {
                        refs.insert(pc);
                    }
                }
            }
            AddressingMode::AbsoluteLong | AddressingMode::AbsoluteLongX => {
                if operand.len() >= 3 {
                    let bank = operand[2];
                    let addr = u16::from_le_bytes([operand[0], operand[1]]);
                    if let Some(pc) = snes_to_lorom(bank, addr, rom_size) {
                        refs.insert(pc);
                    }
                }
            }
            _ => {}
        }
    }
    refs
}

fn parse_absolute_operand(text: &str) -> Option<u16> {
    let cleaned = text
        .trim_start_matches('(')
        .trim_start_matches('[')
        .trim_end_matches(",x)")
        .trim_end_matches(']')
        .trim_end_matches(')');
    cleaned
        .strip_prefix('$')
        .and_then(|x| u16::from_str_radix(x, 16).ok())
}

fn build_basic_blocks(
    instructions: &BTreeMap<usize, Instruction>,
    cfg_edges: &[CfgEdge],
) -> Vec<BasicBlock> {
    let mut leaders = BTreeSet::new();
    if let Some(first) = instructions.keys().next() {
        leaders.insert(*first);
    }
    for edge in cfg_edges {
        leaders.insert(edge.from_pc);
        if let Some(target) = edge.to_pc {
            leaders.insert(target);
        }
        if edge.edge_type == "fallthrough" {
            if let Some(target) = edge.to_pc {
                leaders.insert(target);
            }
        }
    }
    let ordered = instructions.keys().copied().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < ordered.len() {
        let start = ordered[index];
        if !leaders.contains(&start) {
            index += 1;
            continue;
        }
        let mut cursor = index;
        while cursor + 1 < ordered.len() {
            let next = ordered[cursor + 1];
            if leaders.contains(&next) {
                break;
            }
            if instructions[&ordered[cursor]].fallthrough_pc != Some(next) {
                break;
            }
            cursor += 1;
        }
        let end = ordered[cursor];
        let outgoing_edges = cfg_edges
            .iter()
            .filter(|edge| edge.from_pc >= start && edge.from_pc <= end)
            .cloned()
            .collect::<Vec<_>>();
        blocks.push(BasicBlock {
            start_pc: start,
            end_pc: end,
            outgoing_edges,
        });
        index = cursor + 1;
    }
    blocks
}

fn collect_data_regions(classification: &[String]) -> Vec<DataRegion> {
    let mut out = Vec::new();
    let mut start = None;
    let mut current_reason = String::new();
    for (index, kind) in classification.iter().enumerate() {
        let is_data = matches!(
            kind.as_str(),
            "jump_table" | "referenced_data" | "unknown"
        );
        if is_data && start.is_none() {
            start = Some(index);
            current_reason = match kind.as_str() {
                "jump_table" => "jump_table".to_string(),
                "referenced_data" => "referenced_data".to_string(),
                _ => "likely_data_or_unknown".to_string(),
            };
        } else if is_data && start.is_some() && !current_reason.is_empty() && kind != &current_reason {
            let begin = start.take().unwrap_or(0);
            if current_reason != "likely_data_or_unknown" || index > begin + 8 {
                out.push(DataRegion {
                    start_pc: begin,
                    end_pc: index - 1,
                    reason: current_reason.clone(),
                });
            }
            start = Some(index);
            current_reason = match kind.as_str() {
                "jump_table" => "jump_table".to_string(),
                "referenced_data" => "referenced_data".to_string(),
                _ => "likely_data_or_unknown".to_string(),
            };
        }
        if !is_data && start.is_some() {
            let begin = start.take().unwrap_or(0);
            if current_reason != "likely_data_or_unknown" || index > begin + 8 {
                out.push(DataRegion {
                    start_pc: begin,
                    end_pc: index - 1,
                    reason: current_reason.clone(),
                });
            }
        }
    }
    if let Some(begin) = start {
        if current_reason != "likely_data_or_unknown" || classification.len() > begin + 8 {
            out.push(DataRegion {
                start_pc: begin,
                end_pc: classification.len() - 1,
                reason: current_reason.clone(),
            });
        }
    }
    out
}
