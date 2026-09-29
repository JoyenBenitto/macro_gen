//! A module as a hypergraph: cells are nodes, nets are hyperedges, and pins
//! are the connection points between them.
//!
//! Every connection is stored in both directions (`Pin::net` and
//! `Net::pins`). The arenas are private, so all mutation goes through the
//! methods here, which keep both sides in sync; [`Module::verify`] checks
//! that they still are.

use crate::ir::arena::{Arena, CellId, ModuleId, NetId, PinId, PortId};
use crate::ir::cell::{Cell, CellKind, GATE_OUTPUT_PIN, GateType};
use crate::ir::error::IrError;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

/// A module boundary port, backed by one pin inside the module.
#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub dir: Direction,
    pub pin: PinId,
}

/// A hyperedge: every pin in `pins` is electrically the same node. At most
/// one of them drives it (see [`Pin::drives_net`]).
#[derive(Debug, Clone)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinOwner {
    Cell(CellId),
    Port(PortId),
}

/// A connection point on a cell or port. `dir` is from the owner's point of
/// view: a cell's `Output` pin, like a module's `Input` port, drives its net
/// from inside the module.
#[derive(Debug, Clone)]
pub struct Pin {
    pub owner: PinOwner,
    pub name: String,
    pub dir: Direction,
    pub net: Option<NetId>,
}

impl Pin {
    /// Whether this pin is its net's driver (as opposed to a load).
    pub fn drives_net(&self) -> bool {
        matches!(
            (self.owner, self.dir),
            (PinOwner::Cell(_), Direction::Output) | (PinOwner::Port(_), Direction::Input)
        )
    }
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    /// User-added (discardable) attributes from the input, e.g.
    /// `macro_gen.cell = "complex"`.
    pub attrs: BTreeMap<String, String>,
    ports: Arena<PortId, Port>,
    cells: Arena<CellId, Cell>,
    nets: Arena<NetId, Net>,
    pins: Arena<PinId, Pin>,
}

impl Module {
    pub fn new(name: impl Into<String>) -> Self {
        Module {
            name: name.into(),
            attrs: BTreeMap::new(),
            ports: Arena::default(),
            cells: Arena::default(),
            nets: Arena::default(),
            pins: Arena::default(),
        }
    }

    // ---- read access -------------------------------------------------

    pub fn ports(&self) -> &Arena<PortId, Port> {
        &self.ports
    }
    pub fn cells(&self) -> &Arena<CellId, Cell> {
        &self.cells
    }
    pub fn nets(&self) -> &Arena<NetId, Net> {
        &self.nets
    }
    pub fn pins(&self) -> &Arena<PinId, Pin> {
        &self.pins
    }

    /// Tagged `macro_gen.cell = "complex"` in the input: build the whole
    /// module as one pull-up/pull-down network instead of gate by gate.
    pub fn is_complex_cell(&self) -> bool {
        self.attrs.get("macro_gen.cell").map(String::as_str) == Some("complex")
    }

    pub fn port_by_name(&self, name: &str) -> Option<PortId> {
        self.ports.iter().find(|(_, p)| p.name == name).map(|(id, _)| id)
    }

    /// The net a port is connected to, if any.
    pub fn port_net(&self, port: PortId) -> Option<NetId> {
        self.pins[self.ports[port].pin].net
    }

    /// The cell's pin called `name` (`"in0"`, `"y"`, or an instance's port name).
    pub fn cell_pin(&self, cell: CellId, name: &str) -> Option<PinId> {
        self.cells[cell].pins.iter().copied().find(|&p| self.pins[p].name == name)
    }

    /// Nets on the cell's input pins, in pin order (unconnected pins skipped).
    pub fn input_nets(&self, cell: CellId) -> Vec<NetId> {
        self.cell_nets(cell, Direction::Input)
    }

    /// Nets on the cell's output pins, in pin order (unconnected pins skipped).
    pub fn output_nets(&self, cell: CellId) -> Vec<NetId> {
        self.cell_nets(cell, Direction::Output)
    }

