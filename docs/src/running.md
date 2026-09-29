# Running

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

Example with a custom build directory:

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml --build-dir /tmp/my_run
```

With the `circt` feature, a config that has `[circt]` and `[sizing]` sections
also runs the CMOS backend. It writes a sized SPICE netlist and, with
`--add-buffer`, a buffered structural Verilog netlist:

```bash
$ cargo run --features circt -- --config examples/circt_and_or_chain.toml \
      --build-dir ./build_circt --add-buffer --emit-buffered-spice
```

See [CLI Reference](./cli-reference.md) for all available flags.
