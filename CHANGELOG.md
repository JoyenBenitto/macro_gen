# CHANGELOG

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0]

### Added
- Reference inverter synthesis: sizes the PMOS against a target switching threshold, using device parameters extracted from ngspice.
- `--config` / `--build-dir` CLI flags.
- CI: build, test, and publish to crates.io on merge to `main`.
