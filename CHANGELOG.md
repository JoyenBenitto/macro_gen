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
- `--add-buffer [invertible|non-invertible]` flag, which writes a structural, PD-clean Verilog netlist.
- `--emit-buffered-spice` flag, which writes the buffered SPICE deck.
- The sized SPICE netlist is always written to `spice/<top>.spice`.

### Changed
- The CIRCT flow now characterizes the reference inverter with ngspice before sizing.

## [0.1.0]

### Added
- Reference inverter synthesis: sizes the PMOS against a target switching threshold, using device parameters extracted from ngspice.
- `--config` / `--build-dir` CLI flags.
- CI: build, test, and publish to crates.io on merge to `main`.
