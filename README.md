# macro_gen

Macro_gen is an automated digital IC macro generator

# Prerequisites

- A Rust toolchain (install via [rustup](https://rustup.rs/))
- `ngspice`, plus its shared-library development package, since `macro_gen` drives ngspice in-process via FFI rather than shelling out to the CLI:
  - Debian/Ubuntu: `sudo apt install ngspice libngspice0-dev`
  - The build script (`build.rs`) uses `pkg-config` to locate `ngspice.pc`; if it's missing, `cargo build` fails immediately with an explanatory message rather than a cryptic linker error.

# Building the source

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cd macro_gen
$ cargo build
```

For an optimized binary, build with `--release` instead (output lands in `target/release/macro_gen`):

```bash
$ cargo build --release
```

Run the test suite with:

```bash
$ cargo test
```

# Running

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

## CLI options

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--config <CONFIG>` | `-c` | *(required)* | Path to the PDK-adapter TOML config file (see `examples/130nm.toml`) |
| `--build-dir <BUILD_DIR>` | `-b` | `./build` | Output directory. Final decks land under `<build-dir>/spice/`; intermediate/scratch files (characterization decks, sweep candidates, CSV reports) land under `<build-dir>/.macro_gen_project/` |
| `--help` | `-h` | | Print help |
| `--version` | `-V` | | Print version |

Example with a custom build directory:

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml --build-dir /tmp/my_run
```

# Logging

Logging defaults to `info` level, so you'll see output without any extra setup. Control the log level with the `MACROGEN_LOG` environment variable (accepts `error`, `warn`, `info`, `debug`, `trace`):

```bash
$ MACROGEN_LOG=debug ./target/debug/macro_gen --config ./examples/130nm.toml
```

`debug` also surfaces per-iteration characterization/sizing-sweep detail that's hidden at the default `info` level.
