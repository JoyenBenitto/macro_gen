# CLI Reference

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--config <CONFIG>` | `-c` | *(required)* | Path to the PDK-adapter TOML config file (see `examples/130nm.toml`) |
| `--build-dir <BUILD_DIR>` | `-b` | `./build` | Output directory. Final decks land under `<build-dir>/spice/`; intermediate/scratch files (characterization decks, sweep candidates, CSV reports) land under `<build-dir>/.macro_gen_project/` |
| `--help` | `-h` | | Print help |
| `--version` | `-V` | | Print version |
