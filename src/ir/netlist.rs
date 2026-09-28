//! Internal gate-level netlist IR.
//!
//! This is the boundary between "parse CIRCT" ([`crate::ir::from_circt`],
//! feature `circt`), "size gates" ([`crate::sizing`]), and "emit SPICE"
//! ([`crate::spice_gen`]) -- none of the latter two touch MLIR/CIRCT types.

#[derive(Debug, Clone, Default)]
pub struct Netlist {
    pub modules: Vec<Module>,
}

impl Netlist {
    /// Finds a module by name, e.g. the `[circt].top_module` config value.
    pub fn find_module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub ports: Vec<Port>,
    pub instances: Vec<Instance>,
    pub nets: Vec<Net>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub direction: Direction,
    /// Bit width; 2-input/1-output combinational gates only use width 1,
    /// but `hw.module` ports can be wider in general.
    pub width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u32);

#[derive(Debug, Clone)]
pub struct Net {
    pub id: NetId,
    /// Derived from the CIRCT value's name hint when present, otherwise a
    /// synthesized `net_<id>`.
    pub name: String,
    pub driver: Option<PinRef>,
    pub loads: Vec<PinRef>,
}

impl Net {
    /// Fanout count: how many gate/instance inputs this net drives. This is
    /// the `H` (electrical fanout) input to logical-effort sizing.
    pub fn fanout(&self) -> usize {
        self.loads.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(pub u32);

#[derive(Debug, Clone)]
pub struct PinRef {
    pub instance: InstanceId,
    pub pin: String,
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub id: InstanceId,
    pub name: String,
    pub kind: InstanceKind,
    pub inputs: Vec<NetId>,
    pub outputs: Vec<NetId>,
}

#[derive(Debug, Clone)]
pub enum InstanceKind {
    /// A primitive logic gate, sized either via logical effort or (if
    /// tagged in `config.custom_cells`) a custom analytical/sweep flow.
    Gate(GateType),
    /// An instance of another `hw.module` in this netlist -- hierarchy.
    ModuleInstance(String),
}

/// The 2-input primitive gate set macro_gen's gate library and SPICE
/// templates cover today. `comb`'s ops are N-ary in MLIR; wider `and`/`or`/
/// `xor` are decomposed into a chain of these during IR construction (see
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

impl GateType {
    /// The name of this gate's SPICE template,
    /// `constants/gates/<name>.spice.j2`.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_module_looks_up_by_name() {
        let netlist = Netlist {
            modules: vec![
                Module {
                    name: "chain".to_string(),
                    ports: vec![],
                    instances: vec![],
                    nets: vec![],
                },
                Module {
                    name: "leaf".to_string(),
                    ports: vec![],
                    instances: vec![],
                    nets: vec![],
                },
            ],
        };

        assert!(netlist.find_module("leaf").is_some());
        assert!(netlist.find_module("missing").is_none());
    }

    #[test]
    fn net_fanout_counts_loads() {
        let net = Net {
            id: NetId(0),
            name: "n0".to_string(),
            driver: Some(PinRef {
                instance: InstanceId(0),
                pin: "y".to_string(),
            }),
            loads: vec![
                PinRef { instance: InstanceId(1), pin: "a".to_string() },
                PinRef { instance: InstanceId(2), pin: "a".to_string() },
            ],
        };
        assert_eq!(net.fanout(), 2);
    }

    #[test]
    fn gate_type_library_names_are_stable() {
        assert_eq!(GateType::Nand2.library_name(), "nand2");
        assert_eq!(GateType::Inv.library_name(), "inv");
    }
}
