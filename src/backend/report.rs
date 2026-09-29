//! Machine-readable summary of a sized design (`reports/<top>.toml`), for
//! the benchmark runner and anything else downstream.
//!
//! Delays are logical-effort estimates in units of `tau`: every stage runs
//! at the module's stage effort `f` (that is how sizing sets drives), so a
//! path of stages `i` takes `sum(f + p_i)`. An FO4 inverter is `5 tau`.

use crate::ir::{CellId, Direction, IrError, Module};
use crate::passes::buffer::OutputPlan;
use crate::passes::sizing::{ATTR_STAGE_EFFORT, driver_cell, topo_order};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct Report {
    /// `macro_gen <version> (<commit>)` that produced this report.
    pub generator: String,
    pub generated_at: String,
    pub top: String,
    pub gamma: f64,
    pub cload_cinv: f64,
    pub cin_cinv: f64,
    pub unbuffered: Metrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffered: Option<Metrics>,
    pub outputs: Vec<OutputReport>,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub stage_effort: f64,
    pub cells: usize,
    pub transistors: usize,
    /// Worst output delay, in `tau`.
    pub delay_tau: f64,
    #[serde(skip)]
    pub output_delay: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub struct OutputReport {
    pub name: String,
    pub logic_stages: usize,
    pub path_effort: f64,
    /// Inverters `--add-buffer` adds (or would add, in the chosen mode).
    pub added_inverters: usize,
    pub delay_tau: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffered_delay_tau: Option<f64>,
}

/// The stage effort sizing recorded on `m`.
pub(crate) fn stage_effort(m: &Module) -> Result<f64, IrError> {
    m.attrs
        .get(ATTR_STAGE_EFFORT)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| IrError::Unsupported(format!("'{}' has no {ATTR_STAGE_EFFORT}; run sizing first", m.name)))
}

/// Arrival time (in `tau`) at each cell's output: the latest input plus
/// the cell's own `f + p`.
pub(crate) fn arrivals(m: &Module, f: f64, gamma: f64) -> Result<HashMap<CellId, f64>, IrError> {
    let mut arrival: HashMap<CellId, f64> = HashMap::new();
    for cell in topo_order(m)? {
        let stage = m.cell_stage(cell).expect("topo_order checked every cell is CMOS");
        let latest_input = m
            .input_nets(cell)
            .iter()
            .filter_map(|&n| driver_cell(m, n))
            .map(|c| arrival[&c])
            .fold(0.0, f64::max);
        arrival.insert(cell, latest_input + f + stage.parasitic(gamma));
    }
    Ok(arrival)
}

/// Size, transistor count and estimated delays of a sized module.
pub fn metrics(m: &Module, gamma: f64) -> Result<Metrics, IrError> {
    let f = stage_effort(m)?;
    let arrival = arrivals(m, f, gamma)?;
    let transistors = m
        .cells()
        .iter()
        .filter_map(|(id, _)| m.cell_stage(id))
        .map(|s| s.nmos().len() + s.pmos().len())
        .sum();

    let output_delay: HashMap<String, f64> = m
        .ports()
        .iter()
        .filter(|(_, p)| p.dir == Direction::Output)
        .map(|(id, p)| {
            let d = m.port_net(id).and_then(|n| driver_cell(m, n)).map_or(0.0, |c| arrival[&c]);
            (p.name.clone(), d)
        })
        .collect();
    Ok(Metrics {
        stage_effort: f,
        cells: m.cells().len(),
        transistors,
        delay_tau: output_delay.values().copied().fold(0.0, f64::max),
        output_delay,
    })
}

impl Report {
    pub fn new(
        m: &Module,
        gamma: f64,
        cload_cinv: f64,
        cin_cinv: f64,
        unbuffered: Metrics,
        plans: Vec<OutputPlan>,
    ) -> Self {
        let outputs = plans
            .into_iter()
            .map(|p| OutputReport {
                delay_tau: unbuffered.output_delay.get(&p.port).copied().unwrap_or(0.0),
                name: p.port,
                logic_stages: p.logic_stages,
                path_effort: p.path_effort,
                added_inverters: p.added_inverters,
                buffered_delay_tau: None,
            })
            .collect();
        Report {
            generator: crate::build_info::generator(),
            generated_at: crate::build_info::now_utc(),
            top: m.name.clone(),
            gamma,
            cload_cinv,
            cin_cinv,
            unbuffered,
            buffered: None,
            outputs,
        }
    }

    pub fn set_buffered(&mut self, buffered: Metrics) {
        for o in &mut self.outputs {
            o.buffered_delay_tau = buffered.output_delay.get(&o.name).copied();
        }
        self.buffered = Some(buffered);
    }

    pub fn to_toml(&self) -> Result<String, IrError> {
        toml::to_string(self).map_err(|e| IrError::Unsupported(format!("report serialization: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::Pass;
    use crate::passes::buffer::{BufferInsertion, BufferMode, plan};
    use crate::passes::cmos_map::CmosMap;
    use crate::passes::cmos_map::tests::inverter_chain;
    use crate::passes::sizing::{LogicalEffortSizing, SizingParams};

    #[test]
    fn buffering_one_inverter_into_64_cuts_delay() {
        let params = SizingParams { gamma: 2.0, cload: 64.0, cin_max: 1.0 };
        let mut d = inverter_chain(1);
        CmosMap.run(&mut d).unwrap();
        LogicalEffortSizing(params).run(&mut d).unwrap();
        let top = d.top().unwrap();
        let before = metrics(d.module(top), 2.0).unwrap();
        // One stage at f = 64: 64 + 1 tau.
        assert!((before.delay_tau - 65.0).abs() < 1e-6);
        assert_eq!(before.transistors, 2);

        let plans = plan(d.module(top), &params, 4.0, BufferMode::Invertible).unwrap();
        let mut report = Report::new(d.module(top), 2.0, 64.0, 1.0, before, plans);
        BufferInsertion { mode: BufferMode::Invertible, rho: 4.0, params }.run(&mut d).unwrap();
        LogicalEffortSizing(params).run(&mut d).unwrap();
        report.set_buffered(metrics(d.module(top), 2.0).unwrap());

        // Three stages at f = 4: 3 * (4 + 1) = 15 tau.
        let out = &report.outputs[0];
        assert_eq!((out.logic_stages, out.added_inverters), (1, 2));
        assert!((out.buffered_delay_tau.unwrap() - 15.0).abs() < 1e-6);
        let text = report.to_toml().unwrap();
        assert!(text.contains("[buffered]") && text.contains("[[outputs]]"), "{text}");
    }
}
