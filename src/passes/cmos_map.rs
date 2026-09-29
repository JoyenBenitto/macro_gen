//! Lowers the top module's gates to single-stage CMOS cells
//! ([`CellKind::Cmos`]), so every cell afterwards is one pull-up/pull-down
//! network that logical effort can size.
//!
//! - INV, NAND2, NOR2 map one-to-one.
//! - AND2 / OR2 are not a single CMOS stage: they become NAND2 / NOR2
//!   followed by an INV.
//! - A module tagged `macro_gen.cell = "complex"` is collapsed into one
//!   compound stage per output port: its gate cone is rewritten to negation
//!   normal form over the input literals. If every literal is
//!   uninverted (e.g. `(a & b) | c`), the stage's pull-down computes the
//!   complement (AND -> series, OR -> parallel) and an INV restores
//!   polarity: AOI21 + INV for that example. If every literal is inverted
//!   the function is a single stage with the dual pull-down.
//! - XOR2 / XNOR2 / MUX2 (not unate) and module instances are rejected.

use crate::cmos::{CmosStage, Network};
use crate::ir::{CellId, CellKind, Design, GateType, IrError, Module, NetId, PinOwner};
use crate::ir::cell::GATE_OUTPUT_PIN;
use crate::passes::Pass;

pub struct CmosMap;

impl Pass for CmosMap {
    fn name(&self) -> &'static str {
        "cmos-map"
    }

    fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
        let top = design
            .top()
            .ok_or_else(|| IrError::Unsupported("cmos-map needs a top module".into()))?;
        let m = design.module_mut(top);
        if let Some((_, c)) = m.cells().iter().find(|(_, c)| matches!(c.kind, CellKind::Instance(_))) {
            return Err(IrError::Unsupported(format!(
                "cell '{}' in '{}' is a module instance; hierarchy is not supported by cmos-map yet",
                c.name, m.name
            )));
        }
        if m.is_complex_cell() { lower_complex(m) } else { lower_gates(m) }
    }
}

fn lower_gates(m: &mut Module) -> Result<(), IrError> {
    for cell in m.cells().ids() {
        let CellKind::Gate(gate) = m.cells()[cell].kind else { continue };
        let name = m.cells()[cell].name.clone();
        let inputs = m.input_nets(cell);
        let output = m.output_nets(cell).first().copied();
        let (stage, restore) = match gate {
            GateType::Inv => (CmosStage::inv(), false),
            GateType::Nand2 => (CmosStage::nand(2), false),
            GateType::Nor2 => (CmosStage::nor(2), false),
            GateType::And2 => (CmosStage::nand(2), true),
            GateType::Or2 => (CmosStage::nor(2), true),
            GateType::Xor2 | GateType::Xnor2 | GateType::Mux2 => {
                return Err(IrError::Unsupported(format!(
                    "cell '{name}' is {}, which has no single-stage CMOS mapping yet",
                    gate.library_name()
                )));
            }
        };
        m.remove_cell(cell);
        let Some(output) = output else { continue };
        let stage = m.add_stage(stage);
        if restore {
            let mid = m.add_net(format!("{name}_n"));
            add_stage_cell(m, &name, stage, &inputs, mid)?;
            let inv = m.add_stage(CmosStage::inv());
            add_stage_cell(m, &format!("{name}_inv"), inv, &[mid], output)?;
        } else {
            add_stage_cell(m, &name, stage, &inputs, output)?;
        }
    }
    Ok(())
}

pub(crate) fn add_stage_cell(
    m: &mut Module,
    name: &str,
    stage: crate::ir::StageId,
    inputs: &[NetId],
    output: NetId,
) -> Result<CellId, IrError> {
    let cell = m.add_cmos_cell(name, stage);
    for (i, &net) in inputs.iter().enumerate() {
        let pin = m.cell_pin(cell, &GateType::input_pin_name(i)).expect("add_cmos_cell creates in<i>");
        m.connect(pin, net)?;
    }
    let y = m.cell_pin(cell, GATE_OUTPUT_PIN).expect("add_cmos_cell creates y");
    m.connect(y, output)?;
    Ok(cell)
}

/// A gate cone in negation normal form. `Lit(net, positive)` is a module
/// input (the net an input port drives).
#[derive(Debug)]
enum Expr {
    Lit(NetId, bool),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

impl Expr {
    fn literals(&self, out: &mut Vec<(NetId, bool)>) {
        match self {
            Expr::Lit(n, p) => out.push((*n, *p)),
            Expr::And(c) | Expr::Or(c) => c.iter().for_each(|e| e.literals(out)),
        }
    }

