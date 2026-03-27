use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_MESEN_PATH: &str = "/Users/damir00/Sandbox/Mesen2/bin/osx-arm64/Release/Mesen";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TraceCaptureConfig {
    pub rom_path: PathBuf,
    pub out_dir: PathBuf,
    pub mesen_path: PathBuf,
    pub frames: u32,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TraceCaptureResult {
    pub trace_path: PathBuf,
    pub raw_stdout_path: PathBuf,
    pub script_path: PathBuf,
    pub settings_path: PathBuf,
    pub json_line_count: usize,
    pub frames: u32,
    pub profile: String,
    pub mesen_path: PathBuf,
    pub rom_path: PathBuf,
}

pub fn run_collect_trace_cli(args: &[String]) -> io::Result<()> {
    let config = parse_capture_args(args)?;
    let result = collect_trace(&config)?;
    println!(
        "captured runtime trace {} -> {} ({} JSON events)",
        result.rom_path.display(),
        result.trace_path.display(),
        result.json_line_count
    );
    Ok(())
}

pub fn parse_capture_args(args: &[String]) -> io::Result<TraceCaptureConfig> {
    let mut rom_path = None::<PathBuf>;
    let mut out_dir = None::<PathBuf>;
    let mut mesen_path = PathBuf::from(DEFAULT_MESEN_PATH);
    let mut frames = 3600u32;
    let mut profile = "rich-title-loop".to_string();

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--rom" => {
                index += 1;
                rom_path = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out_dir = args.get(index).map(PathBuf::from);
            }
            "--mesen" => {
                index += 1;
                mesen_path = args.get(index).map(PathBuf::from).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "missing `--mesen <path>`")
                })?;
            }
            "--frames" => {
                index += 1;
                frames = args
                    .get(index)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "missing `--frames <n>`")
                    })?
                    .parse::<u32>()
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "invalid frame count")
                    })?;
            }
            "--profile" => {
                index += 1;
                profile = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "missing `--profile <name>`")
                    })?;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown argument `{other}`; expected `collect-trace --rom <path> --out <dir> [--mesen <path>] [--frames <n>] [--profile rich-title-loop|boot-only]`"
                    ),
                ));
            }
        }
        index += 1;
    }

    let rom_path = rom_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--rom <path>` for `collect-trace`",
        )
    })?;
    let out_dir = out_dir.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing `--out <dir>` for `collect-trace`",
        )
    })?;

    if !matches!(profile.as_str(), "rich-title-loop" | "boot-only") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported profile; expected `rich-title-loop` or `boot-only`",
        ));
    }

    Ok(TraceCaptureConfig {
        rom_path,
        out_dir,
        mesen_path,
        frames,
        profile,
    })
}

pub fn collect_trace(config: &TraceCaptureConfig) -> io::Result<TraceCaptureResult> {
    fs::create_dir_all(&config.out_dir)?;
    let out_dir = absolutize_path(&config.out_dir)?;
    let rom_path = absolutize_path(&config.rom_path)?;
    let mesen_path = absolutize_path(&config.mesen_path)?;

    let script_path = out_dir.join("mesen2_headless_capture.lua");
    let trace_path = out_dir.join("trace.jsonl");
    let raw_stdout_path = out_dir.join("mesen_stdout.log");
    let capture_report_path = out_dir.join("capture_report.json");
    let settings_path = ensure_portable_mesen_settings(&mesen_path, config.frames)?;

    fs::write(
        &script_path,
        render_headless_capture_script(config.frames, &config.profile, &trace_path),
    )?;

    let timeout_seconds = u32::max(120, config.frames / 30 + 30);
    let output = Command::new(&mesen_path)
        .arg("--doNotSaveSettings")
        .arg("--testRunner")
        .arg(format!("--timeout={timeout_seconds}"))
        .arg(&script_path)
        .arg(&rom_path)
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "failed to run Mesen2 at {}: {error}",
                    mesen_path.display()
                ),
            )
        })?;

    let mut raw = String::new();
    raw.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !raw.ends_with('\n') {
            raw.push('\n');
        }
        raw.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    fs::write(&raw_stdout_path, &raw)?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Mesen2 exited with status {}. See {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
            raw_stdout_path.display()
        )));
    }

    if !trace_path.exists() {
        return Err(io::Error::other(format!(
            "Mesen2 completed but did not write {}. See {}",
            trace_path.display(),
            raw_stdout_path.display()
        )));
    }
    let trace_text = fs::read_to_string(&trace_path)?;
    let json_lines = extract_json_lines(&trace_text);
    if json_lines.is_empty() {
        return Err(io::Error::other(format!(
            "Mesen2 completed but trace file was empty. See {} and {}",
            trace_path.display(),
            raw_stdout_path.display()
        )));
    }

    let result = TraceCaptureResult {
        trace_path,
        raw_stdout_path,
        script_path,
        settings_path,
        json_line_count: json_lines.len(),
        frames: config.frames,
        profile: config.profile.clone(),
        mesen_path,
        rom_path,
    };
    let report_text = serde_json::to_string_pretty(&result).map_err(io::Error::other)?;
    fs::write(capture_report_path, report_text)?;

    Ok(result)
}

fn extract_json_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn ensure_portable_mesen_settings(mesen_path: &Path, frames: u32) -> io::Result<PathBuf> {
    let mesen_dir = mesen_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid Mesen path `{}`", mesen_path.display()),
        )
    })?;
    let settings_path = mesen_dir.join("settings.json");
    let timeout = u32::max(60, frames / 30 + 30);
    let config = json!({
        "Debug": {
            "ScriptWindow": {
                "AllowIoOsAccess": true,
                "AllowNetworkAccess": false,
                "ScriptTimeout": timeout
            }
        }
    });
    let text = serde_json::to_string_pretty(&config).map_err(io::Error::other)?;
    fs::write(&settings_path, text)?;
    Ok(settings_path)
}

fn render_headless_capture_script(max_frames: u32, profile: &str, trace_path: &Path) -> String {
    let steps = render_profile_steps(profile);
    let trace_path = lua_string_literal(trace_path);
    format!(
        r#"-- Auto-generated by snes_rom_hack collect-trace
local MAX_FRAMES = {max_frames}
local PROFILE = "{profile}"
local TRACE_PATH = "{trace_path}"
local shadow = {{
  frame = -1,
  dma = {{}},
  current = {{}},
  last_phase = "",
  write_counts = {{}},
  write_count_frame = -1,
  wram_write_counts = {{}},
  wram_write_count_frame = -1,
  queue_write_counts = {{}},
  queue_write_count_frame = -1,
}}
local trace_handle = nil

for ch = 0, 7 do
  shadow.dma[ch] = {{}}
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
  local parts = {{}}
  for _, field in ipairs(fields) do
    parts[#parts + 1] = string.format("\"%s\":%s", field[1], field[2])
  end
  trace_handle:write("{{" .. table.concat(parts, ",") .. "}}\n")
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
  local frame = s["frameCount"] or -1
  if frame ~= shadow.write_count_frame then
    shadow.write_count_frame = frame
    shadow.write_counts = {{}}
  end

  local limit = nil
  if kind == "oam_reg" and address == 0x2104 then
    limit = 8
  elseif kind == "cgram_reg" and address == 0x2122 then
    limit = 8
  elseif kind == "vram_reg" and (address == 0x2118 or address == 0x2119) then
    limit = 16
  end

  if limit ~= nil then
    local key = hex(address, 4)
    local count = shadow.write_counts[key] or 0
    if count >= limit then
      return
    end
    shadow.write_counts[key] = count + 1
  end

  local entry = {{
    frame = frame,
    scanline = s["ppu.scanline"] or -1,
    pc = pc,
    kind = kind,
    address = address,
    value = value
  }}
  table.insert(shadow.current, entry)
  emit_json({{
    {{"source", "\"mesen2_lua\""}},
    {{"event", "\"register_write\""}},
    {{"kind", "\"" .. json_escape(kind) .. "\""}},
    {{"frame", tostring(entry.frame)}},
    {{"scanline", tostring(entry.scanline)}},
    {{"pc", "\"0x" .. hex(pc, 6) .. "\""}},
    {{"address", "\"0x" .. hex(address, 4) .. "\""}},
    {{"value", tostring(value)}}
  }})
end

local function workram_to_snes(address)
  if address < 0x10000 then
    return 0x7E0000 | (address & 0xFFFF)
  end
  return 0x7F0000 | ((address - 0x10000) & 0xFFFF)
end

local function note_wram_write(region, address, value)
  local s = emu.getState()
  local pc = get_pc()
  local frame = s["frameCount"] or -1
  if frame ~= shadow.wram_write_count_frame then
    shadow.wram_write_count_frame = frame
    shadow.wram_write_counts = {{}}
  end

  local count = shadow.wram_write_counts[region] or 0
  if count >= 32 then
    return
  end
  shadow.wram_write_counts[region] = count + 1

  local snes = workram_to_snes(address)
  local bank = (snes >> 16) & 0xFF
  local addr16 = snes & 0xFFFF
  local entry = {{
    frame = frame,
    scanline = s["ppu.scanline"] or -1,
    pc = pc,
    kind = "wram_stage_write",
    address = snes,
    value = value
  }}
  table.insert(shadow.current, entry)
  emit_json({{
    {{"source", "\"mesen2_lua\""}},
    {{"event", "\"wram_write\""}},
    {{"kind", "\"wram_stage_write\""}},
    {{"region", "\"" .. json_escape(region) .. "\""}},
    {{"frame", tostring(entry.frame)}},
    {{"scanline", tostring(entry.scanline)}},
    {{"pc", "\"0x" .. hex(pc, 6) .. "\""}},
    {{"address", "\"$" .. hex(bank, 2) .. ":" .. hex(addr16, 4) .. "\""}},
    {{"value", tostring(value)}}
  }})
end

local function note_queue_write(region, address, value)
  local s = emu.getState()
  local pc = get_pc()
  local frame = s["frameCount"] or -1
  if frame ~= shadow.queue_write_count_frame then
    shadow.queue_write_count_frame = frame
    shadow.queue_write_counts = {{}}
  end

  local count = shadow.queue_write_counts[region] or 0
  if count >= 32 then
    return
  end
  shadow.queue_write_counts[region] = count + 1

  local entry = {{
    frame = frame,
    scanline = s["ppu.scanline"] or -1,
    pc = pc,
    kind = "asset_queue_write",
    address = address,
    value = value
  }}
  table.insert(shadow.current, entry)
  emit_json({{
    {{"source", "\"mesen2_lua\""}},
    {{"event", "\"queue_write\""}},
    {{"kind", "\"asset_queue_write\""}},
    {{"region", "\"" .. json_escape(region) .. "\""}},
    {{"frame", tostring(entry.frame)}},
    {{"scanline", tostring(entry.scanline)}},
    {{"pc", "\"0x" .. hex(pc, 6) .. "\""}},
    {{"address", "\"0x" .. hex(address, 4) .. "\""}},
    {{"value", tostring(value)}}
  }})
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
    channel, ctrl, bbus, abus_bank, abus, size, hdma_bank, table_addr, line_counter
  )
end

local function log_launch(kind, mask)
  local s = emu.getState()
  local pc = get_pc()
  emit_json({{
    {{"source", "\"mesen2_lua\""}},
    {{"event", "\"dma_launch\""}},
    {{"kind", "\"" .. json_escape(string.lower(kind) .. "_launch") .. "\""}},
    {{"launch_kind", "\"" .. json_escape(kind) .. "\""}},
    {{"frame", tostring(s["frameCount"] or -1)}},
    {{"scanline", tostring(s["ppu.scanline"] or -1)}},
    {{"pc", "\"0x" .. hex(pc, 6) .. "\""}},
    {{"mask", "\"0x" .. hex(mask, 2) .. "\""}}
  }})
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
      emit_json({{
        {{"source", "\"mesen2_lua\""}},
        {{"event", "\"dma_channel\""}},
        {{"kind", "\"" .. json_escape(string.lower(kind) .. "_channel") .. "\""}},
        {{"launch_kind", "\"" .. json_escape(kind) .. "\""}},
        {{"frame", tostring(s["frameCount"] or -1)}},
        {{"scanline", tostring(s["ppu.scanline"] or -1)}},
        {{"pc", "\"0x" .. hex(pc, 6) .. "\""}},
        {{"mask", "\"0x" .. hex(mask, 2) .. "\""}},
        {{"channel", tostring(ch)}},
        {{"address", "\"0x43" .. hex(ch, 1) .. "0\""}},
        {{"value", "\"" .. json_escape(summarize_channel(ch)) .. "\""}},
        {{"src", "\"$" .. hex(abus_bank, 2) .. ":" .. hex((abus_hi << 8) | abus_lo, 4) .. "\""}},
        {{"bbus", "\"$" .. hex(bbus, 2) .. "\""}},
        {{"size", "\"$" .. hex((size_hi << 8) | size_lo, 4) .. "\""}},
        {{"hdma_bank", "\"$" .. hex(hdma_bank, 2) .. "\""}},
        {{"hdma_table", "\"$" .. hex((table_hi << 8) | table_lo, 4) .. "\""}},
        {{"ctrl", "\"$" .. hex(ctrl, 2) .. "\""}}
      }})
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

local function on_palette_stage_write(address, value)
  note_wram_write("palette_stage", address, value)
end

local function on_graphics_stage_write(address, value)
  note_wram_write("graphics_stage", address, value)
end

local function on_asset_queue_write(address, value)
  note_queue_write("asset_queue", address, value)
end

local steps = {{
{steps}
}}

local function clear_buttons(input)
  input.a = false
  input.b = false
  input.x = false
  input.y = false
  input.l = false
  input.r = false
  input.start = false
  input.select = false
  input.up = false
  input.down = false
  input.left = false
  input.right = false
end

local function active_phase(frame)
  for _, step in ipairs(steps) do
    if frame >= step.start_frame and frame < step.end_frame then
      return step
    end
  end
  return nil
end

local function on_input_polled()
  local s = emu.getState()
  local frame = s["frameCount"] or 0
  local input = emu.getInput(0)
  clear_buttons(input)

  local step = active_phase(frame)
  local phase_name = "idle"
  if step ~= nil then
    phase_name = step.name
    for key, value in pairs(step.buttons) do
      input[key] = value
    end
  end
  emu.setInput(input, 0)

  if phase_name ~= shadow.last_phase then
    shadow.last_phase = phase_name
    emit_json({{
      {{"source", "\"mesen2_lua\""}},
      {{"event", "\"capture_phase\""}},
      {{"kind", "\"capture_phase\""}},
      {{"frame", tostring(frame)}},
      {{"scanline", tostring(s["ppu.scanline"] or -1)}},
      {{"pc", "\"0x" .. hex(get_pc(), 6) .. "\""}},
      {{"value", "\"" .. json_escape(phase_name) .. "\""}}
    }})
  end
end

local function on_end_frame()
  local s = emu.getState()
  local frame = s["frameCount"] or -1
  if frame ~= shadow.frame then
    shadow.frame = frame
    local important = 0
    for _, entry in ipairs(shadow.current) do
      if entry.kind == "dma_start" or entry.kind == "hdma_enable" or entry.kind == "apu_io_reg" then
        important = important + 1
      end
    end
    if important > 0 then
      emit_json({{
        {{"source", "\"mesen2_lua\""}},
        {{"event", "\"frame_summary\""}},
        {{"kind", "\"frame_summary\""}},
        {{"frame", tostring(frame)}},
        {{"scanline", tostring(s["ppu.scanline"] or -1)}},
        {{"pc", "\"0x" .. hex(get_pc(), 6) .. "\""}},
        {{"value", tostring(#shadow.current)}}
      }})
    end
    shadow.current = {{}}
  end

  if frame >= MAX_FRAMES then
    emit_json({{
      {{"source", "\"mesen2_lua\""}},
      {{"event", "\"capture_complete\""}},
      {{"kind", "\"capture_complete\""}},
      {{"frame", tostring(frame)}},
      {{"scanline", tostring(s["ppu.scanline"] or -1)}},
      {{"pc", "\"0x" .. hex(get_pc(), 6) .. "\""}},
      {{"value", "\"" .. json_escape(PROFILE) .. "\""}}
    }})
    trace_handle:flush()
    trace_handle:close()
    emu.stop(0)
  end
end

local state = emu.getState()
trace_handle = io.open(TRACE_PATH, "w")
if trace_handle == nil then
  emu.stop(2)
  return
end
if state["consoleType"] ~= "Snes" then
  trace_handle:write("{{\"event\":\"capture_error\",\"kind\":\"capture_error\",\"value\":\"not_snes\"}}\n")
  trace_handle:close()
  emu.stop(1)
  return
end

emit_json({{
  {{"source", "\"mesen2_lua\""}},
  {{"event", "\"capture_start\""}},
  {{"kind", "\"capture_start\""}},
  {{"frame", tostring(state["frameCount"] or 0)}},
  {{"scanline", tostring(state["ppu.scanline"] or -1)}},
  {{"pc", "\"0x" .. hex(get_pc(), 6) .. "\""}},
  {{"value", "\"" .. json_escape(PROFILE) .. "\""}}
}})

emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x2102, 0x2122)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x2140, 0x2143)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x420B, 0x420C)
emu.addMemoryCallback(on_register_write, emu.callbackType.write, 0x4300, 0x437F)
emu.addMemoryCallback(on_palette_stage_write, emu.callbackType.write, 0x02000, 0x023FF, emu.cpuType.snes, emu.memType.snesWorkRam)
emu.addMemoryCallback(on_graphics_stage_write, emu.callbackType.write, 0x18000, 0x19FFF, emu.cpuType.snes, emu.memType.snesWorkRam)
emu.addMemoryCallback(on_asset_queue_write, emu.callbackType.write, 0x0440, 0x047F)
emu.addEventCallback(on_input_polled, emu.eventType.inputPolled)
emu.addEventCallback(on_end_frame, emu.eventType.endFrame)
"#
    )
}

fn lua_string_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn absolutize_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

fn render_profile_steps(profile: &str) -> &'static str {
    match profile {
        "boot-only" => {
            r#"  { name = "boot_idle", start_frame = 0, end_frame = 900, buttons = {} }"#
        }
        "rich-title-loop" => {
            r#"  { name = "boot_idle", start_frame = 0, end_frame = 240, buttons = {} },
  { name = "press_start_1", start_frame = 240, end_frame = 252, buttons = { start = true } },
  { name = "settle_1", start_frame = 252, end_frame = 420, buttons = {} },
  { name = "press_start_2", start_frame = 420, end_frame = 432, buttons = { start = true } },
  { name = "menu_right", start_frame = 432, end_frame = 540, buttons = { right = true } },
  { name = "confirm_a", start_frame = 540, end_frame = 552, buttons = { a = true } },
  { name = "settle_2", start_frame = 552, end_frame = 720, buttons = {} },
  { name = "move_left", start_frame = 720, end_frame = 792, buttons = { left = true } },
  { name = "confirm_start", start_frame = 792, end_frame = 804, buttons = { start = true } },
  { name = "move_right", start_frame = 900, end_frame = 1020, buttons = { right = true } },
  { name = "jump_b", start_frame = 1080, end_frame = 1092, buttons = { b = true } },
  { name = "confirm_x", start_frame = 1200, end_frame = 1212, buttons = { x = true } },
  { name = "sound_probe_select", start_frame = 1320, end_frame = 1332, buttons = { select = true } },
  { name = "menu_down", start_frame = 1440, end_frame = 1512, buttons = { down = true } },
  { name = "menu_up", start_frame = 1560, end_frame = 1632, buttons = { up = true } },
  { name = "confirm_a_2", start_frame = 1680, end_frame = 1692, buttons = { a = true } },
  { name = "late_start", start_frame = 1860, end_frame = 1872, buttons = { start = true } }"#
        }
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_json_lines, lua_string_literal, parse_capture_args, render_headless_capture_script};
    use std::path::Path;

    #[test]
    fn extracts_only_json_lines() {
        let raw = "hello\n{\"event\":\"x\"}\nnoise {bad}\n  {\"event\":\"y\"}  \n";
        let lines = extract_json_lines(raw);
        assert_eq!(
            lines,
            vec![
                "{\"event\":\"x\"}".to_string(),
                "{\"event\":\"y\"}".to_string()
            ]
        );
    }

    #[test]
    fn parses_collect_trace_defaults() {
        let config = parse_capture_args(&[
            "--rom".to_string(),
            "roms-original/Sunset-Riders.sfc".to_string(),
            "--out".to_string(),
            "out/sunset-trace".to_string(),
        ])
        .expect("parse capture args");
        assert_eq!(config.frames, 3600);
        assert_eq!(config.profile, "rich-title-loop");
        assert!(config.mesen_path.to_string_lossy().contains("Mesen"));
    }

    #[test]
    fn rendered_script_contains_input_and_exit_hooks() {
        let script = render_headless_capture_script(
            1234,
            "rich-title-loop",
            Path::new("/tmp/trace.jsonl"),
        );
        assert!(script.contains("emu.addEventCallback(on_input_polled, emu.eventType.inputPolled)"));
        assert!(script.contains("MAX_FRAMES = 1234"));
        assert!(script.contains("press_start_1"));
        assert!(script.contains("io.open(TRACE_PATH, \"w\")"));
        assert!(script.contains("emu.stop(0)"));
        assert!(script.contains("emu.memType.snesWorkRam"));
        assert!(script.contains("0x18000"));
        assert!(script.contains("wram_stage_write"));
        assert!(script.contains("asset_queue_write"));
        assert!(script.contains("0x0440"));
    }

    #[test]
    fn lua_path_literal_escapes_backslashes() {
        let value = lua_string_literal(Path::new(r"C:\tmp\trace.jsonl"));
        assert_eq!(value, r"C:\\tmp\\trace.jsonl");
    }
}
