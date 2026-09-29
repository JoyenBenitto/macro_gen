# CMOS Backend

With the `circt` feature, macro_gen takes a combinational CIRCT design and
produces sized transistor-level CMOS: a SPICE netlist, structural Verilog and a
sizing report. Every transistor is sized with the method of logical effort,
relative to the reference inverter macro_gen characterizes for your PDK.

## Building

The backend needs a pre-built CIRCT, pointed to by `CIRCT_DIR` (an install
prefix, or a CIRCT checkout built with `ninja` under `build/`). If MLIR lives
in a separate prefix, also set `MLIR_DIR`.

```bash
$ export CIRCT_DIR=/path/to/circt
$ cargo build --release --features circt
```

## Running

A CIRCT run is driven entirely by its config. On top of the usual
`[environment]`, `[models]` and `[reference_inverter]` sections it has a
`[sizing]` section and a `[circt]` section naming the design:

```toml
# Capacitances are in units of C_inv, the reference inverter's input capacitance.
[sizing]
cload_cinv = 64.0  # load on every output
cin_cinv = 1.0     # largest capacitance any input may present (default 1)
stage_effort = 4.0 # buffering targets round(log4 F) stages (default 4)

# mlir_path is relative to this file.
[circt]
mlir_path = "c17.mlir"
top_module = "c17"
```

```bash
$ ./target/release/macro_gen --config benchmarks/c17/c17.toml --emit-verilog --add-buffer
```

| Flag | Effect |
| --- | --- |
| `--emit-verilog` | Write the sized netlist as structural Verilog. |
| `--add-buffer` | Buffer every output to the optimal number of stages and write the buffered Verilog. Same as `--add-buffer invertible`. |
| `--add-buffer non-invertible` | Buffer with inverters in pairs so every output keeps its polarity. |
| `--emit-buffered-spice` | Also write the buffered SPICE netlist. Implies `--add-buffer`. |

The backend flags need a `[circt]` section; without one macro_gen stops with an
error before characterizing anything.

## How a design is sized

1. **Characterize.** ngspice sizes the reference inverter for the target
   switching threshold. Its `Wn`, `Wp` and `γ = Wp/Wn` define the units:
   capacitance in `C_inv` (the inverter's input capacitance) and drive in
   multiples of the inverter's strength.
2. **Import.** The `hw` + `comb` MLIR becomes a hypergraph netlist.
   `comb.and` and `comb.or` become chains of AND2 and OR2 gates; NOT, which
   CIRCT writes as `comb.xor %x, %true`, becomes an inverter.
3. **Clean up.** `dead-logic-elimination` removes unused gates.
   `gate-simplify` folds an inverter into the AND, OR, NAND or NOR that
   drives it when it is that gate's only load, so `~(a & b)` is one NAND2.
4. **Map to CMOS (`cmos-map`).** Every gate becomes one static CMOS stage:
   an NMOS pull-down network and its dual PMOS pull-up network.
   - INV, NAND2 and NOR2 map directly.
   - AND2 and OR2 become NAND2 or NOR2 followed by an inverter.
   - A module tagged `macro_gen.cell = "complex"` collapses into one stage
     per output. Its logic is rewritten in negation normal form; the
     pull-down implements the complement, AND as series and OR as parallel.
     A non-inverting function such as `(a & b) | c` gets an output inverter
     (AOI21 + INV).
5. **Size (`logical-effort-sizing`).** Each stack is sized to drive like the
   reference inverter, which fixes each input's logical effort `g` and the
   stage's parasitic delay `p` (NAND2: `g = (2+γ)/(1+γ)`). Every stage then
   runs at one stage effort `f`: working from the outputs back, a stage with
   load `C_out` gets drive `s = C_out / f`, and each input presents
   `g · s`. `f` is solved so the heaviest primary input presents exactly
   `cin_cinv`. On a single path this is the textbook `f = F^(1/N)`; with
   fanout, branching is accounted for automatically.
6. **Buffer (optional).** For each output, the path effort `F = f^N` gives
   the optimal stage count `round(log_ρ F)`, with `ρ = stage_effort`.
   Inverters make up the difference between the driving stage and the
   port, then sizing runs again. In invertible mode an odd count inverts the
   output, which is logged and recorded as `macro_gen.inverted.<port>`. In
   non-invertible mode an odd shortfall becomes whichever neighbouring even
   count is faster.

For `and_or_chain` with a 64 C_inv load: `F = g · H = 2 × 64 = 128`. The two
logic stages alone run at `f = 11.3`. `log4(128)` rounds to 4, so two
inverters are added and `f` drops to 3.36.

## Outputs

```
<build-dir>/
  spice/
    inv.spice                  reference inverter
    <top>.spice                sized netlist (always)
    <top>_buffered.spice       --emit-buffered-spice
  verilog/
    <top>.v                    --emit-verilog
    <top>_cells.v
    <top>_buffered.v           --add-buffer
    <top>_buffered_cells.v
  reports/
    <top>.toml                 sizing and delay report (always)
```

**SPICE.** One `.subckt` per distinct stage and drive, named
`<stage>_x<drive>` (for example `nand2_x4p45`), plus a `.subckt <top>`
connecting them. Supplies are explicit `VDD` and `GND` pins, and the PDK
models are pulled in with `.lib`, so a testbench can `.include` the file
directly. Widths are `relative width × drive × Wn` (or `Wp`), clamped to
`min_width` and snapped to it when `w_is_multiple_of_w_min` is set.

**Verilog.** Structural and PD clean: port and `wire` declarations and cell
instances with named connections only. No `assign`, no behavioural code, no
escaped identifiers. Cell masters carry the same names as the SPICE
subcircuits so the two line up for LVS, and `*_cells.v` declares them as
`(* blackbox *)` modules.

**Report.** `reports/<top>.toml` holds the generator version, cell and
transistor counts, stage effort and worst delay before and after buffering,
and each output's logic stages, path effort, added inverters and delay.

Every generated file is stamped with the macro_gen version, commit and time.

## Sizing log

At the default `info` level the run log shows the complete calculation:

- **Sizing assumptions:** process, reference inverter, `γ`, units, output
  load, input limit, `ρ`, buffering mode, minimum width and delay model.
- **Sized cells:** for every cell in signal order, `g` and `p`, input and
  output capacitance, drive, and NMOS and PMOS widths in µm. `*` marks a
  width clamped up to `min_width`.
- **Output paths:** for every output, its critical path, `N`, `G`, `B`,
  `H`, `F = G·B·H`, `f`, `N_opt = log_ρ F`, inverters added, parasitic delay
  `P` and delay `N·f + P`.
- **Summary:** stage effort, cells, transistors and worst delay (in τ and
  FO4) before and after buffering.

The cell and path tables are printed for the unbuffered and the buffered
netlist.

## Limitations

- XOR2, XNOR2 and MUX2 are rejected: they are not unate and have no single
  stage static CMOS mapping. Constants other than NOT's `true` are rejected.
- Hierarchy (`hw.instance`) is rejected; only the top module is sized.
- A complex cell may use only AND, OR, NAND, NOR and NOT, with its inputs
  all uninverted or all inverted.
- Gates are mapped one at a time. Wide `comb.and` and `comb.or` become linear
  chains, and AND-OR logic is not remapped to NAND-NAND.
- Transistors narrower than `min_width` are clamped up with a warning, which
  makes those stages slower than planned. Raise `cin_cinv` to avoid it.
- Buffering chooses its stage count from the unbuffered path effort; after
  resizing, the path effort can come out lower than planned.
