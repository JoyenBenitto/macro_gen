# CLI Reference

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--config <CONFIG>` | `-c` | *(required)* | Path to the PDK-adapter TOML config file (see `examples/130nm.toml`) |
| `--build-dir <BUILD_DIR>` | `-b` | `./build` | Output directory. Final decks land under `<build-dir>/spice/`. Intermediate files (characterization decks, sweep candidates, CSV reports) land under `<build-dir>/.macro_gen_project/` |
| `--emit-verilog` | | off | CIRCT flow only. Also writes the sized netlist as structural, PD clean Verilog to `<build-dir>/verilog/<top>.v`, plus black-box cell declarations in `<top>_cells.v` |
| `--add-buffer [MODE]` | | off; `invertible` if given without a value | CIRCT flow only. Buffers every output to the delay optimal `round(log_rho F)` stages (`rho` = `sizing.stage_effort`, default 4). The SPICE and Verilog outputs are then the buffered design, under the same names. `invertible` allows any number of added inverters, so an output may come out inverted (tagged `macro_gen.inverted.<port>`). `non-invertible` adds inverters in pairs (inv + inv), so the logic function is kept |
| `--help` | `-h` | | Print help |
| `--version` | `-V` | | `-V` prints the version. `--version` also prints the git commit, build time, profile, target, rustc, features and ngspice version |
