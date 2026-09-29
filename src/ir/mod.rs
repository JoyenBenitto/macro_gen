//! The gate-level netlist IR every backend stage works on.
//!
//! A [`Design`] holds module definitions. Each [`Module`] is a hypergraph:
//! [`Cell`]s (gates or instances of other modules) are nodes, [`Net`]s are
//! hyperedges, and [`Pin`]s are where the two meet. Module boundary
//! [`Port`]s own a pin too, so a port sits on its net like any cell pin.
//!
//! Entities live in tombstoning arenas ([`arena`]) keyed by typed IDs, so
//! passes (see [`crate::passes`]) can add, rewire and remove nodes without
//! invalidating IDs held elsewhere.

pub mod arena;
pub mod cell;
pub mod design;
pub mod error;
pub mod module;

#[cfg(feature = "circt")]
pub mod from_circt;

pub use arena::{CellId, ModuleId, NetId, PinId, PortId};
pub use cell::{Cell, CellKind, GateType};
pub use design::Design;
pub use error::IrError;
pub use module::{Direction, Module, Net, Pin, PinOwner, Port};
