# Prerequisites

- A Rust toolchain (install via [rustup](https://rustup.rs/))
- `ngspice`, plus its shared-library development package, since `macro_gen` drives ngspice
  in-process via FFI rather than shelling out to the CLI:
  - Debian/Ubuntu: `sudo apt install ngspice libngspice0-dev`
  - The build script (`build.rs`) uses `pkg-config` to locate `ngspice.pc`; if it's
    missing, `cargo build` fails immediately with an explanatory message rather than a
    cryptic linker error.
