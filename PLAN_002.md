# PLAN 002

This plan starts the dynamic-analysis phase for SNES ROM investigation.

## Why A New Plan

`PLAN_001` established the static baseline:

- LoROM header parsing
- recursive 65816 traversal
- CFG construction
- jump-table recovery
- conservative state merges
- first-pass code/data classification

That work is good enough to support a second phase centered on runtime evidence.

## Primary Goal

Use a local emulator with debugger/script support to collect high-value runtime signals for:

- DMA setup and launch
- VRAM, CGRAM, and OAM update paths
- likely decompression routines
- ROM-to-PPU asset-loading flows

## Emulator Choice

Primary target: `~/Sandbox/Mesen2`

Reason:

- explicit debugger interop API
- script window / Lua support
- trace logger support
- debug event support
- memory operation categories that already mention DMA

Secondary reference: `~/Sandbox/ares`

Reason:

- strong tracer/debugger architecture
- useful reference for accuracy and debugger design
- possible later use through tracer or GDB-server style integration

## Scope Of PLAN 002

1. Inspect Mesen2’s SNES debugging and scripting surfaces
2. Identify practical runtime hooks for SNES DMA and PPU writes
3. Build a first probe artifact that can capture candidate asset-loading behavior
4. Correlate runtime evidence back to the static labels and CFG produced by this project

## Expected Deliverables

- documentation of the usable Mesen2 SNES API surface
- a concrete dynamic tracing strategy
- a first probe script or integration helper
- a mapping between runtime events and static addresses from the Rust disassembler
- routine-level summaries for DMA and PPU-heavy behavior
- a combined evidence report that merges runtime and usage-map signals

## First Work Items

### 1. API inspection

Determine whether Mesen2’s SNES support exposes:

- Lua callbacks for instruction execution
- CPU read/write hooks
- memory access logging
- breakpoint/action callbacks
- trace logger export
- event-stream access for SNES debugger tools

### 2. DMA-focused probe design

Need visibility into writes to:

- `$4300-$43FF`
- `$420B`
- `$420C`
- `$2115-$2119`
- `$2121-$2122`
- `$2102-$2104`

Need to record:

- CPU PC at write time
- target register
- written value
- nearby preceding ROM reads if exposed
- frame / scanline / cycle context if available

### 3. Static/dynamic correlation

Map runtime PCs back onto:

- disassembler labels
- subroutine starts
- jump-table entries
- likely data regions

This should make it possible to identify which static subroutines are responsible for DMA and asset uploads.

## Immediate Next Task

Inspect Mesen2’s SNES debugger/Lua API and determine the narrowest viable path to a DMA/VRAM tracing probe.

## Current Status

Completed:

- inspected Mesen2 source for SNES debugger, event manager, Lua API, and interop
- confirmed Lua memory callbacks are sufficient for a first DMA/PPU register probe
- added a first Lua probe script to this repo
- added an event-dumper scaffold that mirrors the in-process Mesen2 debug API
- wired the existing Rust runtime correlator into the usable dynamic-analysis path
- added routine-level runtime summaries that bucket DMA, VRAM, CGRAM, and OAM activity
- added usage-map import with a generic byte-per-ROM format and native BizHawk SNES CDL `CARTROM` support
- added a combined evidence report that ranks routines using both runtime and usage signals
- added a repo-native `collect-trace` command that generates a self-contained Mesen2 headless Lua script
- added scripted-input capture profiles for richer boot/title/menu traces
- switched rich trace capture away from `emu.log` scraping and onto Lua-written `trace.jsonl`

Next:

- finish validating the headless Mesen2 path on real ROMs now that `collect-trace` exists
- collect richer traces for `Pocky-n-Rocky` and `Sunset-Riders`
- correlate dynamic PCs to static labels, subroutines, and basic blocks automatically
- add another concrete emulator-native usage-map parser if we confirm one beyond BizHawk CDL
- feed combined evidence back into static recovery and candidate ranking
- defer full debug-event export until we either patch Mesen2 for IPC or build a minimal host around its core/debug DLL
