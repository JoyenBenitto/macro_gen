# Introduction

`macro_gen` is an automated digital IC macro generator. It drives [ngspice](http://ngspice.sourceforge.net/)
in-process via FFI to characterize and size cells — starting with an inverter — against a
PDK-supplied configuration, then emits a spice deck.

Source code: [github.com/JoyenBenitto/macro_gen](https://github.com/JoyenBenitto/macro_gen)

New here? Jump to [Getting Started](./getting-started.md) for the fastest path from clone to a
generated macro.