    /// Pull-down for `NOT(self)` over uninverted inputs (`dual = false`),
    /// or for `NOT(self)` with every literal inverted, which is the
    /// pull-down of the dual expression (`dual = true`).
    fn pdn(&self, dual: bool, index_of: &dyn Fn(NetId) -> usize) -> Network {
        match (self, dual) {
            (Expr::Lit(n, _), _) => Network::Fet(index_of(*n)),
            (Expr::And(c), false) | (Expr::Or(c), true) => {
                Network::Series(c.iter().map(|e| e.pdn(dual, index_of)).collect())
            }
            (Expr::Or(c), false) | (Expr::And(c), true) => {
                Network::Parallel(c.iter().map(|e| e.pdn(dual, index_of)).collect())
            }
        }
    }
}

/// The cone driving `net`, with `negate` pushed down to the literals.
fn build_expr(m: &Module, net: NetId, negate: bool) -> Result<Expr, IrError> {
    let driver = m.driver(net).ok_or_else(|| {
        IrError::Unsupported(format!("net '{}' in '{}' has no driver", m.nets()[net].name, m.name))
    })?;
    let cell = match m.pins()[driver].owner {
        PinOwner::Port(_) => return Ok(Expr::Lit(net, !negate)),
        PinOwner::Cell(c) => c,
    };
    let kind = m.cells()[cell].kind;
    let ins = m.input_nets(cell);
    let sub = |neg: bool| -> Result<Vec<Expr>, IrError> {
        ins.iter().map(|&n| build_expr(m, n, neg)).collect()
    };
    // (inverting gate?, AND-like?)
    let (inverting, and_like) = match kind {
        CellKind::Gate(GateType::Inv) => return build_expr(m, ins[0], !negate),
        CellKind::Gate(GateType::And2) => (false, true),
        CellKind::Gate(GateType::Nand2) => (true, true),
        CellKind::Gate(GateType::Or2) => (false, false),
        CellKind::Gate(GateType::Nor2) => (true, false),
        _ => {
            return Err(IrError::Unsupported(format!(
                "cell '{}' cannot be part of complex cell '{}': only INV/AND2/NAND2/OR2/NOR2 are supported",
                m.cells()[cell].name, m.name
            )));
        }
    };
    let neg = negate != inverting;
    // De Morgan: a negated AND is an OR of negated inputs, and vice versa.
    let children = sub(neg)?;
    Ok(if and_like != neg { Expr::And(children) } else { Expr::Or(children) })
}

fn lower_complex(m: &mut Module) -> Result<(), IrError> {
    // Input port nets in port order: the stage input numbering.
    let input_nets: Vec<NetId> = m
        .ports()
        .iter()
        .filter(|(_, p)| p.dir == crate::ir::Direction::Input)
        .filter_map(|(id, _)| m.port_net(id))
        .collect();

    let mut plans = Vec::new();
    for (pid, port) in m.ports().iter() {
        if port.dir != crate::ir::Direction::Output {
            continue;
        }
        let Some(net) = m.port_net(pid) else { continue };
        let expr = build_expr(m, net, false)?;
        let mut lits = Vec::new();
        expr.literals(&mut lits);
        let positive = if lits.iter().all(|&(_, p)| p) {
            true
        } else if lits.iter().all(|&(_, p)| !p) {
            false
        } else {
            return Err(IrError::Unsupported(format!(
                "output '{}' of complex cell '{}' needs both inverted and uninverted inputs",
                port.name, m.name
            )));
        };
        let used: Vec<NetId> = input_nets.iter().copied().filter(|n| lits.iter().any(|l| l.0 == *n)).collect();
        let index_of = |n: NetId| used.iter().position(|&u| u == n).expect("literal is a used input");
        let pdn = expr.pdn(!positive, &index_of);
        let stage = CmosStage::new(format!("{}_{}", m.name, port.name), used.len(), pdn);
        plans.push((port.name.clone(), net, used, stage, positive));
    }

    for cell in m.cells().ids() {
        m.remove_cell(cell);
    }
    for net in m.nets().ids() {
        if m.nets()[net].pins.is_empty() {
            m.remove_net(net)?;
        }
    }

    for (port, net, used, stage, positive) in plans {
        let stage = m.add_stage(stage);
        let stage_name = m.stages()[stage].name.clone();
        if positive {
            let mid = m.add_net(format!("{port}_b"));
            add_stage_cell(m, &stage_name, stage, &used, mid)?;
            let inv = m.add_stage(CmosStage::inv());
            add_stage_cell(m, &format!("{port}_inv"), inv, &[mid], net)?;
        } else {
            add_stage_cell(m, &stage_name, stage, &used, net)?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ir::Direction;

    /// Builds a top module with the given ports and `(name, gate, inputs, output)` gates.
    pub(crate) fn module(ports: &[(&str, Direction)], gates: &[(&str, GateType, &[&str], &str)], complex: bool) -> Design {
        let mut d = Design::new();
        let id = d.add_module("top").unwrap();
        d.set_top(id);
        let m = d.module_mut(id);
        if complex {
            m.attrs.insert("macro_gen.cell".into(), "complex".into());
        }
        let net = |m: &mut Module, name: &str| {
            let existing = m.nets().iter().find(|(_, n)| n.name == name).map(|(id, _)| id);
            existing.unwrap_or_else(|| m.add_net(name))
        };
        for &(name, dir) in ports {
            let p = m.add_port(name, dir);
            let n = net(m, name);
            m.connect(m.ports()[p].pin, n).unwrap();
        }
        for &(name, gate, ins, out) in gates {
            let c = m.add_gate(name, gate);
            for (i, n) in ins.iter().enumerate() {
                let n = net(m, n);
                m.connect(m.cell_pin(c, &GateType::input_pin_name(i)).unwrap(), n).unwrap();
            }
            let n = net(m, out);
            m.connect(m.cell_pin(c, "y").unwrap(), n).unwrap();
        }
        d
    }

    /// `a -> INV -> ... -> INV -> y`, `n` inverters.
    pub(crate) fn inverter_chain(n: usize) -> Design {
        let nets: Vec<String> = (0..=n)
            .map(|i| match i {
                0 => "a".into(),
                i if i == n => "y".into(),
                i => format!("n{i}"),
            })
            .collect();
        let names: Vec<String> = (0..n).map(|i| format!("u{i}")).collect();
        let ins: Vec<[&str; 1]> = (0..n).map(|i| [nets[i].as_str()]).collect();
        let gates: Vec<(&str, GateType, &[&str], &str)> =
            (0..n).map(|i| (names[i].as_str(), GateType::Inv, &ins[i][..], nets[i + 1].as_str())).collect();
        module(&[("a", Direction::Input), ("y", Direction::Output)], &gates, false)
    }

    fn stage_names(d: &Design) -> Vec<String> {
        let m = d.module(d.top().unwrap());
        m.cells().iter().map(|(id, _)| m.cell_stage(id).unwrap().name.clone()).collect()
    }

    const AND_OR: &[(&str, Direction)] =
        &[("a", Direction::Input), ("b", Direction::Input), ("c", Direction::Input), ("y", Direction::Output)];

    #[test]
    fn and_becomes_nand_plus_inv() {
        let mut d = module(AND_OR, &[("u0", GateType::And2, &["a", "b"], "ab"), ("u1", GateType::Nor2, &["ab", "c"], "y")], false);
        CmosMap.run(&mut d).unwrap();
        d.verify().unwrap();
        assert_eq!(stage_names(&d), vec!["nand2", "inv", "nor2"]);
    }

    #[test]
    fn complex_and_or_is_aoi21_plus_inv() {
        let mut d = module(AND_OR, &[("u0", GateType::And2, &["a", "b"], "ab"), ("u1", GateType::Or2, &["ab", "c"], "y")], true);
        CmosMap.run(&mut d).unwrap();
        d.verify().unwrap();
        let m = d.module(d.top().unwrap());
        assert_eq!(stage_names(&d), vec!["top_y", "inv"]);
        let aoi = m.cell_stage(m.cells().ids()[0]).unwrap();
        assert_eq!(
            aoi.pdn,
            Network::Parallel(vec![Network::Series(vec![Network::Fet(0), Network::Fet(1)]), Network::Fet(2)])
        );
        // The dead intermediate net is gone; y is still the port's net.
        assert!(m.nets().iter().all(|(_, n)| n.name != "ab"));
    }

    #[test]
    fn complex_nand_is_a_single_stage() {
        let mut d = module(AND_OR, &[("u0", GateType::And2, &["a", "b"], "ab"), ("u1", GateType::Nor2, &["ab", "c"], "y")], true);
        CmosMap.run(&mut d).unwrap();
        d.verify().unwrap();
        // NOR(a&b, c) = AOI21 directly: one stage, no inverter.
        assert_eq!(stage_names(&d), vec!["top_y"]);
    }

    #[test]
    fn xor_is_rejected() {
        let mut d = module(AND_OR, &[("u0", GateType::Xor2, &["a", "b"], "y")], false);
        assert!(matches!(CmosMap.run(&mut d), Err(IrError::Unsupported(_))));
    }
}
