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

Run the smallest CIRCT example, `y = (a AND b) OR c`. It imports the MLIR
into the netlist IR, runs the pass pipeline, and prints the result (no
ngspice run needed):

```bash
$ cargo run --features circt -- --config examples/circt_and_or_chain.toml --build-dir ./build_circt
```

Run the tests, including the CIRCT-gated ones:

```bash
$ cargo test --features circt
```

If MLIR lives in a separate prefix from CIRCT, also set `MLIR_DIR`. Plain
`cargo build` (no feature) still builds the ngspice-only flow without CIRCT.
