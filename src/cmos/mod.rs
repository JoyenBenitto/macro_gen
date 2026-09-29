//! Transistor-level view of a gate: a CMOS stage is a pull-down network of
//! NMOS and its dual pull-up network of PMOS, both series-parallel trees.
//!
//! Widths here are *relative*: 1.0 is the reference inverter's NMOS (for
//! the pull-down) or PMOS (for the pull-up), and each stack is sized to
//! drive as well as that inverter. Everything logical-effort needs (per-input
//! `g`, parasitic `p`) follows from those widths and `gamma = Wp/Wn` of the
//! reference inverter. Pure math: no IR, config or FFI dependency.

pub mod network;

pub use network::{CmosStage, Fet, Network, Node};
