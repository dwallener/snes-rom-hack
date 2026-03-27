# Mesen2 Rich Trace Capture

Use the Rust wrapper instead of running Mesen2 manually:

```bash
cargo run -- collect-trace --rom roms-original/Pocky-n-Rocky/Pocky-n-Rocky.sfc --out out/pocky-trace
```

This generates:

- `out/pocky-trace/trace.jsonl`
- `out/pocky-trace/mesen_stdout.log`
- `out/pocky-trace/mesen2_headless_capture.lua`
- `out/pocky-trace/capture_report.json`

Profiles:

- `rich-title-loop`
- `boot-only`

Example:

```bash
cargo run -- collect-trace --rom roms-original/Sunset-Riders.sfc --out out/sunset-trace --frames 5400 --profile rich-title-loop
```
