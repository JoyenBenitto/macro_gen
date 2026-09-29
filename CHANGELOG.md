# CHANGELOG

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.2.0] - 2026-09-29

### Added
- CIRCT input (`circt` feature): `hw` + `comb` MLIR is imported into a hypergraph netlist IR.
  - `comb.and` and `comb.or` become AND2 and OR2 chains; NOT (`comb.xor %x, %true`) becomes an inverter.
  - `hw.module` attributes are kept, and `macro_gen.cell = "complex"` marks a module to build as one CMOS stage per output.
  - A relative `[circt] mlir_path` is resolved against the config file's directory.
- Pass manager that verifies the IR after every pass, with these passes:
  - `dead-logic-elimination` removes gates that drive nothing.
  - `gate-simplify` folds an inverter into the AND, OR, NAND or NOR driving it.
  - `cmos-map` lowers every gate to a static CMOS pull-up and pull-down network.
  - `logical-effort-sizing` sizes every stage relative to the characterized reference inverter.
  - `buffer-insertion` buffers each output to `round(log_rho F)` stages.
- `[sizing]` config section: `cload_cinv`, `cin_cinv` and `stage_effort`.
- `--add-buffer [invertible|non-invertible]` buffers the outputs in place.
- `--emit-verilog` also writes the sized netlist as structural, PD clean Verilog, with black-box cell declarations.
- Outputs: a sized SPICE netlist (`spice/<top>.spice`) and a TOML report (`reports/<top>.toml`) on every CIRCT run.
- Units: the report has a `[units]` table and unit suffixed keys. C_inv is estimated in fF from ngspice gate capacitance, so capacitances appear in both C_inv and fF.
- Sizing tables in the run log: assumptions, every cell's logical effort, capacitances, drive and transistor widths, and every output's critical path with `G`, `B`, `H`, `F`, `f`, `N_opt` and delay, before and after buffering.
- Provenance: `--version` prints the commit, build time, toolchain, features and ngspice version; the startup banner adds host, OS, CPU, memory, command line and working directory; generated files are stamped with the generator version and time.
- Benchmark suite: ten designs in `benchmarks/<name>/`, each with its own config, and `benchmarks/run.py` to run them all and tabulate the results.
- Docs pages: CMOS Backend and Benchmarks.

### Changed
- The CIRCT flow characterizes the reference inverter with ngspice before sizing.
- Clear errors when backend flags are given without a `[circt]` section, or when an `.mlir` file is passed to `--config`.

## [0.1.0]

### Added
- Reference inverter synthesis: sizes the PMOS against a target switching threshold, using device parameters extracted from ngspice.
- `--config` / `--build-dir` CLI flags.
- CI: build, test, and publish to crates.io on merge to `main`.
