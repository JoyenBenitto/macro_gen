# Benchmarks

The benchmark suite measures the [CMOS backend](./cmos-backend.md) on a set of
small combinational designs. Each benchmark is a directory with the design and
its own config:

```
benchmarks/
  run.py
  c17/
    c17.mlir
    c17.toml
  maj3/
    maj3.mlir
    maj3.toml
  ...
```

Every config is complete, so any benchmark runs on its own:

```bash
$ ./target/release/macro_gen --config benchmarks/c17/c17.toml --emit-verilog --add-buffer
```

All configs target SKY130 (`tt`, 1.8 V) with a load of 64 C_inv per output and
inputs limited to 1 C_inv. Edit `include_path` to point at your SKY130 install.

## Designs

| Benchmark | What it exercises |
| --- | --- |
| `inv` | A single inverter driving the full load: the basic buffering case. |
| `nand2` | AND followed by NOT, folded back into one NAND2 stage. |
| `c17` | ISCAS-85 c17: six NAND2s, two outputs, reconvergent fanout. |
| `aoi21` | Inverting complex cell, one stage. |
| `oai22` | Inverting complex cell with two-high stacks in both networks. |
| `and_or_chain` | Non-inverting complex cell: AOI21 plus an inverter. |
| `maj3` | Majority (full adder carry) as a complex cell plus an inverter. |
| `mux2` | A 2:1 mux from AND, OR and NOT, mapped gate by gate. |
| `dec2to4` | A decoder with four outputs sharing inverted inputs. |
| `and8` | An 8 input AND, imported as a chain of AND2 gates. |

## Running the suite

```bash
$ export CIRCT_DIR=/path/to/circt
$ benchmarks/run.py                          # every benchmark
$ benchmarks/run.py c17 maj3                 # a subset
$ benchmarks/run.py --mode non-invertible    # keep output polarity
```

The runner builds macro_gen (`--release --features circt`) and runs every
config with `--emit-verilog --add-buffer <mode> --emit-buffered-spice`,
several at a time (`-j`). Each run characterizes the reference inverter in
ngspice, which takes about 40 seconds. It then reads each run's report and
prints a summary table, which it also writes to `benchmarks/build/summary.md`.
Each benchmark's netlists, report and full log land in
`benchmarks/build/<name>/`. The runner exits with an error if any benchmark
fails.

## Results

SKY130 `tt`, γ = 2.68, `--add-buffer invertible`. Arrows read unbuffered →
buffered; delay is the logical effort estimate of the worst output in τ
(an FO4 inverter is 5 τ).

| Benchmark | Cells | Transistors | Logic stages | F (worst) | Added inv | Stage effort | Delay (τ) |
|---|---|---|---|---|---|---|---|
| and8 | 14 → 22 | 42 → 58 | 14 | 2.363e+13 | 8 | 9.02 → 1.55 | 147.3 → 63.2 |
| and_or_chain | 2 → 4 | 8 → 12 | 2 | 128 | 2 | 11.31 → 3.36 | 27.4 → 20.2 |
| aoi21 | 1 → 4 | 6 → 12 | 1 | 128 | 3 | 128.00 → 3.36 | 131.7 → 20.2 |
| c17 | 6 → 12 | 24 → 36 | 3 | 2977 | 6 | 14.39 → 2.98 | 49.2 → 26.9 |
| dec2to4 | 10 → 21 | 28 → 50 | 3 | 2316 | 11 | 13.23 → 3.31 | 43.7 → 26.9 |
| inv | 1 → 3 | 2 → 6 | 1 | 64 | 2 | 64.00 → 4.00 | 65.0 → 15.0 |
| maj3 | 2 → 4 | 14 → 18 | 2 | 442.5 | 2 | 21.04 → 4.59 | 50.5 → 28.8 |
| mux2 | 7 → 7 | 20 → 20 | 5 | 655.2 | 0 | 3.66 → 3.66 | 25.3 → 25.3 |
| nand2 | 1 → 3 | 4 → 8 | 1 | 81.38 | 2 | 81.38 → 4.33 | 83.4 → 17.0 |
| oai22 | 1 → 4 | 8 → 14 | 1 | 128 | 3 | 128.00 → 3.36 | 132.0 → 20.5 |

Buffering brings the lone inverter from 65 τ to 15 τ (three stages at
`f = 4`) and c17 from 49 τ to 27 τ. `and8` and `mux2` show the cost of mapping
gate by gate (see [Limitations](./cmos-backend.md#limitations)).

## Adding a benchmark

Create `benchmarks/<name>/` with `<name>.mlir`, whose top `hw.module` is
`<name>`, and a `<name>.toml` config (copy one from another benchmark and
change its `[circt]` section and header comment). The runner picks it up
automatically.
