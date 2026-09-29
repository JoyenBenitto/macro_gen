# Building

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cd macro_gen
$ cargo build
```

For an optimized binary, build with `--release` instead (output lands in
`target/release/macro_gen`):

```bash
$ cargo build --release
```

To also build the CMOS backend, point `CIRCT_DIR` at a pre-built CIRCT and
enable the `circt` feature (see [CMOS Backend](./cmos-backend.md#building)):

```bash
$ export CIRCT_DIR=/path/to/circt
$ cargo build --release --features circt
```

Run the test suite with:

```bash
$ cargo test
```

and with the CIRCT gated tests included:

```bash
$ cargo test --features circt
```
