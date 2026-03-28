# SNES ROM Hack Tools

This crate now includes a first-pass static SNES ROM disassembler and code mapper for LoROM 65816 games.

## Disassembler

Run:

```bash
cargo run -- disasm rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --out out/pocky
```

Outputs:

- `out/pocky/disasm.txt`
- `out/pocky/report.json`
- `out/pocky/code_map.json`
- `out/pocky/cfg.json`
- `out/pocky/labels.json`

Correlate structured runtime probe output back to those labels and the recovered CFG:

```bash
cargo run -- runtime-correlate --input trace.jsonl --labels out/pocky/labels.json --cfg out/pocky/cfg.json --out out/pocky/runtime
```

This emits:

- `out/pocky/runtime/runtime_summary.txt`
- `out/pocky/runtime/runtime_report.json`
- `out/pocky/runtime/annotated_events.json`

The runtime summary groups activity by routine and highlights DMA/VRAM/CGRAM/OAM-heavy code paths.

Collect a richer headless trace directly from local Mesen2 with a scripted input loop:

```bash
cargo run -- collect-trace --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --out out/pocky-trace --frames 3600 --profile rich-title-loop
```

This writes:

- `out/pocky-trace/trace.jsonl`
- `out/pocky-trace/mesen_stdout.log`
- `out/pocky-trace/mesen2_headless_capture.lua`
- `out/pocky-trace/capture_report.json`

Notes:

- this uses the local Mesen2 build at `/Users/damir00/Sandbox/Mesen2/bin/osx-arm64/Release/Mesen` by default
- it provisions a portable `settings.json` beside that binary so Lua file I/O is enabled in `--testRunner`
- current built-in capture profiles are `rich-title-loop` and `boot-only`

Overlay external usage-map or CDL-style evidence onto the static map:

```bash
cargo run -- usage-map-import --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --input usage.bin --labels out/pocky/labels.json --code-map out/pocky/code_map.json --out out/pocky/usage
```

The current importer supports `--format simple-bits`:

- bit 0: observed execute
- bit 1: observed data/read

It also supports `--format bizhawk-cdl-snes` for BizHawk SNES CDL files by extracting the `CARTROM` block and mapping:

- `ExecFirst` / `ExecOperand` -> observed execute
- `CPUData` / `DMAData` -> observed data

Outputs:

- `out/pocky/usage/usage_summary.txt`
- `out/pocky/usage/usage_report.json`
- `out/pocky/usage/merged_code_map.json`

Combine runtime and usage evidence into one ranked routine report:

```bash
cargo run -- evidence-report --runtime-report out/pocky/runtime/runtime_report.json --usage-report out/pocky/usage/usage_report.json --out out/pocky/evidence
```

Outputs:

- `out/pocky/evidence/evidence_summary.txt`
- `out/pocky/evidence/evidence_report.json`

Overlay that evidence onto the disassembly and emit a hot-routine index:

```bash
cargo run -- annotate-evidence --disasm out/pocky/disasm.txt --labels out/pocky/labels.json --evidence out/pocky/evidence/evidence_report.json --out out/pocky/annotated
```

Derive first-pass asset path candidates from runtime events plus combined evidence:

```bash
cargo run -- asset-paths --events out/pocky/runtime/annotated_events.json --evidence out/pocky/evidence/evidence_report.json --out out/pocky/asset-paths
```

This now includes APU port activity as well, so sound-transfer routines can be surfaced alongside graphics-upload routines.

For the current Pocky player/sprite batch, decode the `sub_80_A39A` graphics commands into concrete ROM sources and raw 4bpp previews:

```bash
cargo run -- player-gfx-report --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --disasm out/pocky-seeded-plan3h/disasm.txt --out out/pocky-player-gfx
```

This emits:

- `out/pocky-player-gfx/player_gfx_summary.txt`
- `out/pocky-player-gfx/player_gfx_report.json`
- `out/pocky-player-gfx/previews/*.png`

Write a fixed-size proof patch directly into one decoded player graphics source region:

```bash
cargo run -- patch-player-gfx \
  --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc \
  --disasm out/pocky-seeded-plan3h/disasm.txt \
  --callsite '$80:BCB2' \
  --png artwork/SNES-Pocky-Sayo-chan.png \
  --out out/pocky-sayo-proof.sfc
```

This keeps the ROM size fixed and overwrites only the chosen decoded source window.

Attribute the decoded player batch against a supplied extracted character sheet:

```bash
cargo run -- match-player-gfx-sheet \
  --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc \
  --disasm out/pocky-seeded-plan3h/disasm.txt \
  --sheet artwork/SNES-Pocky-Sayo-chan.png \
  --out out/player-match-sayo
```

This emits a ranked summary of which decoded player-batch regions overlap that sheet.

Or run the whole phase-2 pipeline in one shot:

```bash
cargo run -- phase2-analyze --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --trace trace.jsonl --usage usage.cdl --usage-format bizhawk-cdl-snes --out out/pocky-phase2
```

Practical rich-trace workflow:

```bash
cargo run -- collect-trace --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --out out/pocky-trace
cargo run -- phase2-analyze --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --trace out/pocky-trace/trace.jsonl --usage usage.cdl --usage-format bizhawk-cdl-snes --out out/pocky-phase2
```

What v1 does:

- strips optional 512-byte copier headers
- scores and parses SNES headers
- recognizes LoROM vs HiROM headers
- decodes 65816 instructions with M/X-sensitive immediate widths
- recursively traverses code from vector-derived entry points
- builds a reachable code map and CFG
- marks unresolved indirect transfers instead of inventing targets
- records first-pass jump-table candidates

What v1 does not do:

- emulate execution
- recover semantics
- fully support HiROM traversal
- support enhancement chips
- guarantee perfect code/data separation

## Tests

Run:

```bash
cargo test
```

## Template Scaffold

The first `PLAN_004` slice adds a generation-side `template` namespace.

Initialize a new single-screen-action project:

```bash
cargo run -- template init --kind single-screen-action --out /tmp/template-demo
```

Validate the project layout:

```bash
cargo run -- template validate --project /tmp/template-demo
```

Preview the current asset tree:

```bash
cargo run -- template preview-assets --project /tmp/template-demo
```

Emit the current build scaffold:

```bash
cargo run -- template build --project /tmp/template-demo --out /tmp/template-demo-build
```

This is scaffold-only for now:

- project manifest and folder conventions exist
- cartridge memory model and content contracts are emitted as `memory.toml` and `contracts.toml`
- validation exists
- build-plan emission exists
- runtime layout and engine stub emission exist under `engine/`
- reset/NMI, joypad, DMA queue, and room-load contracts are described in the generated runtime files
- scene/entity/script TOML stubs are parsed and emitted as compiled manifest files under `content/`
- asset TOML stubs are parsed into stable-id asset tables and resolved scene/entity references under `assets/`
- placeholder binary asset packs and per-scene load packets are emitted for runtime consumption
- sprite assets can now be procedurally generated as animated “breathing ball” previews for player/NPC validation
- no engine/runtime ROM generation exists yet
