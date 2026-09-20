# Logging

Logging defaults to `info` level, so you'll see output without any extra setup. Control the
log level with the `MACROGEN_LOG` environment variable (accepts `error`, `warn`, `info`,
`debug`, `trace`):

```bash
$ MACROGEN_LOG=debug ./target/debug/macro_gen --config ./examples/130nm.toml
```

`debug` also surfaces per-iteration characterization/sizing-sweep detail that's hidden at
the default `info` level.
