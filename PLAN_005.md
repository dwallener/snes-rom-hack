# PLAN 005

`PLAN_005` is the first designer-facing layer for ROM generation.

The goal is not to expose every engine knob.
The goal is to let a designer produce a tiny but real `single-screen action` game loop without editing TOML by hand.

## Product Slice

The first supported designer workflow is:

- one screen
- one player
- one enemy archetype
- multiple enemy instances
- one battle type
- defeat all enemies to win
- no scrolling
- no inventory
- no pickups
- no dialogue
- no multi-room progression beyond a title -> arena loop

This is intentionally narrow.

## Why Now

The current generation pipeline already has:

- project manifests
- content contracts
- asset definitions
- compiled scene and asset tables
- scene packets
- scene previews
- procedural placeholder sprites
- movement simulation
- generated engine-frame contracts

That means a webapp can now sit on top of real pipeline pieces instead of inventing a parallel authoring model.

## App Shape

The first designer app can be a small Streamlit app.

It should do four things:

1. create or open a template project
2. edit the small set of supported gameplay and asset fields
3. run validate/build/simulate
4. show logs and generated preview images

It should not try to be a full editor.

## Designer Inputs For V1

The app should expose only the fields needed for the one-screen arena battle:

- Project
  - project path
  - project name
  - game title

- Arena
  - background
  - music
  - player spawn

- Player
  - sprite page
  - palette
  - speed
  - attack type

- Enemy
  - sprite page
  - palette
  - speed
  - count
  - spawn positions

- Simulation
  - input sequence

The app may expose more later, but not in the first pass.

## Output Flow

The app should write/update:

- `game.toml`
- `assets/**/*.toml`
- `scenes/*.toml`
- `entities/*.toml`
- `scripts/main.toml`

Then call:

- `cargo run -- template validate ...`
- `cargo run -- template build ...`
- `cargo run -- template simulate ...`

## UI Sections

The first version should have:

1. Project
2. Assets
3. Arena
4. Actors
5. Build & Simulate

That is enough.

## Non-Goals

Do not add yet:

- open-ended prompt-to-game generation
- room painting tools
- pathfinding editors
- tilemap editing
- multiple templates in one UI
- sound editing
- package management for external asset libraries

## Immediate Deliverables

1. Add a Streamlit app file
2. Add a minimal README section for running it
3. Make the app create/open a project folder
4. Make the app save the constrained TOML fields
5. Make the app run validate/build/simulate and display outputs

## Success Criteria

`PLAN_005` is successful when a designer can:

- open the app
- initialize a one-screen action project
- change the player and enemy settings
- change the background/music bindings
- press a big build/simulate button
- see generated preview frames and logs

That is the first real designer loop.
