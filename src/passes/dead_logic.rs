//! Removes gates whose output reaches nothing: no cell input and no module
//! output port. Repeats until nothing changes (removing one gate can make
//! its drivers dead too), then drops nets left with no pins at all.
//!
//! Module instances are left alone; only primitive gates are removed.

use crate::ir::{CellKind, Design, IrError};
use crate::passes::Pass;

pub struct DeadLogicElimination;

impl Pass for DeadLogicElimination {
    fn name(&self) -> &'static str {
        "dead-logic-elimination"
    }

    fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
        for mid in design.modules().ids() {
            let m = design.module_mut(mid);
            loop {
                let dead: Vec<_> = m
                    .cells()
                    .iter()
                    .filter(|(_, c)| matches!(c.kind, CellKind::Gate(_)))
                    .map(|(id, _)| id)
                    .filter(|&id| m.output_nets(id).iter().all(|&n| m.fanout(n) == 0))
                    .collect();
                if dead.is_empty() {
                    break;
                }
                for cell in dead {
                    m.remove_cell(cell);
                }
            }
            for net in m.nets().ids() {
                if m.nets()[net].pins.is_empty() {
                    m.remove_net(net)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Direction, GateType, Module};

    /// a -> INV(u0) -> y (output port), plus a dead chain a -> INV(u1) -> INV(u2) -> (nothing).
    fn design() -> Design {
        let mut d = Design::new();
        let id = d.add_module("m").unwrap();
        d.set_top(id);
        let m: &mut Module = d.module_mut(id);
        let a = m.add_net("a");
        let y = m.add_net("y");
        let n1 = m.add_net("n1");
        let n2 = m.add_net("n2");
        let pa = m.add_port("a", Direction::Input);
        let py = m.add_port("y", Direction::Output);
        m.connect(m.ports()[pa].pin, a).unwrap();
        m.connect(m.ports()[py].pin, y).unwrap();
        for (name, input, output) in [("u0", a, y), ("u1", a, n1), ("u2", n1, n2)] {
            let c = m.add_gate(name, GateType::Inv);
            m.connect(m.cell_pin(c, "in0").unwrap(), input).unwrap();
            m.connect(m.cell_pin(c, "y").unwrap(), output).unwrap();
        }
        d
    }

    #[test]
    fn removes_dead_chain_keeps_live_logic() {
        let mut d = design();
        DeadLogicElimination.run(&mut d).unwrap();
        d.verify().unwrap();
        let m = d.module(d.top().unwrap());
        let names: Vec<_> = m.cells().iter().map(|(_, c)| c.name.as_str()).collect();
        assert_eq!(names, vec!["u0"]);
        let nets: Vec<_> = m.nets().iter().map(|(_, n)| n.name.as_str()).collect();
        assert_eq!(nets, vec!["a", "y"]);
    }
}
