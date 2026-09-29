# macro_gen

[![Docs](https://img.shields.io/badge/docs-online-blue)](https://joyenbenitto.github.io/macro_gen/)

Automated digital IC macro generator.

macro_gen characterizes a reference inverter for your PDK in ngspice, then turns
a combinational CIRCT design into sized transistor-level CMOS. Every gate becomes
a pull-up and pull-down network sized by logical effort, outputs can be buffered
to the optimal number of stages, and the result is written as SPICE and PD clean
structural Verilog.

## Features

- Reference inverter characterization against any PDK, in process via ngspice
- CIRCT `hw` + `comb` import
- Static CMOS mapping, including multi-gate complex cells such as AOI21
- Logical effort sizing relative to the characterized inverter
- Output buffering to `round(log4 F)` stages, with or without inversion
- SPICE, structural Verilog and a TOML report, with every sizing step in the log

## Quickstart

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cd macro_gen
$ cargo build
$ ./target/debug/macro_gen --config examples/130nm.toml
```

This characterizes the SKY130 reference inverter. Requires ngspice with its
shared library ([Prerequisites](https://joyenbenitto.github.io/macro_gen/prerequisites.html)).

## Sizing a design

The CMOS backend needs a pre-built CIRCT:

```bash
$ export CIRCT_DIR=/path/to/circt
$ cargo build --release --features circt
$ ./target/release/macro_gen --config benchmarks/c17/c17.toml --emit-verilog --add-buffer
```

The design and sizing targets live in the config:

```toml
[sizing]
cload_cinv = 64.0  # load on each module output port, in units of the inverter's input capacitance
cin_cinv = 1.0     # largest input capacitance
stage_effort = 4.0

[circt]
mlir_path = "c17.mlir"
top_module = "c17"
```

Netlists land in `build/spice/` and `build/verilog/`, and a report in
`build/reports/`. See [CMOS Backend](https://joyenbenitto.github.io/macro_gen/cmos-backend.html)
for how sizing works and what each output contains.

## Benchmarks

Ten designs, from a single inverter to ISCAS c17, each with its own config in
`benchmarks/<name>/`. Run them all and get a summary table:

```bash
$ benchmarks/run.py
```

See [Benchmarks](https://joyenbenitto.github.io/macro_gen/benchmarks.html) for the
designs and results.

## Documentation

- [Getting Started](https://joyenbenitto.github.io/macro_gen/getting-started.html)
- [CMOS Backend](https://joyenbenitto.github.io/macro_gen/cmos-backend.html)
- [Benchmarks](https://joyenbenitto.github.io/macro_gen/benchmarks.html)
- [CLI Reference](https://joyenbenitto.github.io/macro_gen/cli-reference.html)

## Tests

```bash
$ cargo test                    # core
$ cargo test --features circt   # including the CIRCT backend
```

## License

MIT
