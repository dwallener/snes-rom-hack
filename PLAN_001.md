# PLAN 001

This document captures the next implementation steps after the first-pass static LoROM 65816 disassembler and code mapper.

Dynamic emulator-driven work has moved into `PLAN_002.md`. This plan stays focused on static recovery quality.

## Current Status

The project can now:

- ingest `.sfc` ROMs
- strip optional 512-byte copier headers
- score and parse SNES headers
- recognize LoROM vs HiROM headers
- decode all 256 65816 opcodes
- track M/X-dependent immediate widths with explicit uncertainty
- recursively traverse from vector-derived entry points
- emit annotated disassembly, CFG, labels, code/data classification, jump-table candidates, and JSON reports

The current tool is useful for initial code recovery, but coverage is still limited by unresolved indirect transfers, conservative state handling, and shallow ROM data reference analysis.

## Goal Of The Next Phase

Increase structure recovery quality enough that later passes can reliably identify:

- decompression routines
- DMA setup and launch sites
- ROM-to-VRAM/CGRAM/OAM asset-loading paths

The immediate goal is not semantic lifting. It is better control-flow and better code/data boundaries.

## Priority Order

1. Improve indirect control-flow recovery
2. Strengthen decode-state propagation
3. Improve code/data classification
4. Add richer reporting and cross-references
5. Add phase-2 signal detection for DMA and decompression
6. Expand regression coverage on real ROMs

## Step 1: Improve Indirect Control-Flow Recovery

This is the highest-value next task because it should materially increase reachable code coverage.

### Work items

- Generalize jump-table detection beyond the current `ASL A ; TAX ; JMP (abs,X)` pattern.
- Add recognizers for:
  - `JMP (abs,X)`
  - `JSR (abs,X)`
  - `JMP (abs)`
  - index-scaling variants using repeated `ASL`, `ROL`, `TAX`, `TAY`, or explicit table index math
- Validate candidate tables by checking:
  - table base maps into ROM
  - entry addresses are same-bank plausible ROM targets
  - decoded targets begin with plausible instructions
  - multiple entries resolve cleanly
- Feed confirmed targets back into recursive traversal.
- Record rejected candidates explicitly in the report with reasons.

### Expected output changes

- more discovered subroutines
- more CFG edges
- fewer unresolved indirect transfers
- improved `jump_tables` section in JSON and text output

## Step 2: Strengthen Decode-State Propagation

Current state handling is intentionally conservative. It needs better block-merge behavior.

### Work items

- Replace hard stop on state collision with merge logic.
- Represent state merges explicitly:
  - equal values stay known
  - conflicting values become unknown
- Continue traversal through merged states rather than abandoning the target.
- Track likely PBR/DBR context heuristically when safe:
  - reset starts in bank `$80`
  - `JSL` and `JML` set explicit bank
  - same-bank `JSR/JMP abs` should stay bank-local unless proven otherwise
- Refine `XCE` handling:
  - keep uncertainty explicit
  - avoid poisoning unrelated state when the transition is locally inferable

### Expected output changes

- fewer early decode stops
- fewer false decode collisions
- better width handling for immediate operands

## Step 3: Improve Code/Data Classification

The current map mainly distinguishes decoded code from untouched bytes. That is too blunt for the next phase.

### Work items

- Split classification into:
  - `code`
  - `vector`
  - `header`
  - `referenced_data`
  - `jump_table`
  - `unknown`
- Mark ROM operands that point into valid LoROM space as referenced data when they are not traversed as code.
- Emit contiguous classified regions with reasons.
- Distinguish:
  - undecoded gaps inside reachable banks
  - long untouched banks with no code evidence
- Add summary metrics:
  - bytes referenced as data
  - bytes in jump tables
  - number of ambiguous regions

### Expected output changes

- better `code_map.json`
- more actionable ROM region summaries
- cleaner targeting for phase-2 analysis

## Step 4: Add Richer Reporting And Cross-References

The next analysis stages need faster navigation through the recovered graph.

### Work items

- Add subroutine summaries:
  - entry address
  - caller count
  - callee count
  - block count
  - unresolved exits
- Emit call graph summaries.
- Emit code/data xrefs.
- Annotate register-heavy blocks, especially writes to:
  - `$2100-$21FF`
  - `$4200-$43FF`
- Add warnings for:
  - suspicious decode truncation
  - invalid branch targets
  - jumps into undecoded regions

### Expected output changes

- faster manual reverse-engineering
- easier spotting of likely engine subsystems

## Step 5: Add Phase-2 Signal Detection

Once control-flow recovery is stronger, add targeted static signatures for DMA and decompression work.

### DMA detection

Detect routines that write to:

- `$4300-$43FF` DMA/HDMA registers
- `$420B` DMA start
- `$420C` HDMA enable
- `$2115-$2119` VRAM access registers
- `$2121-$2122` CGRAM access
- `$2102-$2104` OAM access

Emit:

- routine label
- write sites
- register set used
- likely transfer type
- confidence

### Decompression candidate detection

Look for patterns such as:

- tight loops with bit tests and shifts
- byte-stream reads from ROM-backed tables
- frequent `ROL`/`ROR`/`LSR`/`ASL` with branches
- output streams to WRAM or PPU staging buffers
- long loops preceding DMA setup

Emit:

- candidate routine
- reason codes
- nearby callers/callees
- related data regions
- confidence

### Asset-path detection

Correlate:

- ROM data references
- decompression candidates
- WRAM staging writes
- DMA launch sites

The target output is a list of likely ROM-to-PPU paths that can be inspected manually.

## Step 6: Expand Regression Coverage

Synthetic tests are useful but not enough.

### Work items

- Add regression tests from real LoROM snippets.
- Preserve small extracted byte windows that reproduce:
  - startup code with `XCE`, `REP`, `SEP`
  - interrupt handlers
  - indirect jump tables
  - bank-local subroutine dispatch
  - decode-state merges
- Add snapshot-style checks for:
  - CFG edge counts
  - unresolved indirect counts
  - jump-table detections

## Suggested Implementation Sequence

1. Indirect jump/call recovery and jump-table confirmation
2. State merge logic and bank-context heuristics
3. Better code/data classification
4. Cross-reference and subroutine reporting
5. DMA signal detection
6. Decompression candidate heuristics
7. Real-ROM regression suite

## Immediate Next Task

If work starts now, the best next task is:

Implement generalized indirect transfer recovery and confirmed jump-table target seeding.

That should deliver the largest coverage improvement for the least architectural churn.

## Definition Of Progress

This plan is succeeding when a real LoROM ROM produces:

- meaningfully more reachable code than the current pass
- fewer unresolved indirect jumps
- cleaner code/data region summaries
- explicit candidate routines for DMA and decompression follow-up
