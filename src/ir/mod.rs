//! Internal gate-level netlist IR: the boundary between reading CIRCT
//! `hw`+`comb` input, logical-effort/custom sizing, and SPICE emission.

pub mod error;
pub mod netlist;

#[cfg(feature = "circt")]
pub mod from_circt;

pub use error::IrError;
pub use netlist::{
    Direction, GateType, Instance, InstanceId, InstanceKind, Module, Net, NetId, Netlist, PinRef,
    Port,
};
