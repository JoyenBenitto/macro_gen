# CHANGELOG

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- CMOS backend for the CIRCT flow:
  - `cmos-map` lowers gates, and `macro_gen.cell = "complex"` modules, to pull-up/pull-down networks.
  - `logical-effort-sizing` sizes the stages against the characterized reference inverter.
  - `buffer-insertion` buffers each output to `round(log_rho F)` stages.
- `[sizing]` config section: `cload_cinv`, `cin_cinv`, `stage_effort`.
- `--emit-verilog` flag, which writes the sized netlist as structural, PD-clean Verilog.
- `--add-buffer [invertible|non-invertible]` flag, which writes the buffered netlist as Verilog (`<top>_buffered.v`).
- `--emit-buffered-spice` flag, which writes the buffered SPICE deck.
- The sized SPICE netlist is always written to `spice/<top>.spice`, and a sizing and delay report to `reports/<top>.toml`.
- Sizing tables in the run log: assumptions, per-cell logical effort, capacitances, drive and transistor widths, and per-output critical path `G`, `B`, `H`, `F`, `f`, `N_opt` and delay, before and after buffering, plus a summary.
- A clear error when backend flags are given without a `[circt]` section, or when an `.mlir` file is passed to `--config`.
- NOT import: `comb.xor %x, %true` becomes an INV.
- `gate-simplify` pass, which folds an INV into the AND/OR/NAND/NOR driving it.
- `benchmarks/`: ten designs (c17, maj3, aoi21, oai22, dec2to4, and8 and more), each in `benchmarks/<name>/` with its own config, plus a runner, `benchmarks/run.py`, that sizes them all and tabulates the results.
- A relative `[circt] mlir_path` is resolved against the config file's directory.
- Docs pages: CMOS Backend and Benchmarks.
- Provenance:
  - `--version` prints the commit, build time, toolchain, features and ngspice version.
  - The startup banner shows the build details plus the host, OS, CPU, memory, command line and working directory.
  - Generated files are stamped with the generator version and time.

### Changed
- The CIRCT flow now characterizes the reference inverter with ngspice before sizing.

### Removed
- `examples/circt/` and the `examples/circt_*.toml` configs. They are replaced by `benchmarks/`.

## [0.1.0]

### Added
- Reference inverter synthesis: sizes the PMOS against a target switching threshold, using device parameters extracted from ngspice.
- `--config` / `--build-dir` CLI flags.
- CI: build, test, and publish to crates.io on merge to `main`.
