# Introduction

`macro_gen` is an automated digital IC macro generator. It drives
[ngspice](http://ngspice.sourceforge.net/) in process via FFI to characterize a
reference inverter against a PDK supplied configuration. With the `circt`
feature it goes further: it reads a combinational CIRCT design, builds each gate
as a CMOS pull-up and pull-down network, sizes every transistor by logical effort
relative to that inverter, optionally buffers the outputs to the optimal number
of stages, and writes SPICE, structural Verilog and a sizing report.

Source code: [github.com/JoyenBenitto/macro_gen](https://github.com/JoyenBenitto/macro_gen)

New here? [Getting Started](./getting-started.md) is the fastest path from clone
to a generated macro. [CMOS Backend](./cmos-backend.md) covers sizing a design,
and [Benchmarks](./benchmarks.md) the benchmark suite.
