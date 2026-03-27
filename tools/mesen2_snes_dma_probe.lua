-- PLAN_002: first-pass SNES DMA / asset-path probe for Mesen2
--
-- Intended use:
-- 1. Load a SNES ROM in Mesen2.
-- 2. Open the Script Window.
-- 3. Load and run this script.
--
-- What it does:
-- - hooks CPU writes to DMA/HDMA/PPU/APU upload registers
-- - records current PC and frame when those writes occur
-- - keeps a shadow copy of DMA channel registers
-- - emits a compact summary whenever DMA/HDMA is launched
--
-- This is deliberately conservative and log-oriented.
-- It emits JSON lines so the Rust runtime correlator can resolve PCs against labels.json.

local state = emu.getState()
if state["consoleType"] ~= "Snes" then
  emu.displayMessage("DMA Probe", "This script only works on SNES.")
  return
end

local shadow = {
  frame = -1,
  dma = {},
  current = {},
}

for ch = 0, 7 do
  shadow.dma[ch] = {}
end

local function hex(value, digits)
  return string.format("%0" .. digits .. "X", value & ((1 << (digits * 4)) - 1))
end

local function json_escape(value)
  local s = tostring(value)
  s = s:gsub("\\", "\\\\")
  s = s:gsub("\"", "\\\"")
  return s
end

