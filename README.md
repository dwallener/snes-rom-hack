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
