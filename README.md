# macro_gen

Macro_gen is an automated digital IC macro generator

# Building the source

```bash
$ git clone https://github.com/JoyenBenitto/macro_gen.git
$ cargo build
```

# Running

```bash
$ ./target/debug/macro_gen --config ./examples/130nm.toml
```

# Logging

Logging defaults to `info` level, so you'll see output without any extra setup. Control the log level with the `MACROGEN_LOG` environment variable (accepts `error`, `warn`, `info`, `debug`, `trace`):

```bash
$ MACROGEN_LOG=debug ./target/debug/macro_gen --config ./examples/130nm.toml
```
