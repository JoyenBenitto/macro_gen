//! Buffers each output port up to the delay-optimal number of stages.
//!
//! For an output whose critical path has `N` stages at stage effort `f`
//! (from [`sizing::solve`]), the path effort is `F = f^N` and the optimal
//! stage count is `round(log_rho F)` (`rho` = 4 by default). The shortfall
//! is made up with inverters between the driving stage and the port; the
//! caller re-runs sizing afterwards.
//!
//! - [`BufferMode::Invertible`]: any number of inverters. An odd count
//!   inverts the output; the module is tagged `macro_gen.inverted.<port>`.
//! - [`BufferMode::NonInvertible`]: inverters come in pairs, so the logic
//!   function is kept. An odd shortfall becomes whichever neighbouring even
//!   count is faster.

use crate::cmos::CmosStage;
use crate::ir::{CellId, Design, Direction, IrError, Module};
use crate::passes::Pass;
use crate::passes::cmos_map::add_stage_cell;
use crate::passes::sizing::{self, SizingParams, driver_cell, topo_order};
use log::{info, warn};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BufferMode {
    /// Any number of inverters; the output may come out inverted.
    Invertible,
    /// Inverters in pairs (inv + inv), so the output keeps its polarity.
    NonInvertible,
}

pub struct BufferInsertion {
    pub mode: BufferMode,
    /// Target stage effort; the optimal stage count is `log_rho(F)`.
    pub rho: f64,
    pub params: SizingParams,
}

/// Parasitic delay of an added inverter, in units of `tau`.
const P_INV: f64 = 1.0;

/// How many inverters to add to an `n_logic`-stage path of effort `f_path`.
pub fn extra_stages(f_path: f64, n_logic: usize, rho: f64, mode: BufferMode) -> usize {
    let n_opt = (f_path.ln() / rho.ln()).round().max(1.0) as usize;
    let extra = n_opt.saturating_sub(n_logic);
    if mode == BufferMode::Invertible || extra % 2 == 0 {
        return extra;
    }
    let delay = |added: usize| {
        let n = (n_logic + added) as f64;
        n * f_path.powf(1.0 / n) + added as f64 * P_INV
    };
    if delay(extra - 1) <= delay(extra + 1) { extra - 1 } else { extra + 1 }
}

/// Stages on the longest input-to-cell path, counting the cell itself.
pub(crate) fn depths(m: &Module) -> Result<HashMap<CellId, usize>, IrError> {
    let mut depth = HashMap::new();
    for cell in topo_order(m)? {
        let d = m
            .input_nets(cell)
            .iter()
            .filter_map(|&n| driver_cell(m, n))
            .map(|c| depth[&c])
            .max()
            .unwrap_or(0);
        depth.insert(cell, d + 1);
    }
    Ok(depth)
}

/// What buffering does (or would do) to one output port.
#[derive(Debug, Clone)]
pub struct OutputPlan {
    pub port: String,
    /// Stages on the port's critical path before buffering.
    pub logic_stages: usize,
    /// `F = f^N` for that path.
    pub path_effort: f64,
    pub added_inverters: usize,
}

/// Plans buffering for every output port of a sized (`cmos-map`ped) module,
/// in port order. Ports driven straight by an input are skipped.
pub fn plan(m: &Module, params: &SizingParams, rho: f64, mode: BufferMode) -> Result<Vec<OutputPlan>, IrError> {
    let f = sizing::solve(m, params)?.stage_effort;
    let depth = depths(m)?;
    let mut plans = Vec::new();
    for (pid, port) in m.ports().iter().filter(|(_, p)| p.dir == Direction::Output) {
        let Some(net) = m.port_net(pid) else { continue };
        let Some(n_logic) = driver_cell(m, net).map(|c| depth[&c]) else { continue };
        let path_effort = f.powi(n_logic as i32);
        plans.push(OutputPlan {
            port: port.name.clone(),
            logic_stages: n_logic,
            path_effort,
            added_inverters: extra_stages(path_effort, n_logic, rho, mode),
        });
    }
    Ok(plans)
}

impl Pass for BufferInsertion {
    fn name(&self) -> &'static str {
        "buffer-insertion"
    }

    fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
        let top = design.top().ok_or_else(|| IrError::Unsupported("buffering needs a top module".into()))?;
        let m = design.module_mut(top);
        for p in plan(m, &self.params, self.rho, self.mode)? {
            let (port, extra) = (p.port, p.added_inverters);
            info!(
                "output '{port}': {} logic stage(s), path effort F = {:.2}, adding {extra} inverter(s)",
                p.logic_stages, p.path_effort
            );
            if extra == 0 {
                continue;
            }
            let pid = m.port_by_name(&port).expect("planned from this module's ports");
            let net = m.port_net(pid).expect("planned ports are connected");

            let port_pin = m.ports()[pid].pin;
            m.disconnect(port_pin);
            if m.nets()[net].name == port {
                m.rename_net(net, format!("{port}_pre"));
            }
            let inv = m.add_stage(CmosStage::inv());
            let mut cur = net;
            for i in 0..extra {
                let out = if i + 1 == extra { m.add_net(port.clone()) } else { m.add_net(format!("{port}_buf{i}_y")) };
                add_stage_cell(m, &format!("{port}_buf{i}"), inv, &[cur], out)?;
                cur = out;
            }
            m.connect(port_pin, cur)?;

            if extra % 2 == 1 {
                warn!("output '{port}' is now inverted ({extra} inverters added, --add-buffer invertible)");
                m.attrs.insert(format!("macro_gen.inverted.{port}"), "true".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::cmos_map::CmosMap;
    use crate::passes::cmos_map::tests::inverter_chain;

    #[test]
    fn stage_counts_follow_log4() {
        use BufferMode::*;
        assert_eq!(extra_stages(64.0, 1, 4.0, Invertible), 2);
        assert_eq!(extra_stages(256.0, 1, 4.0, Invertible), 3);
        // Odd shortfall of 3: +4 (5 stages at f ~ 3.0) beats +2 (3 stages at f ~ 6.3).
        assert_eq!(extra_stages(256.0, 1, 4.0, NonInvertible), 4);
        assert_eq!(extra_stages(64.0, 1, 4.0, NonInvertible), 2);
        // Already long enough.
        assert_eq!(extra_stages(4.0, 3, 4.0, Invertible), 0);
    }

    #[test]
    fn buffers_a_single_inverter_driving_64() {
        let mut d = inverter_chain(1);
        CmosMap.run(&mut d).unwrap();
        let params = SizingParams { gamma: 2.0, cload: 64.0, cin_max: 1.0 };
        BufferInsertion { mode: BufferMode::Invertible, rho: 4.0, params }.run(&mut d).unwrap();
        d.verify().unwrap();
        let m = d.module(d.top().unwrap());
        assert_eq!(m.cells().len(), 3);
        // Two inverters added: even, so y keeps its polarity.
        assert!(!m.attrs.contains_key("macro_gen.inverted.y"));
        let y = m.port_net(m.port_by_name("y").unwrap()).unwrap();
        assert_eq!(m.nets()[y].name, "y");
        assert!((sizing::solve(m, &params).unwrap().stage_effort - 4.0).abs() < 1e-6);
    }
}
