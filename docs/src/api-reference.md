# API Reference

The full rustdoc API reference, generated from the crate's doc comments, is hosted at
[`/api/`](../api/index.html).

It covers the internal module structure:

- `config` — PDK-adapter TOML config types.
- `validator` — config validation.
- `characterization` — drives ngspice to extract device parameters and size cells:
  - `device_params`
  - `inverter`
  - `ngspice_ffi`
  - `normalize`
  - `paths`
