# PLAN 003

This plan is the mental reset after the first static recovery work and the first successful dynamic trace collection.

`PLAN_001` answered: can we recover executable structure from the ROM?

`PLAN_002` answered: can we collect and correlate real runtime evidence from a local emulator?

`PLAN_003` is about the actual end goal:

- understand a ROM well enough to replace art and sound reliably
- stop optimizing for generic reverse-engineering output
- start optimizing for concrete asset pipeline recovery

## Why A New Plan

The project now has enough foundation to change the question.

We no longer need to ask:

- can we disassemble a LoROM?
- can we build a CFG?
- can we capture DMA and PPU activity?

We now need to ask:

- which routines prepare graphics and audio assets?
- where do those assets live in ROM?
- are they compressed, banked, streamed, or table-driven?
- what exact transformation path gets them from ROM into VRAM, CGRAM, OAM, or APU state?
- what do we need to patch so alternate assets can be inserted without breaking the engine?

That is a different kind of work. It is less about tooling breadth and more about end-to-end asset path recovery.

## Current Reality

What we already have:

- static LoROM 65816 disassembly and CFG recovery
- labels, basic blocks, jump-table recovery, code/data classification
- runtime correlation against those labels
- usage/CDL import
- combined evidence reporting
- annotated disassembly
- headless Mesen2 rich-trace collection

What the first real trace already proved:

- runtime collection works on a real ROM
- the trace can be mapped back to recovered routines
- `nmi_entry` and nearby helpers are visibly driving DMA/PPU activity in `Pocky-n-Rocky`

That means the next bottleneck is no longer “collect data at all”.

The bottleneck is:

- separate control/setup code from bulk transfer noise
- identify the producer routines and source ROM regions
- turn that into a reproducible asset replacement map

## First Findings

The first `PLAN_003` slice is now implemented in the runtime correlator.

What changed:

- interrupted traces with a truncated final JSON line are now accepted directly
- runtime summaries now include transfer episodes
- each episode includes:
  - frame window
  - register mix
  - primary routine
  - producer candidate that prefers a non-NMI helper when present

What the current `Pocky-n-Rocky` trace shows:

- the dominant transfer traffic still lives in `nmi_entry`
- the best current producer candidate for the repeated DMA-heavy episodes is `loc_80_8F1E`
- the two biggest transfer windows in the captured boot/menu slice are:
  - frames `97..277`
  - frames `281..344`
- those windows are no longer just “DMA happened”
- they currently resolve as:
  - WRAM staged CGRAM upload from `$7E:2000`
  - WRAM staged VRAM upload from `$7F:8000`
  - low-memory OAM-oriented transfer activity from `$00:0220`

Those windows are currently the best starting point for tracing graphics/palette upload preparation backward into ROM source selection.

## Current PLAN_003 Result

The staging-buffer producer path is now partially recovered from a fresh rich trace.

What changed:

- headless Mesen2 capture now records targeted WRAM writes for the known staging windows
- runtime correlation keeps valid LoROM PCs even when static CFG coverage is incomplete
- transfer episodes now attribute both:
  - upload-side producer
  - staging-buffer writer

What the updated `Pocky-n-Rocky` trace now shows:

- main upload window:
  - frames `97..277`
  - primary hot loop: `nmi_entry`
  - upload-side helper: `loc_80_8F1E`
  - staging-buffer writer: `sub_80_8B3B`
- smaller graphics upload windows:
  - frame `95`
  - frame `279`
  - both also point at `sub_80_8B3B` as the staging writer

This is the clearest current replacement-oriented chain:

- `sub_80_8B3B` fills staged WRAM buffers
- `loc_80_8F1E` participates in the upload path around NMI
- `nmi_entry` performs the recurring transfer traffic

That means the working graphics/palette replacement hypothesis is no longer just “patch the NMI uploader”.
It is now:

- inspect `sub_80_8B3B` first for asset preparation and source selection
- inspect `loc_80_8F1E` second for upload orchestration
- treat `nmi_entry` as the execution context, not the likely asset selector

## Primary Goal

Build an asset-path recovery workflow that answers, for any observed upload:

1. which routine initiated or orchestrated it
2. which ROM region supplied the data
3. whether the data was copied raw or transformed first
4. which destination domain received it:
   - VRAM
   - CGRAM
   - OAM
   - SPC/APU-side command or data path
5. what patch point or asset container should be modified for replacement

## Non-Goals

Still out of scope for this phase:

- full decompilation
- full emulator implementation
- universal support for every SNES coprocessor
- generalized asset editor UI
- automatic one-click art or sound replacement

This phase is about reliable mapping, not packaging.

## New Priority Order

1. Make runtime evidence cleaner and more actionable
2. Identify producer routines, not just hot upload loops
3. Recover ROM source regions for graphics and sound paths
4. Classify transform stages: raw copy vs decompression vs command packaging
5. Build replacement-oriented reports
6. Validate on more than one ROM

## Step 1: Clean Runtime Evidence

