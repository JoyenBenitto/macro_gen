# macro_gen

[![Docs](https://img.shields.io/badge/docs-online-blue)](https://joyenbenitto.github.io/macro_gen/)

Macro_gen is an automated digital IC macro generator.

Full documentation: <https://joyenbenitto.github.io/macro_gen/>

## Quickstart

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cd macro_gen
$ cargo build
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

See the [Prerequisites](https://joyenbenitto.github.io/macro_gen/prerequisites.html) page
if the build fails looking for `ngspice`, and the
[Getting Started](https://joyenbenitto.github.io/macro_gen/getting-started.html) guide for
a full walkthrough, CLI reference, and logging options.

## Building with CIRCT

Reading CIRCT `hw`+`comb` IR as input is behind the optional `circt` feature.
It needs a pre-built CIRCT, pointed to by `CIRCT_DIR` (either an install
prefix or an in-tree CIRCT checkout built with `ninja` under `build/`):

```bash
$ export CIRCT_DIR=/path/to/circt
$ cargo build --features circt
```

Run the smallest CIRCT example, `y = (a AND b) OR c`:

```bash
$ cargo run --features circt -- --config examples/circt_and_or_chain.toml --build-dir ./build_circt
```

## CMOS backend

A CIRCT run turns the design into sized transistors in these steps:

1. **Characterize.** Size the reference inverter with ngspice (as in the
   plain flow). Its `Wn`, `Wp` and `gamma = Wp/Wn` become the unit that
   every other stage is sized against.
2. **Import.** Read the `hw`+`comb` MLIR into the hypergraph netlist IR,
   then run `dead-logic-elimination`.
3. **`cmos-map`.** Lower every gate to one static CMOS stage: a pull-down
   network of NMOS and its dual pull-up network of PMOS.
   - INV, NAND2 and NOR2 map directly.
   - AND2 and OR2 become NAND2/NOR2 followed by an INV.
   - A module tagged `macro_gen.cell = "complex"` becomes one merged stage
     per output. `examples/circt/and_or_chain.mlir` becomes an AOI21 plus
     an inverter.
4. **`logical-effort-sizing`.** Size each stage so it drives like the
   reference inverter, which gives each input its logical effort `g`
   (NAND2 = (2+γ)/(1+γ)). Then pick one stage effort `f` for the whole
   module, so that the heaviest input presents exactly `cin_cinv` while
   every output drives `cload_cinv`. Each cell gets `macro_gen.drive` and
   `macro_gen.cin.<pin>` attributes for the next stage.
5. **Write `spice/<top>.spice`.** One `.subckt` per distinct
   (stage, drive), plus a `.subckt <top>` that wires them together.
6. **Optionally buffer.** With `--add-buffer`, each output's path effort
   `F = f^N` sets the optimal stage count `round(log_ρ F)` (ρ =
   `stage_effort`). Inverters make up the difference, sizing runs again,
   and the result is written as structural Verilog.

### Configuration

Capacitances are in units of `C_inv`, the input capacitance of the reference
inverter. A `[sizing]` section is required whenever `[circt]` is present:

```toml
[sizing]
cload_cinv = 64.0   # load on every output port
cin_cinv = 1.0      # largest capacitance any input may present (default 1)
stage_effort = 4.0  # buffering targets round(log4 F) stages (default 4)
```

### Buffering flags

| Flag | Effect |
| --- | --- |
| `--add-buffer` / `--add-buffer invertible` | Adds any number of inverters, so an odd count flips the output. The module is then tagged `macro_gen.inverted.<port>` and a warning is logged. |
| `--add-buffer non-invertible` | Adds inverters in pairs (inv + inv), so the logic function is kept. An odd shortfall rounds to whichever neighbouring even count is faster. |
| `--emit-buffered-spice` | Also writes the buffered SPICE deck. Implies `--add-buffer` (`invertible` unless a mode is given). |

```bash
$ cargo run --features circt -- --config examples/circt_and_or_chain.toml \
      --build-dir ./build_circt --add-buffer non-invertible --emit-buffered-spice
```

For that example, `F = g·H = 2 × 64 = 128`. Without buffers the two stages
run at `f = 11.3`. With buffers, `log4(128)` rounds to 4 stages, so two
inverters are added and `f` drops to `3.36`.

### Outputs

```
<build-dir>/
  spice/
    inv.spice                  reference inverter deck
    <top>.spice                sized, unbuffered netlist (always)
    <top>_buffered.spice       with --emit-buffered-spice
  verilog/                     with --add-buffer / --emit-buffered-spice
    <top>.v                    structural netlist
    <top>_cells.v              (* blackbox *) declarations of the cell masters
```

The Verilog is kept PD-clean. It has only port and `wire` declarations and
cell instances with named connections: no `assign`, no behavioural code and
no escaped identifiers. Cell masters are named `<stage>_x<drive>` (e.g.
`inv_x5p66`), the same names as the SPICE subcircuits, so the two match up
for LVS. The decks use explicit `VDD`/`GND` subckt pins and `.lib` the PDK
models, so a testbench can `.include` them directly.

### Current limitations

- XOR2, XNOR2 and MUX2 are rejected. They aren't unate, so they have no
  single-stage static CMOS mapping. Because of this,
  `examples/circt_comb_chain.toml` does not get through the backend yet.
- Module hierarchy (`hw.instance`) is rejected. Only the top module is sized.
- A complex cell must use only AND/OR/NAND/NOR/INV, and its inputs must be
  either all uninverted or all inverted.
- Transistors that would come out narrower than `min_width` are clamped up,
  with a warning. Raise `cin_cinv` if that happens.

## Tests

Run the tests, including the CIRCT-gated ones:

```bash
$ cargo test --features circt
```

If MLIR lives in a separate prefix from CIRCT, also set `MLIR_DIR`. Plain
`cargo build` (no feature) still builds the ngspice-only flow without CIRCT.
