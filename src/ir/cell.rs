//! Hypergraph nodes: cells, and the primitive gate set they can be.

use crate::ir::arena::{ModuleId, PinId, StageId};
use std::collections::BTreeMap;

/// A node in a module's hypergraph: a primitive gate or an instance of
/// another module. Its connections are its [`pins`](Cell::pins), in
/// declaration order (gate inputs `in0..inN`, then output `y`; instance
/// pins follow the target module's port order).
#[derive(Debug, Clone)]
pub struct Cell {
    pub name: String,
    pub kind: CellKind,
    pub pins: Vec<PinId>,
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Gate(GateType),
    /// Hierarchy: an instance of another module in the same [`crate::ir::Design`].
    Instance(ModuleId),
    /// A transistor-level CMOS stage (see [`crate::cmos`]) owned by the
    /// module, produced by lowering gates (`passes::cmos_map`). Same pin
    /// convention as a gate: `in0..inN`, then `y`.
    Cmos(StageId),
}

/// The primitive gate set. `comb`'s ops are N-ary in MLIR; wider
/// `and`/`or`/`xor` are decomposed into chains of these on import (see
/// `ir::from_circt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateType {
    Inv,
    And2,
    Nand2,
    Or2,
    Nor2,
    Xor2,
    Xnor2,
    Mux2,
}

/// Name of every gate's single output pin.
pub const GATE_OUTPUT_PIN: &str = "y";

impl GateType {
    /// Stable lowercase name, used for generated cell names (`and2_0`).
    pub fn library_name(&self) -> &'static str {
        match self {
            GateType::Inv => "inv",
            GateType::And2 => "and2",
            GateType::Nand2 => "nand2",
            GateType::Or2 => "or2",
            GateType::Nor2 => "nor2",
            GateType::Xor2 => "xor2",
            GateType::Xnor2 => "xnor2",
            GateType::Mux2 => "mux2",
        }
    }

    /// Number of input pins (`in0..in{n-1}`). Every gate has one output.
    pub fn num_inputs(&self) -> usize {
        match self {
            GateType::Inv => 1,
            GateType::Mux2 => 3,
            _ => 2,
        }
    }

    /// Name of input pin `i`.
    pub fn input_pin_name(i: usize) -> String {
        format!("in{i}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_type_library_names_are_stable() {
        assert_eq!(GateType::Nand2.library_name(), "nand2");
        assert_eq!(GateType::Inv.library_name(), "inv");
    }

    #[test]
    fn gate_arity() {
        assert_eq!(GateType::Inv.num_inputs(), 1);
        assert_eq!(GateType::Nor2.num_inputs(), 2);
        assert_eq!(GateType::Mux2.num_inputs(), 3);
    }
}
