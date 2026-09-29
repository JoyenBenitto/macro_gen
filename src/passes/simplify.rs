//! Local gate rewrites that save CMOS stages.
//!
//! CIRCT has no NAND/NOR: `~(a & b)` arrives as AND2 feeding an INV
//! (`comb.xor %x, %true`). Lowered naively that is NAND2 + INV + INV:
//! three stages for one. This pass folds an INV into the gate driving it
//! when the INV is that gate's only load:
//!
//! - INV(AND2) -> NAND2, INV(OR2) -> NOR2
//! - INV(NAND2) -> AND2, INV(NOR2) -> OR2
//!
//! The gate takes over the INV's output net, so a port name on that net is
//! kept. Repeats until nothing changes.

use crate::ir::cell::GATE_OUTPUT_PIN;
use crate::ir::{CellKind, Design, GateType, IrError};
use crate::passes::Pass;

pub struct GateSimplify;

fn complement(g: GateType) -> Option<GateType> {
    Some(match g {
        GateType::And2 => GateType::Nand2,
        GateType::Or2 => GateType::Nor2,
        GateType::Nand2 => GateType::And2,
        GateType::Nor2 => GateType::Or2,
        _ => return None,
    })
}

impl Pass for GateSimplify {
    fn name(&self) -> &'static str {
        "gate-simplify"
    }

    fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
        for mid in design.modules().ids() {
            let m = design.module_mut(mid);
            loop {
                let fold = m.cells().iter().find_map(|(inv, c)| {
                    if c.kind != CellKind::Gate(GateType::Inv) {
                        return None;
                    }
                    let mid_net = *m.input_nets(inv).first()?;
                    let out_net = *m.output_nets(inv).first()?;
                    let gate = m.driver(mid_net).and_then(|p| m.pin_cell(p))?;
                    let CellKind::Gate(g) = m.cells()[gate].kind else { return None };
                    let to = complement(g)?;
                    (m.fanout(mid_net) == 1).then_some((inv, gate, to, mid_net, out_net))
                });
                let Some((inv, gate, to, mid_net, out_net)) = fold else { break };
                m.set_gate_type(gate, to)?;
                m.remove_cell(inv);
                let y = m.cell_pin(gate, GATE_OUTPUT_PIN).expect("gates have y");
                m.disconnect(y);
                m.connect(y, out_net)?;
                m.remove_net(mid_net)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Direction;
    use crate::passes::cmos_map::tests::module;

    #[test]
    fn and_then_inv_becomes_nand() {
        let mut d = module(
            &[("a", Direction::Input), ("b", Direction::Input), ("y", Direction::Output)],
            &[("u0", GateType::And2, &["a", "b"], "n"), ("u1", GateType::Inv, &["n"], "y")],
            false,
        );
        GateSimplify.run(&mut d).unwrap();
        d.verify().unwrap();
        let m = d.module(d.top().unwrap());
        let cells: Vec<_> = m.cells().iter().map(|(_, c)| (c.name.as_str(), c.kind)).collect();
        assert_eq!(cells, vec![("u0", CellKind::Gate(GateType::Nand2))]);
        let y = m.port_net(m.port_by_name("y").unwrap()).unwrap();
        assert_eq!(m.nets()[y].name, "y");
        assert_eq!(m.driver(y), m.cell_pin(m.cells().ids()[0], "y"));
    }

    #[test]
    fn shared_gate_output_is_left_alone() {
        // n also feeds an output port, so folding the INV would lose it.
        let mut d = module(
            &[("a", Direction::Input), ("b", Direction::Input), ("n", Direction::Output), ("y", Direction::Output)],
            &[("u0", GateType::Or2, &["a", "b"], "n"), ("u1", GateType::Inv, &["n"], "y")],
            false,
        );
        GateSimplify.run(&mut d).unwrap();
        assert_eq!(d.module(d.top().unwrap()).cells().len(), 2);
    }
}