The first real trace was useful, but it also showed the current weakness clearly:

- raw writes to `$2104`, `$2122`, `$2118`, and `$2119` can dominate the event stream
- a killed capture leaves a truncated final JSON line
- hot routines can collapse onto `nmi_entry` even when a helper or caller is the real producer

### Work items

- make `runtime-correlate` tolerate a truncated final line automatically
- keep per-frame throttling for data-port writes and tune it with real ROMs
- distinguish:
  - register setup writes
  - transfer launch writes
  - bulk data-port writes
- emit frame-window summaries so traces are easier to segment by scene transition

### Success condition

A partial or interrupted trace should still be directly analyzable, and the report should make setup/launch code stand out from bulk byte pumping.

## Step 2: Identify Producer Routines

The current runtime summary is good at telling us what was hottest.
That is not the same as telling us what actually prepared the asset.

For asset replacement, the interesting routine is usually:

- the caller that chose the resource
- the decompressor or formatter
- the DMA setup routine

not just the final inner loop in NMI.

### Work items

- add caller-chain attribution around hot runtime events
- attribute transfer activity to:
  - the immediate routine
  - nearest non-NMI caller
  - recurring helper wrappers
- collapse per-frame repeated helper invocations into transfer episodes
- rank routines by “producer-likelihood”, not just raw event count

### Success condition

For each hot upload path, the report should identify both:

- the transfer loop
- the likely higher-level producer routine

## Step 3: Recover ROM Source Regions

This is the core bridge from reverse engineering to asset replacement.

We need to identify not only that DMA happened, but which ROM bytes likely fed it.

### Work items

- extend runtime capture or static analysis to highlight source addresses used in DMA channel setup
- map DMA source banks/offsets back to PC/ROM regions
- mark recurring ROM source regions that correlate with scene transitions
- connect source regions to:
  - jump tables
  - pointer tables
  - referenced data regions
  - decompression candidates
- emit a source-region report with:
  - ROM span
  - observed destination domain
  - controlling routines
  - confidence

### Success condition

We can point to specific ROM regions and say:

- this looks like sprite/tile data for a traced scene
- this looks like palette data
- this looks like sound command or song/sample setup data

## Step 4: Classify Transform Stages

Not every upload is a raw ROM-to-PPU copy.
Some assets will be:

- decompressed
- copied through WRAM staging
- table-driven
- packed into sound commands before APU transfer

### Work items

- identify WRAM staging buffers adjacent to DMA launches
- correlate candidate decompression routines with later transfer episodes
- separate:
  - raw ROM -> DMA
  - ROM -> WRAM -> DMA
  - ROM -> CPU register writes
  - CPU -> APU command/data stream
- add confidence tags:
  - `raw_copy`
  - `decompress_then_upload`
  - `table_driven_upload`
  - `sound_command_path`

### Success condition

Each asset path candidate should describe the likely transformation pipeline, not just the final destination register writes.

## Step 5: Build Replacement-Oriented Reports

The current reports are useful to the tool author.
The next reports need to be useful to the ROM hacker trying to replace assets.

### Work items

- add a per-episode asset transfer report
- group by destination domain:
  - graphics
  - palette
  - sprite/OAM
  - sound
- emit replacement-oriented fields:
  - source ROM region
  - controlling routine(s)
  - candidate pointer table
  - candidate decompressor
  - destination registers
  - scene/frame window
- add a shortlist report:
  - “best current art replacement candidates”
  - “best current sound replacement candidates”

### Success condition

A user should be able to open one report and immediately know where to start for:

- replacing a palette
- replacing a tile or sprite set
- tracing music/SFX setup

## Step 6: Validate On Multiple ROMs

It is too easy to overfit to one title.

### Work items

- run the phase-3 workflow on:
  - `Pocky-n-Rocky`
  - `Sunset-Riders`
- compare whether:
  - event throttling still works
  - producer attribution still finds sensible routines
  - source-region mapping still identifies plausible ROM spans
- add regression fixtures from the real traces where possible

### Success condition

The workflow should still produce useful candidate asset paths on more than one game without title-specific hacks.

## Immediate Next Task

Implement the smallest change that improves replacement-oriented usefulness immediately:

- make `runtime-correlate` accept truncated trailing lines
- add episode-level grouping so repeated NMI transfer traffic is collapsed into higher-level transfer windows
- emit a first replacement-oriented summary for the existing `Pocky-n-Rocky` trace

## Deliverables For PLAN 003

- `PLAN_003.md`
- more robust runtime correlation on imperfect captures
- episode-level transfer reports
- producer-routine attribution
- source-region candidate reports
- replacement-oriented art/sound candidate summaries

## Definition Of Success

This phase is successful when the tool can process a real traced session and produce a report that lets us say, with reasonable confidence:

- here is the routine that selected the resource
- here is the routine that transformed it, if any
- here is the ROM region that likely stores it
- here is how it reaches VRAM/CGRAM/OAM or the sound path
- here is the patch surface most likely needed to swap in an alternate asset
