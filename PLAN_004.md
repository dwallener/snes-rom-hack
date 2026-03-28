# PLAN 004

`PLAN_004` is the pivot from ROM analysis to ROM creation.

The goal is not to auto-generate arbitrary games.
The goal is to build a reusable SNES cartridge framework that can produce a small set of classic game templates from structured content.

## Product Definition

We are building:

- a reusable SNES cartridge runtime
- a content pipeline
- a genre-template system
- one template at a time

We are not building:

- a universal game generator
- a decompiler-driven commercial ROM editor
- a no-code engine for every possible game style

The output should be a real `.sfc` ROM with:

- graphics
- sound
- input
- scenes/levels
- entity behavior
- genre-specific game logic

## Template List

The template list is fixed for now:

1. Single screen action
2. Side scroller
3. Vertical scroller
4. Top-down action
5. RPG

Only one template is active at a time.

## First Template Recommendation

Start with:

1. Single screen action

Reason:

- simplest streaming model
- simplest camera model
- smallest content footprint
- easiest collision and entity activation rules
- easiest asset-loading model
- lowest risk path to a complete playable ROM

This is the right place to prove:

- engine architecture
- asset compilation
- scene loading
- NMI/DMA scheduling
- sprite/tile pipeline
- sound hooks
- content-driven build flow

After that, the likely order should be:

2. Top-down action
3. Side scroller
4. Vertical scroller
5. RPG

## Why Keep This In This Project

Yes, this should remain in this repository for now.

Reason:

- the existing reverse-engineering and probing tools already solve real SNES runtime problems
- the new engine work will need the same:
  - ROM mapping helpers
  - graphics and tile tooling
  - runtime capture
  - DMA/VRAM/CGRAM/OAM introspection
  - asset-path reasoning

So the project should become a multi-binary workspace with two major responsibilities:

- ROM analysis
- ROM generation

That is a good fit right now.

If it grows enough later, generation can be split out.
It is premature to split now.

## Proposed Repo Direction

Treat the crate as three layers:

1. Analysis layer
- disassembly
- runtime correlation
- asset-path tracing
- sheet matching

2. Shared SNES layer
- ROM/header handling
- LoROM mapping
- tile/palette encoders
- DMA-safe asset pack formats
- common address/layout helpers

3. Generation layer
- cartridge runtime
- asset compiler
- template compiler
- game build commands

## New Binary Direction

Add a new binary path inside this project, not a new repository.

Suggested command family:

- `cargo run -- template init --kind single-screen-action --out games/demo`
- `cargo run -- template build --project games/demo --out build/demo.sfc`
- `cargo run -- template validate --project games/demo`
- `cargo run -- template preview-assets --project games/demo`

This can share the existing CLI entrypoint, or be split into a second binary later.

For now, the cleanest path is to keep one binary and add a `template` command namespace.

## Architecture

### Engine Core

The cartridge runtime should provide:

- reset/init
- NMI loop
- joypad polling
- DMA scheduler
- VRAM/CGRAM/OAM upload queues
- scene manager
- entity update loop
- collision hooks
- text/UI hooks
- sound command hooks

The engine core must be:

- deterministic
- table-driven where possible
- strict about memory ownership
- explicit about per-frame budgets

### Template Logic

Each genre template adds:

- scene semantics
- entity semantics
- camera rules
- scoring/progression rules
- template-specific content schema

The template layer should be narrow.

It should not redefine the whole runtime.

### Content Compiler

The build pipeline should compile:

- indexed PNG sprite/tile inputs
- palettes
- tilemaps
- metasprites
- entity definitions
- scene definitions
- text
- music and SFX references

Into:

- SNES-native graphics data
- palettes
- scene tables
- entity tables
- generated manifests
- fixed bank allocations

### Project Format

Each generated game project should have a declarative source tree, for example:

- `game.toml`
- `assets/sprites/`
- `assets/backgrounds/`
- `assets/palettes/`
- `assets/audio/`
- `scenes/`
- `entities/`
- `scripts/`

The exact layout can evolve, but the principle is fixed:

