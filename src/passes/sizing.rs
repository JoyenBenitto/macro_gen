//! Logical-effort sizing of the top module's CMOS stages.
//!
//! Units are the reference inverter's input capacitance `C_inv`. A stage
//! with drive `s` (1.0 = reference inverter strength) presents `g_i * s`
//! on input `i`, and drives `C_out` = the sum of its loads (fanout pins,
//! plus `cload` per output port). For one stage effort `f` shared by every
//! stage, a single sweep from the outputs back to the inputs fixes every
//! drive: `s = C_out / f`.
//!
//! The capacitance each primary input sees falls as `f` rises, so `f` is
//! bisected until the heaviest input presents exactly `cin_max`. On a
//! single path that is the textbook `f = F^(1/N)`; with fanout, branching
//! is accounted for by the sweep instead of by enumerating paths.
//!
//! Every cell gets `macro_gen.drive` and `macro_gen.cin.in<i>`; the module
//! gets `macro_gen.stage_effort`.

use crate::ir::{CellId, CellKind, Design, Direction, IrError, Module, PinId, PinOwner};
use crate::passes::Pass;
use std::collections::HashMap;

pub const ATTR_DRIVE: &str = "macro_gen.drive";
pub const ATTR_STAGE_EFFORT: &str = "macro_gen.stage_effort";

#[derive(Debug, Clone, Copy)]
pub struct SizingParams {
    /// `Wp / Wn` of the reference inverter.
    pub gamma: f64,
    /// Load on every output port, in `C_inv`.
    pub cload: f64,
    /// Largest capacitance any primary input may present, in `C_inv`.
    pub cin_max: f64,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub stage_effort: f64,
    pub drive: HashMap<CellId, f64>,
    pub pin_cap: HashMap<PinId, f64>,
}

pub struct LogicalEffortSizing(pub SizingParams);

impl Pass for LogicalEffortSizing {
    fn name(&self) -> &'static str {
        "logical-effort-sizing"
    }

    fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
        let top = design.top().ok_or_else(|| IrError::Unsupported("sizing needs a top module".into()))?;
        let sol = solve(design.module(top), &self.0)?;
        let m = design.module_mut(top);
        for (&cell, &s) in &sol.drive {
            m.set_cell_attr(cell, ATTR_DRIVE, format!("{s:.6}"));
            for &pin in &m.cells()[cell].pins.clone() {
                if let Some(c) = sol.pin_cap.get(&pin) {
                    let key = format!("macro_gen.cin.{}", m.pins()[pin].name);
                    m.set_cell_attr(cell, key, format!("{c:.6}"));
                }
            }
        }
        m.attrs.insert(ATTR_STAGE_EFFORT.into(), format!("{:.6}", sol.stage_effort));
        Ok(())
    }
}

/// Cells in topological order (drivers before loads). Every cell must be a
/// CMOS stage, i.e. `cmos-map` has run.
pub(crate) fn topo_order(m: &Module) -> Result<Vec<CellId>, IrError> {
    let mut indegree: HashMap<CellId, usize> = HashMap::new();
    for (id, cell) in m.cells().iter() {
        if !matches!(cell.kind, CellKind::Cmos(_)) {
            return Err(IrError::Unsupported(format!(
                "cell '{}' in '{}' is not a CMOS stage; run cmos-map before sizing",
                cell.name, m.name
            )));
        }
        let fanin = m.input_nets(id).iter().filter(|&&n| driver_cell(m, n).is_some()).count();
        indegree.insert(id, fanin);
    }
    let mut ready: Vec<CellId> = m.cells().ids().into_iter().filter(|c| indegree[c] == 0).collect();
    let mut order = Vec::new();
    while let Some(cell) = ready.pop() {
        order.push(cell);
        for net in m.output_nets(cell) {
            for load in m.loads(net) {
                if let Some(l) = m.pin_cell(load) {
                    let d = indegree.get_mut(&l).expect("every cell has an indegree");
                    *d -= 1;
                    if *d == 0 {
                        ready.push(l);
                    }
                }
            }
        }
    }
    if order.len() != m.cells().len() {
        return Err(IrError::Unsupported(format!("'{}' has a combinational loop", m.name)));
    }
    Ok(order)
}

/// The cell driving `net`, if a cell (not an input port) drives it.
pub(crate) fn driver_cell(m: &Module, net: crate::ir::NetId) -> Option<CellId> {
    m.driver(net).and_then(|p| m.pin_cell(p))
}

/// Capacitance hanging on `net`: its cell-input loads plus `cload` per
/// output port.
fn net_load(m: &Module, net: crate::ir::NetId, pin_cap: &HashMap<PinId, f64>, cload: f64) -> f64 {
    m.loads(net)
        .map(|p| match m.pins()[p].owner {
            PinOwner::Cell(_) => pin_cap.get(&p).copied().unwrap_or(0.0),
            PinOwner::Port(_) => cload,
        })
        .sum()
}

