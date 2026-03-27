use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
pub struct CpuAddress {
    pub bank: u8,
    pub addr: u16,
}

impl CpuAddress {
    pub fn new(bank: u8, addr: u16) -> Self {
        Self { bank, addr }
    }

    pub fn format_snes(self) -> String {
        format!("${:02X}:{:04X}", self.bank, self.addr)
    }
}

pub fn format_pc(pc: usize) -> String {
    format!("0x{pc:06X}")
}

pub fn pc_to_lorom(pc_offset: usize) -> CpuAddress {
    let bank = 0x80u8.wrapping_add((pc_offset / 0x8000) as u8);
    let addr = 0x8000u16 + (pc_offset % 0x8000) as u16;
    CpuAddress { bank, addr }
}

pub fn snes_to_lorom(bank: u8, addr: u16, rom_size: usize) -> Option<usize> {
    if addr < 0x8000 {
        return None;
    }
    if bank == 0x7E || bank == 0x7F {
        return None;
    }
    let bank_index = (bank & 0x7F) as usize;
    let pc = bank_index.checked_mul(0x8000)? + usize::from(addr - 0x8000);
    (pc < rom_size).then_some(pc)
}

pub fn lorom_vector_target_to_pc(vector: u16, rom_size: usize) -> Option<usize> {
    snes_to_lorom(0x80, vector, rom_size)
}
