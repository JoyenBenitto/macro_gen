# Getting Started

This walks through the fastest path from a clone to a generated macro, using the SKY130
example config shipped in the repo (`examples/130nm.toml`).

## 1. Clone and build

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cd macro_gen
$ cargo build
```

See [Prerequisites](./prerequisites.md) if the build fails looking for `ngspice`.

## 2. Run against the example config

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

`examples/130nm.toml` is a PDK-adapter config for the SKY130 process: it points at the
NMOS/PMOS device models, the process corner, and a reference inverter's starting geometry
and sizing-sweep parameters.

## 3. Find the output

By default, output lands under `./build/`:

- `./build/spice/` — the final generated spice deck(s).
- `./build/.macro_gen_project/` — intermediate/scratch files: characterization decks,
  sizing-sweep candidates, and CSV reports.

Use `--build-dir` to point elsewhere; see [CLI Reference](./cli-reference.md).

## Next steps

- [CLI Reference](./cli-reference.md) for all flags.
- [Logging](./logging.md) to see per-iteration characterization/sizing-sweep detail.
- [API Reference](./api-reference.md) for the internal module structure.
