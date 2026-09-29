<h1 align="center">macro_gen</h1>

<p align="center">
  <strong>Transistor-level CMOS generation and sizing from CIRCT, calibrated to your PDK.</strong>
</p>

<p align="center">
  <a href="https://github.com/JoyenBenitto/macro_gen/actions/workflows/ci.yml"><img src="https://github.com/JoyenBenitto/macro_gen/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/JoyenBenitto/macro_gen/actions/workflows/docs.yml"><img src="https://github.com/JoyenBenitto/macro_gen/actions/workflows/docs.yml/badge.svg?branch=main" alt="Docs"></a>
  <a href="https://crates.io/crates/macro_gen"><img src="https://img.shields.io/crates/v/macro_gen.svg" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/JoyenBenitto/macro_gen.svg" alt="License"></a>
</p>

---

macro_gen turns combinational logic into sized, transistor-level CMOS. It characterizes
a reference inverter against your PDK with ngspice, builds every gate as a static
pull-up and pull-down network, sizes each transistor by the method of logical effort,
and buffers outputs to the delay-optimal number of stages. The result is a SPICE
netlist ready for simulation, a structural Verilog netlist ready for physical design,
and a report of every sizing decision.

## Highlights

- **PDK calibrated.** Sizing is anchored to a reference inverter characterized in
  process through ngspice, including its measured input capacitance.
- **CIRCT native.** Reads `hw` and `comb` MLIR directly, the IR produced by modern
  hardware compilers.
- **Complex cells.** Multi-gate functions such as AOI and OAI collapse into a single
  CMOS stage.
- **Logical effort sizing.** One stage effort across the whole netlist, with fanout and
  reconvergence handled automatically.
- **Optimal buffering.** Outputs are buffered to `round(log4 F)` stages, with or
  without preserving polarity.
- **Traceable output.** Every width and delay can be checked by hand from the run log,
  and every file is stamped with the version and commit that produced it.

## Installation

| Requirement | Needed for |
| --- | --- |
| Rust (stable) | Building macro_gen |
| ngspice with its shared library (`libngspice0-dev`) | All runs |
| A pre-built [CIRCT](https://circt.llvm.org/), via `CIRCT_DIR` | The CMOS backend (`circt` feature) |

```bash
# Inverter characterization only
cargo install macro_gen

# Full CMOS backend
git clone https://github.com/JoyenBenitto/macro_gen.git && cd macro_gen
export CIRCT_DIR=/path/to/circt
cargo build --release --features circt
```

## Usage

A run is described by a single TOML config: the PDK, the reference inverter, the sizing
targets, and the design.

```toml
[sizing]
cload_cinv = 64.0   # load on each output port, in reference inverter input capacitances
cin_cinv = 1.0      # largest capacitance any input may present
stage_effort = 4.0  # target effort per stage when buffering

[circt]
mlir_path = "c17.mlir"
top_module = "c17"
```

```bash
macro_gen --config c17.toml --emit-verilog --add-buffer
```

| Output | Contents |
| --- | --- |
| `spice/<top>.spice` | Sized transistor netlist, one subcircuit per cell size |
| `verilog/<top>.v` | Structural netlist: cell instances and wires only (`--emit-verilog`) |
| `reports/<top>.toml` | Units, reference inverter, cell and transistor counts, per-output path effort and delay |

Complete, runnable configs for ten reference designs live in [`benchmarks/`](benchmarks).

## Documentation

- [Getting started](https://joyenbenitto.github.io/macro_gen/getting-started.html)
- [CMOS backend](https://joyenbenitto.github.io/macro_gen/cmos-backend.html): sizing method, outputs, units and limitations
- [Benchmarks](https://joyenbenitto.github.io/macro_gen/benchmarks.html): reference designs and results
- [CLI reference](https://joyenbenitto.github.io/macro_gen/cli-reference.html)
- [Changelog](CHANGELOG.md)

## License

Released under the [MIT License](LICENSE).
