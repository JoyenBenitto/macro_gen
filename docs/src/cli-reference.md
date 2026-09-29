# CLI Reference

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--config <CONFIG>` | `-c` | *(required)* | Path to the PDK-adapter TOML config file (see `examples/130nm.toml`) |
| `--build-dir <BUILD_DIR>` | `-b` | `./build` | Output directory. Final decks land under `<build-dir>/spice/`; intermediate/scratch files (characterization decks, sweep candidates, CSV reports) land under `<build-dir>/.macro_gen_project/` |
| `--add-buffer [MODE]` | | off; `invertible` if given without a value | CIRCT flow only. Buffers every output to the delay-optimal `round(log_rho F)` stages (`rho` = `sizing.stage_effort`, default 4) and writes a structural, PD-clean Verilog netlist to `<build-dir>/verilog/<top>.v` (plus black-box cell declarations in `<top>_cells.v`). `invertible` allows any number of added inverters, so an output may come out inverted (tagged `macro_gen.inverted.<port>`). `non-invertible` adds inverters in pairs (inv + inv), so the logic function is kept |
| `--emit-buffered-spice` | | off | CIRCT flow only. Also writes the buffered netlist as `<build-dir>/spice/<top>_buffered.spice`. Implies `--add-buffer` (`invertible` unless a mode is given) |
| `--help` | `-h` | | Print help |
| `--version` | `-V` | | Print version |
