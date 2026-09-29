# Prerequisites

- A Rust toolchain (install via [rustup](https://rustup.rs/))
- `ngspice`, plus its shared-library development package, since `macro_gen` drives ngspice
  in-process via FFI rather than shelling out to the CLI:
  - Debian/Ubuntu: `sudo apt install ngspice libngspice0-dev`
  - The build script (`build.rs`) uses `pkg-config` to locate `ngspice.pc`; if it's
    missing, `cargo build` fails immediately with an explanatory message rather than a
    cryptic linker error.
- For the CMOS backend only: a pre-built [CIRCT](https://circt.llvm.org/)
  (with its MLIR), pointed to by `CIRCT_DIR`, and Python 3.11 or newer to run
  the benchmark suite.
