# Mesen2 Dynamic Probe Notes

This note records the practical dynamic-analysis path for `PLAN_002`.

## Summary

`Mesen2` is the preferred first integration target for runtime SNES probing because its source tree exposes:

- Lua scripting with memory callbacks
- debugger interop exports
- trace logger support
- SNES event manager support with `ProgramCounter`, `Scanline`, `Cycle`, DMA channel metadata, and register classification

## Source Findings

Relevant local files:

- `~/Sandbox/Mesen2/Core/Debugger/LuaApi.cpp`
- `~/Sandbox/Mesen2/Core/Debugger/ScriptingContext.h`
- `~/Sandbox/Mesen2/Core/Debugger/ScriptManager.h`
- `~/Sandbox/Mesen2/Core/SNES/Debugger/SnesDebugger.cpp`
- `~/Sandbox/Mesen2/Core/SNES/Debugger/SnesEventManager.cpp`
- `~/Sandbox/Mesen2/InteropDLL/DebugApiWrapper.cpp`
- `~/Sandbox/Mesen2/UI/Interop/DebugApi.cs`

## What Lua Can Do Immediately

Lua scripts can:

- register memory callbacks on CPU and PPU memory
- register frame/reset/input/code-break event callbacks
- read the full serialized emulator state with `emu.getState()`
- inspect current CPU state such as `cpu.pc`, `cpu.k`, and PPU state such as `ppu.scanline`

This is enough for a first-pass probe script that logs:

- DMA register setup
- DMA launch writes
- VRAM/CGRAM/OAM register writes
- current PC and frame

## What The Richer Debugger Layer Can Do

The SNES event manager records, for each debug event:

- `ProgramCounter`
- `Scanline`
- `Cycle`
- `DmaChannel`
- `DmaChannelInfo`
- `Operation`

This is exposed through debugger interop APIs such as:

- `GetDebugEvents`
- `GetDebugEventCount`
- `TakeEventSnapshot`
- `GetEventViewerEvent`

That path is better than Lua if we need high-fidelity event export without modifying the emulator core.

## First Probe Artifact

The first probe script is:

- [tools/mesen2_snes_dma_probe.lua](/Users/damir00/Sandbox/snes-rom-hack/tools/mesen2_snes_dma_probe.lua)

It hooks writes to:

- `$2102-$2104`
- `$2115-$2119`
- `$2121-$2122`
- `$420B-$420C`
- `$4300-$437F`

and logs DMA/HDMA launch summaries with current PC correlation.

## Current Practical Path

The practical path today is:

1. run `collect-trace` to generate a self-contained Lua test-runner script and emit `trace.jsonl`
2. feed those JSON lines into the Rust correlator with `labels.json` and `cfg.json`
3. use the resulting grouped report and annotated event list to identify which discovered routines perform DMA and PPU upload work

The current correlator now emits routine-level buckets for:

- DMA / HDMA
- VRAM
- CGRAM
- OAM
- other PPU register activity

This avoids blocking on a custom host or Mesen2 patch.

Current command:

```bash
cargo run -- collect-trace --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --out out/pocky-trace --frames 3600 --profile rich-title-loop
```

Important implementation detail:

- `--enableStdout` is not enough for `emu.log(...)` in `--testRunner`
- the current path enables Lua file I/O via portable `settings.json`
- the generated script writes JSON lines directly to `trace.jsonl`

## Next Integration Step

If the Lua path proves too shallow, the next escalation is still the same:

- build a small host-side helper that can consume Mesen2 debug events and dump structured JSON lines for:
  - DMA read/write events
  - register-write events in the PPU/DMA ranges
  - frame/scanline/cycle/PC correlation

That would complement the Rust static disassembler without requiring semantic emulation.

## Important Constraint

Mesen2’s debug interop is designed for an in-process host, not for attaching to an already-running external Mesen2 UI instance over IPC.

That means:

- a standalone helper can mirror the API surface now
- but it will only become operational when embedded into a host process that loads the Mesen2 core/debug DLL and initializes the debugger

So the practical path is:

1. use Lua immediately for first-pass DMA/PPU tracing
2. correlate the resulting JSON lines with `runtime-correlate`
3. later decide whether to:
   - patch Mesen2 lightly to expose an IPC/event stream, or
   - build a minimal custom host around the Mesen2 core/debug DLL

## DiztinGUIsh Comparison

Useful ideas borrowed from DiztinGUIsh:

- dynamic evidence should be ingested as a first-class input to disassembly work
- usage-map / CDL style evidence is complementary to trace capture
- the useful user-facing output is routine-level understanding, not raw event logs

Current status in this repo:

- runtime trace correlation is in place
- generic usage-map import is in place
- BizHawk SNES CDL `CARTROM` import is in place
- combined routine-evidence ranking is in place

This gives us the two evidence streams Diz’s approach suggests we should combine:

- runtime event traces
- accumulated code/data usage evidence

Not directly borrowed:

- WinForms UI architecture
- bsnes-specific socket capture path

Those are implementation details from a different toolchain, not the right short-term fit for this Rust/Mesen2 workflow.