- authored assets live outside engine code
- the builder converts authored assets into engine-ready ROM data

The generated project should also carry explicit engine assumptions:

- `memory.toml` for bank layout, WRAM usage, VRAM ownership, and DMA budgets
- `contracts.toml` for per-template scene, sprite, entity, and audio limits

## Single Screen Action Template Scope

This first template should support:

- one fixed-screen room at a time
- room transitions
- sprite actors
- ladders/platforms if needed
- simple tile collision
- enemy spawn tables
- player movement/jump/attack rules
- score/lives/game-over
- title screen
- simple HUD
- background music and SFX

It should not support in v1:

- streaming camera
- large scrolling maps
- complex scripting
- save files
- cutscenes beyond static transitions

## Technical Principles

### 1. Stable Engine, Generated Content

The ROM should be conceptually split into:

- engine code
- generated data banks
- manifests/tables

This matters because it keeps:

- template debugging sane
- content iteration fast
- bank allocation explicit

### 2. Asset Contracts Over Heuristics

Do not rely on runtime guessing in the generated engine.

Prefer:

- explicit sprite sheet manifests
- explicit palette assignments
- explicit VRAM slot plans
- explicit scene asset lists

### 3. Per-Scene Loading Discipline

The first template should load assets at scene boundaries.

Do not try to solve fully dynamic runtime streaming in v1.

### 4. Boring Runtime, Flexible Content

The engine should be conservative.
The content layer should be expressive within constraints.

## Immediate Plan

### Step 1. Formalize project structure

Create the initial generation-side module layout and command namespace.

Goal:

- no engine implementation yet
- just enough structure that the work has a stable home

Deliverables:

- `src/template/` or equivalent modules
- CLI command skeleton
- initial project manifest design

### Step 2. Define the cartridge memory model

Before writing engine code, define:

- LoROM bank budget
- engine bank allocation
- graphics bank allocation
- audio bank allocation
- scene data allocation
- save RAM assumptions
- per-frame DMA budget assumptions

Goal:

- prevent architecture drift

### Step 3. Define asset formats

Specify first-pass file formats for:

- spritesheets
- palettes
- metasprites
- backgrounds
- tilemaps
- scenes
- entities
- text
- audio references

Goal:

- content pipeline first
- no magic implied formats

### Step 4. Build the single-screen runtime skeleton

Implement:

- boot/reset
- NMI
- VRAM upload queue
- input
- one scene load
- one player sprite
- one enemy sprite
- one static background

Goal:

- first visible template ROM

Current state:

- project generation now emits explicit `memory.toml` and `contracts.toml`
- build output now emits a concrete runtime skeleton:
  - engine module layout
  - WRAM ownership
  - VRAM slot plan
  - frame schedule
  - scene flow
  - stub engine entrypoints
- runtime files now also define:
  - reset/bootstrap state ownership
  - joypad snapshot and edge state
  - DMA queue descriptor shape
  - minimal title/room load sequence

### Step 5. Build the authoring/build loop

Implement:

- asset import
- validation
- ROM assembly
- error reporting

Goal:

- build a ROM from a declarative project folder

### Step 6. Add gameplay

Only after the runtime skeleton is stable:

- player rules
- enemy rules
- collision
- score
- room progression

## Success Criteria For Template 1

The first template is successful when:

- a project folder with custom graphics can build into a working `.sfc`
- the resulting ROM boots on emulator
- player sprite art is clearly replaceable by content, not code edits
- one complete short game loop exists:
  - title
  - start
  - gameplay
  - lose/win/reset

## What To Reuse From Existing Work

Keep and reuse:

- ROM/header utilities
- LoROM address helpers
- tile/sheet tooling
- runtime capture tools
- graphics matching utilities
- documentation of SNES asset movement

Do not let reverse-engineering internals drive the engine design.

Analysis work informs the runtime.
It should not dictate messy compatibility layers.

## Current Decision

The first active implementation target under `PLAN_004` is:

- a single-screen action template
- inside this repository
- as another binary/command family
- with a clean content-first build pipeline

That is the narrowest path to a working “blank cartridge” that can later grow into a template family.
