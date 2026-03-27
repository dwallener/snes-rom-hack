# Mesen2 Event Dumper Scaffold

This is a minimal .NET scaffold for consuming Mesen2 SNES debug events as JSON lines.

## Important Limitation

Mesen2's debug API is designed for an in-process host. This scaffold can compile now, but it will only work when pointed at a live Mesen2 core/debug DLL environment that supports:

- `InitializeDebugger`
- `ReleaseDebugger`
- `GetDebugEventCount`
- `GetDebugEvents`
- `SetEventViewerConfig`

It is not an attach-to-running-process tool.

## Build

```bash
dotnet build tools/mesen2_event_dumper
```

## Run

```bash
dotnet run --project tools/mesen2_event_dumper -- /path/to/mesen/debug/dll
```

If the debugger runtime is not actually usable in-process, the tool will fail cleanly and explain the gap.

## Practical Recommendation

For immediate SNES work, use the Lua probe first:

1. run [tools/mesen2_snes_dma_probe.lua](/Users/damir00/Sandbox/snes-rom-hack/tools/mesen2_snes_dma_probe.lua) in Mesen2
2. save the JSON-line log output
3. correlate it with:

```bash
cargo run -- runtime-correlate --input trace.jsonl --labels out/pocky/labels.json --cfg out/pocky/cfg.json --out out/pocky/runtime
```