    fn cell_nets(&self, cell: CellId, dir: Direction) -> Vec<NetId> {
        self.cells[cell]
            .pins
            .iter()
            .map(|&p| &self.pins[p])
            .filter(|p| p.dir == dir)
            .filter_map(|p| p.net)
            .collect()
    }

    /// The pin driving `net`, if any.
    pub fn driver(&self, net: NetId) -> Option<PinId> {
        self.nets[net].pins.iter().copied().find(|&p| self.pins[p].drives_net())
    }

    /// Every pin `net` drives: cell inputs and module output ports.
    pub fn loads(&self, net: NetId) -> impl Iterator<Item = PinId> + '_ {
        self.nets[net].pins.iter().copied().filter(|&p| !self.pins[p].drives_net())
    }

    /// Number of loads on `net`.
    pub fn fanout(&self, net: NetId) -> usize {
        self.loads(net).count()
    }

    /// The cell owning `pin`, or `None` for a port pin.
    pub fn pin_cell(&self, pin: PinId) -> Option<CellId> {
        match self.pins[pin].owner {
            PinOwner::Cell(c) => Some(c),
            PinOwner::Port(_) => None,
        }
    }

    /// `module.cell.pin` / `module.port`, for error messages.
    pub fn pin_path(&self, pin: PinId) -> String {
        let p = &self.pins[pin];
        match p.owner {
            PinOwner::Cell(c) => format!("{}.{}.{}", self.name, self.cells[c].name, p.name),
            PinOwner::Port(_) => format!("{}.{}", self.name, p.name),
        }
    }

    // ---- mutation ----------------------------------------------------

    pub fn add_net(&mut self, name: impl Into<String>) -> NetId {
        self.nets.alloc(Net { name: name.into(), pins: Vec::new() })
    }

    pub fn rename_net(&mut self, net: NetId, name: impl Into<String>) {
        self.nets[net].name = name.into();
    }

    /// Adds a boundary port and its pin (unconnected).
    pub fn add_port(&mut self, name: impl Into<String>, dir: Direction) -> PortId {
        let name = name.into();
        let port = self.ports.alloc(Port { name: name.clone(), dir, pin: PinId(u32::MAX) });
        let pin = self.pins.alloc(Pin { owner: PinOwner::Port(port), name, dir, net: None });
        self.ports[port].pin = pin;
        port
    }

    /// Adds a gate with unconnected pins `in0..inN` and `y`.
    pub fn add_gate(&mut self, name: impl Into<String>, gate: GateType) -> CellId {
        let mut spec: Vec<(String, Direction)> = (0..gate.num_inputs())
            .map(|i| (GateType::input_pin_name(i), Direction::Input))
            .collect();
        spec.push((GATE_OUTPUT_PIN.to_string(), Direction::Output));
        self.add_cell(name.into(), CellKind::Gate(gate), &spec)
    }

    /// Adds an instance of `target` with one unconnected pin per entry of
    /// `ports` (the target's port names/directions, in order). Prefer
    /// [`crate::ir::Design::add_instance`], which reads them from the target.
    pub fn add_instance(
        &mut self,
        name: impl Into<String>,
        target: ModuleId,
        ports: &[(String, Direction)],
    ) -> CellId {
        self.add_cell(name.into(), CellKind::Instance(target), ports)
    }

    fn add_cell(&mut self, name: String, kind: CellKind, spec: &[(String, Direction)]) -> CellId {
        let cell = self.cells.alloc(Cell { name, kind, pins: Vec::new(), attrs: BTreeMap::new() });
        for (pin_name, dir) in spec {
            let pin = self.pins.alloc(Pin {
                owner: PinOwner::Cell(cell),
                name: pin_name.clone(),
                dir: *dir,
                net: None,
            });
            self.cells[cell].pins.push(pin);
        }
        cell
    }

    /// Connects `pin` to `net`. Fails if the pin is already connected, or if
    /// it would be a second driver on `net`.
    pub fn connect(&mut self, pin: PinId, net: NetId) -> Result<(), IrError> {
        if self.pins[pin].net.is_some() {
            return Err(IrError::PinAlreadyConnected(self.pin_path(pin)));
        }
        if self.pins[pin].drives_net() && self.driver(net).is_some() {
            return Err(IrError::MultiDriverNet(format!("{}.{}", self.name, self.nets[net].name)));
        }
        self.pins[pin].net = Some(net);
        self.nets[net].pins.push(pin);
        Ok(())
    }

    /// Detaches `pin` from its net (no-op if unconnected).
    pub fn disconnect(&mut self, pin: PinId) {
        if let Some(net) = self.pins[pin].net.take() {
            self.nets[net].pins.retain(|&p| p != pin);
        }
    }

    /// Moves every load of `from` onto `to`, leaving `from` with only its
    /// driver (if any). The usual first step of replacing a cell's output.
    pub fn move_loads(&mut self, from: NetId, to: NetId) -> Result<(), IrError> {
        let loads: Vec<PinId> = self.loads(from).collect();
        for pin in loads {
            self.disconnect(pin);
            self.connect(pin, to)?;
        }
        Ok(())
    }

    /// Changes a gate's type in place, keeping its connections. Only allowed
    /// between gates with the same pins (e.g. AND2 → NAND2).
    pub fn set_gate_type(&mut self, cell: CellId, gate: GateType) -> Result<(), IrError> {
        let c = &mut self.cells[cell];
        let from = match c.kind {
            CellKind::Gate(old) if old.num_inputs() == gate.num_inputs() => {
                c.kind = CellKind::Gate(gate);
                return Ok(());
            }
            CellKind::Gate(old) => old.library_name(),
            CellKind::Instance(_) => "a module instance",
        };
        Err(IrError::GateArityMismatch(c.name.clone(), from, gate.library_name()))
    }

    pub fn set_cell_attr(&mut self, cell: CellId, key: impl Into<String>, value: impl Into<String>) {
        self.cells[cell].attrs.insert(key.into(), value.into());
    }

    /// Disconnects and removes a cell and its pins. Nets it touched stay
    /// (possibly now empty -- see [`Module::remove_net`]).
    pub fn remove_cell(&mut self, cell: CellId) {
        let Some(c) = self.cells.remove(cell) else { return };
        for pin in c.pins {
            self.disconnect(pin);
            self.pins.remove(pin);
        }
    }

    /// Removes a net that no pin is connected to.
    pub fn remove_net(&mut self, net: NetId) -> Result<(), IrError> {
        if !self.nets[net].pins.is_empty() {
            return Err(IrError::NetInUse(format!("{}.{}", self.name, self.nets[net].name)));
        }
        self.nets.remove(net);
        Ok(())
    }

    // ---- verification ------------------------------------------------

    /// Checks this module's own invariants: pin ↔ net back-references
    /// agree, pins belong to live owners that list them, nets have at most
    /// one driver, and gates have the pins their type requires. Instance
    /// targets are checked by [`crate::ir::Design::verify`].
    pub fn verify(&self) -> Result<(), IrError> {
        let fail = |msg: String| Err(IrError::Verify(self.name.clone(), msg));

        for (pid, pin) in self.pins.iter() {
            let listed = match pin.owner {
                PinOwner::Cell(c) => self.cells.get(c).is_some_and(|c| c.pins.contains(&pid)),
                PinOwner::Port(p) => self.ports.get(p).is_some_and(|p| p.pin == pid),
            };
            if !listed {
                return fail(format!("{pid:?} '{}' is not listed by its owner {:?}", pin.name, pin.owner));
            }
            if let Some(net) = pin.net {
                if !self.nets.get(net).is_some_and(|n| n.pins.contains(&pid)) {
                    return fail(format!("{} points at {net:?}, which does not list it", self.pin_path(pid)));
                }
            }
        }

        for (nid, net) in self.nets.iter() {
            let mut drivers = 0;
            for &pid in &net.pins {
                match self.pins.get(pid) {
                    Some(p) if p.net == Some(nid) => drivers += usize::from(p.drives_net()),
                    _ => return fail(format!("net '{}' lists {pid:?}, which is not connected to it", net.name)),
                }
            }
            if drivers > 1 {
                return Err(IrError::MultiDriverNet(format!("{}.{}", self.name, net.name)));
            }
        }

        for (pid, port) in self.ports.iter() {
            if !self.pins.get(port.pin).is_some_and(|p| p.owner == PinOwner::Port(pid)) {
                return fail(format!("port '{}' has no pin of its own", port.name));
            }
        }

        for (_, cell) in self.cells.iter() {
            if let CellKind::Gate(g) = cell.kind {
                let inputs = cell.pins.iter().filter(|&&p| self.pins[p].dir == Direction::Input).count();
                let outputs = cell.pins.len() - inputs;
                if inputs != g.num_inputs() || outputs != 1 {
                    return fail(format!(
                        "{} cell '{}' has {inputs} input / {outputs} output pins",
                        g.library_name(),
                        cell.name
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// y = (a & b) | c, built through the API.
    fn and_or() -> (Module, CellId, CellId) {
        let mut m = Module::new("and_or");
        let nets: Vec<NetId> = ["a", "b", "c", "ab", "y"].iter().map(|n| m.add_net(*n)).collect();
        for (name, dir, net) in [
            ("a", Direction::Input, nets[0]),
            ("b", Direction::Input, nets[1]),
            ("c", Direction::Input, nets[2]),
            ("y", Direction::Output, nets[4]),
        ] {
            let port = m.add_port(name, dir);
            m.connect(m.ports()[port].pin, net).unwrap();
        }
        let and = m.add_gate("u0", GateType::And2);
        let or = m.add_gate("u1", GateType::Or2);
        for (cell, pin, net) in [
            (and, "in0", nets[0]),
            (and, "in1", nets[1]),
            (and, "y", nets[3]),
            (or, "in0", nets[3]),
            (or, "in1", nets[2]),
            (or, "y", nets[4]),
        ] {
            let p = m.cell_pin(cell, pin).unwrap();
            m.connect(p, net).unwrap();
        }
        (m, and, or)
    }

    #[test]
    fn built_module_verifies_and_answers_queries() {
        let (m, and, or) = and_or();
        m.verify().unwrap();
        let ab = m.output_nets(and)[0];
        assert_eq!(m.nets()[ab].name, "ab");
        assert_eq!(m.fanout(ab), 1);
        assert_eq!(m.driver(ab), m.cell_pin(and, "y"));
        assert_eq!(m.input_nets(or), vec![ab, m.port_net(m.port_by_name("c").unwrap()).unwrap()]);
        // Input port drives its net; output port is a load.
        let a = m.port_net(m.port_by_name("a").unwrap()).unwrap();
        assert!(m.pins()[m.driver(a).unwrap()].owner == PinOwner::Port(m.port_by_name("a").unwrap()));
        let y = m.output_nets(or)[0];
        assert_eq!(m.fanout(y), 1);
    }

    #[test]
    fn second_driver_is_rejected() {
        let (mut m, and, _) = and_or();
        let ab = m.output_nets(and)[0];
        let extra = m.add_gate("u2", GateType::Inv);
        let y = m.cell_pin(extra, "y").unwrap();
        assert!(matches!(m.connect(y, ab), Err(IrError::MultiDriverNet(_))));
    }

    #[test]
    fn remove_cell_leaves_no_dangling_pins() {
        let (mut m, and, _) = and_or();
        let ab = m.output_nets(and)[0];
        m.remove_cell(and);
        m.verify().unwrap();
        assert!(!m.cells().contains(and));
        assert_eq!(m.driver(ab), None);
        assert_eq!(m.fanout(ab), 1);
        assert!(matches!(m.remove_net(ab), Err(IrError::NetInUse(_))));
    }

    #[test]
    fn set_gate_type_keeps_connections_and_checks_arity() {
        let (mut m, and, _) = and_or();
        m.set_gate_type(and, GateType::Nand2).unwrap();
        assert_eq!(m.cells()[and].kind, CellKind::Gate(GateType::Nand2));
        m.verify().unwrap();
        assert!(matches!(m.set_gate_type(and, GateType::Inv), Err(IrError::GateArityMismatch(..))));
    }
}