local function emit_json(fields)
  local parts = {}
  for _, field in ipairs(fields) do
    parts[#parts + 1] = string.format("\"%s\":%s", field[1], field[2])
  end
  emu.log("{" .. table.concat(parts, ",") .. "}")
end

local function get_pc()
  local s = emu.getState()
  if s["cpu.k"] ~= nil and s["cpu.pc"] ~= nil then
    return ((s["cpu.k"] & 0xFF) << 16) | (s["cpu.pc"] & 0xFFFF)
  end
  if s["cpu.pc"] ~= nil then
    return s["cpu.pc"] & 0xFFFFFF
  end
  return 0
end

local function note_write(kind, address, value)
  local s = emu.getState()
  local pc = get_pc()
  local entry = {
    frame = s["frameCount"] or -1,
    scanline = s["ppu.scanline"] or -1,
    pc = pc,
    kind = kind,
    address = address,
    value = value
  }
  table.insert(shadow.current, entry)
  emit_json({
    {"source", "\"mesen2_lua\""},
    {"event", "\"register_write\""},
    {"kind", "\"" .. json_escape(kind) .. "\""},
    {"frame", tostring(entry.frame)},
    {"scanline", tostring(entry.scanline)},
    {"pc", "\"0x" .. hex(pc, 6) .. "\""},
    {"address", "\"0x" .. hex(address, 4) .. "\""},
    {"value", tostring(value)}
  })
end

local function update_dma_shadow(address, value)
  if address < 0x4300 or address > 0x437F then
    return
  end
  local channel = (address - 0x4300) >> 4
  local reg = (address - 0x4300) & 0x0F
  shadow.dma[channel][reg] = value
  note_write("dma_reg", address, value)
end

local function summarize_channel(channel)
  local regs = shadow.dma[channel]
  local ctrl = regs[0x0] or 0
  local bbus = regs[0x1] or 0
  local abus_lo = regs[0x2] or 0
  local abus_hi = regs[0x3] or 0
  local abus_bank = regs[0x4] or 0
  local size_lo = regs[0x5] or 0
  local size_hi = regs[0x6] or 0
  local hdma_bank = regs[0x7] or 0
  local hdma_table = regs[0x8] or 0
  local line_counter = regs[0xA] or 0

  local abus = (abus_hi << 8) | abus_lo
  local size = (size_hi << 8) | size_lo
  local table_addr = (hdma_table & 0xFF) | ((regs[0x9] or 0) << 8)

  return string.format(
    "ch=%d ctrl=$%02X bbus=$%02X src=$%02X:%04X size=$%04X hdmaBank=$%02X hdmaTable=$%04X lineCtr=$%02X",
    channel,
    ctrl,
    bbus,
    abus_bank,
    abus,
    size,
    hdma_bank,
    table_addr,
    line_counter
  )
end

local function log_launch(kind, mask)
  local s = emu.getState()
  local pc = get_pc()
  emit_json({
    {"source", "\"mesen2_lua\""},
    {"event", "\"dma_launch\""},
    {"kind", "\"" .. json_escape(string.lower(kind) .. "_launch") .. "\""},
    {"launch_kind", "\"" .. json_escape(kind) .. "\""},
    {"frame", tostring(s["frameCount"] or -1)},
    {"scanline", tostring(s["ppu.scanline"] or -1)},
    {"pc", "\"0x" .. hex(pc, 6) .. "\""},
    {"mask", "\"0x" .. hex(mask, 2) .. "\""}
  })
  for ch = 0, 7 do
    if (mask & (1 << ch)) ~= 0 then
      local regs = shadow.dma[ch]
      local ctrl = regs[0x0] or 0
      local bbus = regs[0x1] or 0
      local abus_lo = regs[0x2] or 0
      local abus_hi = regs[0x3] or 0
      local abus_bank = regs[0x4] or 0
      local size_lo = regs[0x5] or 0
      local size_hi = regs[0x6] or 0
      local hdma_bank = regs[0x7] or 0
      local table_lo = regs[0x8] or 0
      local table_hi = regs[0x9] or 0
      emit_json({
        {"source", "\"mesen2_lua\""},
        {"event", "\"dma_channel\""},
        {"kind", "\"" .. json_escape(string.lower(kind) .. "_channel") .. "\""},
        {"launch_kind", "\"" .. json_escape(kind) .. "\""},
        {"frame", tostring(s["frameCount"] or -1)},
        {"scanline", tostring(s["ppu.scanline"] or -1)},
        {"pc", "\"0x" .. hex(pc, 6) .. "\""},
        {"mask", "\"0x" .. hex(mask, 2) .. "\""},
        {"channel", tostring(ch)},
        {"address", "\"0x43" .. hex(ch, 1) .. "0\""},
        {"value", "\"" .. json_escape(summarize_channel(ch)) .. "\""},
        {"src", "\"$" .. hex(abus_bank, 2) .. ":" .. hex((abus_hi << 8) | abus_lo, 4) .. "\""},
        {"bbus", "\"$" .. hex(bbus, 2) .. "\""},
        {"size", "\"$" .. hex((size_hi << 8) | size_lo, 4) .. "\""},
        {"hdma_bank", "\"$" .. hex(hdma_bank, 2) .. "\""},
        {"hdma_table", "\"$" .. hex((table_hi << 8) | table_lo, 4) .. "\""},
        {"ctrl", "\"$" .. hex(ctrl, 2) .. "\""}
      })
    end
  end
end

local function on_register_write(address, value)
  if address >= 0x4300 and address <= 0x437F then
    update_dma_shadow(address, value)
    return
  end

  if address == 0x420B then
    note_write("dma_start", address, value)
    log_launch("DMA", value)
    return
  end

  if address == 0x420C then
    note_write("hdma_enable", address, value)
    log_launch("HDMA", value)
    return
  end

  if address >= 0x2115 and address <= 0x2119 then
    note_write("vram_reg", address, value)
    return
  end

  if address >= 0x2121 and address <= 0x2122 then
    note_write("cgram_reg", address, value)
    return
  end

  if address >= 0x2102 and address <= 0x2104 then
    note_write("oam_reg", address, value)
    return
  end

  if address >= 0x2140 and address <= 0x2143 then
    note_write("apu_io_reg", address, value)
    return
  end
end

local function on_end_frame()
  local s = emu.getState()
  local frame = s["frameCount"] or -1
  if frame == shadow.frame then
    return
  end
  shadow.frame = frame

  local important = 0
  for _, entry in ipairs(shadow.current) do
    if entry.kind == "dma_start" or entry.kind == "hdma_enable" then
      important = important + 1
    end
  end

  if important > 0 then
    emit_json({
      {"source", "\"mesen2_lua\""},
      {"event", "\"frame_summary\""},
      {"kind", "\"frame_summary\""},
      {"frame", tostring(frame)},
      {"scanline", tostring(s["ppu.scanline"] or -1)},
      {"pc", "\"0x" .. hex(get_pc(), 6) .. "\""},
      {"value", tostring(#shadow.current)}
    })
  end

  shadow.current = {}
end

emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x2102, 0x2122)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x2140, 0x2143)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x420B, 0x420C)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x4300, 0x437F)
emu.addEventCallback(on_end_frame, emu.eventType.endFrame)

emu.displayMessage("DMA Probe", "SNES DMA probe loaded.")
