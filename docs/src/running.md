# Running

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

Example with a custom build directory:

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml --build-dir /tmp/my_run
```

With the `circt` feature, a config with `[circt]` and `[sizing]` sections also
sizes a design. It always writes a SPICE netlist and a report, and optionally
structural Verilog and a buffered netlist:

```bash
$ ./target/release/macro_gen --config benchmarks/c17/c17.toml --emit-verilog --add-buffer
```

See [CMOS Backend](./cmos-backend.md) for details, [Benchmarks](./benchmarks.md)
for running the whole suite, and [CLI Reference](./cli-reference.md) for all
flags.