fn size_at(m: &Module, order: &[CellId], f: f64, p: &SizingParams) -> (HashMap<CellId, f64>, HashMap<PinId, f64>) {
    let mut drive = HashMap::new();
    let mut pin_cap = HashMap::new();
    for &cell in order.iter().rev() {
        let c_out: f64 = m.output_nets(cell).iter().map(|&n| net_load(m, n, &pin_cap, p.cload)).sum();
        let s = c_out / f;
        let stage = m.cell_stage(cell).expect("topo_order checked every cell is CMOS");
        for &pin in &m.cells()[cell].pins {
            if m.pins()[pin].dir == Direction::Input {
                let i = m.cells()[cell].pins.iter().position(|&q| q == pin).expect("pin is on its cell");
                pin_cap.insert(pin, stage.logical_effort(i, p.gamma) * s);
            }
        }
        drive.insert(cell, s);
    }
    (drive, pin_cap)
}

/// Heaviest capacitance presented at any input port.
fn max_input_cap(m: &Module, pin_cap: &HashMap<PinId, f64>, cload: f64) -> f64 {
    m.ports()
        .iter()
        .filter(|(_, port)| port.dir == Direction::Input)
        .filter_map(|(id, _)| m.port_net(id))
        .map(|n| net_load(m, n, pin_cap, cload))
        .fold(0.0, f64::max)
}

/// Sizes every stage of `m` for the stage effort that makes the heaviest
/// input present exactly `cin_max`.
pub fn solve(m: &Module, p: &SizingParams) -> Result<Solution, IrError> {
    let order = topo_order(m)?;
    if order.is_empty() {
        return Ok(Solution { stage_effort: 1.0, drive: HashMap::new(), pin_cap: HashMap::new() });
    }
    let cap_at = |f: f64| {
        let (_, pin_cap) = size_at(m, &order, f, p);
        max_input_cap(m, &pin_cap, p.cload)
    };
    let (mut lo, mut hi) = (1e-6_f64, 1e12_f64);
    if cap_at(hi) > p.cin_max {
        return Err(IrError::Unsupported(format!(
            "'{}': an input drives an output port directly, so it sees the full load {} C_inv > cin {} C_inv",
            m.name, p.cload, p.cin_max
        )));
    }
    // cap_at is decreasing in f: bisect in log space.
    for _ in 0..200 {
        let mid = (lo * hi).sqrt();
        if cap_at(mid) > p.cin_max { lo = mid } else { hi = mid }
        if hi / lo < 1.0 + 1e-12 {
            break;
        }
    }
    let (drive, pin_cap) = size_at(m, &order, hi, p);
    Ok(Solution { stage_effort: hi, drive, pin_cap })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::GateType;
    use crate::passes::cmos_map::CmosMap;
    use crate::passes::cmos_map::tests::{inverter_chain, module};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() / b.abs().max(1.0) < 1e-6
    }

    #[test]
    fn three_inverters_driving_64_have_stage_effort_4() {
        let mut d = inverter_chain(3);
        CmosMap.run(&mut d).unwrap();
        let p = SizingParams { gamma: 2.0, cload: 64.0, cin_max: 1.0 };
        let sol = solve(d.module(d.top().unwrap()), &p).unwrap();
        assert!(close(sol.stage_effort, 4.0), "f = {}", sol.stage_effort);
        let mut drives: Vec<f64> = sol.drive.values().copied().collect();
        drives.sort_by(f64::total_cmp);
        assert!(close(drives[0], 1.0) && close(drives[1], 4.0) && close(drives[2], 16.0), "{drives:?}");
    }

    #[test]
    fn nand_path_accounts_for_logical_effort() {
        // a -> NAND2(a, b) -> y, load 4/3 * 3 = 4: F = g*H = 4/3 * 4, one stage.
        let mut d = module(
            &[("a", Direction::Input), ("b", Direction::Input), ("y", Direction::Output)],
            &[("u0", GateType::Nand2, &["a", "b"], "y")],
            false,
        );
        CmosMap.run(&mut d).unwrap();
        LogicalEffortSizing(SizingParams { gamma: 2.0, cload: 4.0, cin_max: 1.0 }).run(&mut d).unwrap();
        let m = d.module(d.top().unwrap());
        assert!(close(m.attrs[ATTR_STAGE_EFFORT].parse().unwrap(), 16.0 / 3.0));
        let (_, cell) = m.cells().iter().next().unwrap();
        assert!(close(cell.attrs["macro_gen.cin.in0"].parse().unwrap(), 1.0));
    }
}
