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
    pub units: Units,
    pub reference_inverter: ReferenceInverter,
    pub sizing: SizingTargets,
    pub unbuffered: Metrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffered: Option<Metrics>,
    pub outputs: Vec<OutputReport>,
}

/// What each unit suffix in the report means.
#[derive(Debug, Serialize)]
pub struct Units {
    pub um: &'static str,
    #[serde(rename = "fF")]
    pub ff: &'static str,
    pub cinv: String,
    pub tau: &'static str,
    pub fo4: &'static str,
    pub effort: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReferenceInverter {
    pub wn_um: f64,
    pub wp_um: f64,
    pub ln_um: f64,
    pub lp_um: f64,
    /// Wp / Wn.
    pub gamma: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c_inv_ff: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SizingTargets {
    pub cload_cinv: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cload_ff: Option<f64>,
    pub cin_cinv: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cin_ff: Option<f64>,
    /// Target stage effort rho for buffering.
    pub stage_effort: f64,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub stage_effort: f64,
    pub cells: usize,
    pub transistors: usize,
    /// Worst output delay.
    pub delay_tau: f64,
    pub delay_fo4: f64,
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

/// Rounds for the report: 4 decimals is well below the model's accuracy.
fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
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
    let worst = output_delay.values().copied().fold(0.0, f64::max);
    Ok(Metrics {
        stage_effort: r4(f),
        cells: m.cells().len(),
        transistors,
        delay_tau: r4(worst),
        delay_fo4: r4(worst / 5.0),
        output_delay,
    })
}

impl Report {
    pub fn new(
        m: &Module,
        unit: &crate::backend::UnitInverter,
        sizing: &crate::config::Sizing,
        unbuffered: Metrics,
        plans: Vec<OutputPlan>,
    ) -> Self {
        let outputs = plans
            .into_iter()
            .map(|p| OutputReport {
                delay_tau: r4(unbuffered.output_delay.get(&p.port).copied().unwrap_or(0.0)),
                name: p.port,
                logic_stages: p.logic_stages,
                path_effort: r4(p.path_effort),
                added_inverters: p.added_inverters,
                buffered_delay_tau: None,
            })
            .collect();
        let ff = |cinv: f64| unit.c_inv_ff.map(|c| r4(cinv * c));
        Report {
            generator: crate::build_info::generator(),
            generated_at: crate::build_info::now_utc(),
            top: m.name.clone(),
            units: Units {
                um: "micrometre",
                ff: "femtofarad",
                cinv: match unit.c_inv_ff {
                    Some(c) => format!("input capacitance of the reference inverter, 1 C_inv = {c:.4} fF"),
                    None => "input capacitance of the reference inverter".into(),
                },
                tau: "logical effort delay unit (delay of an ideal inverter driving an identical one, no parasitics)",
                fo4: "delay of an inverter driving four identical ones, 1 FO4 = 5 tau",
                effort: "stage_effort, path_effort and gamma are dimensionless",
            },
            reference_inverter: ReferenceInverter {
                wn_um: r4(unit.wn),
                wp_um: r4(unit.wp),
                ln_um: r4(unit.ln),
                lp_um: r4(unit.lp),
                gamma: r4(unit.gamma()),
                c_inv_ff: unit.c_inv_ff.map(r4),
            },
            sizing: SizingTargets {
                cload_cinv: sizing.cload_cinv,
                cload_ff: ff(sizing.cload_cinv),
                cin_cinv: sizing.cin_cinv,
                cin_ff: ff(sizing.cin_cinv),
                stage_effort: sizing.stage_effort,
            },
            unbuffered,
            buffered: None,
            outputs,
        }
    }

    pub fn set_buffered(&mut self, buffered: Metrics) {
        for o in &mut self.outputs {
            o.buffered_delay_tau = buffered.output_delay.get(&o.name).copied().map(r4);
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
        let unit = crate::backend::UnitInverter { wn: 1.0, wp: 2.0, ln: 0.15, lp: 0.15, c_inv_ff: Some(1.5) };
        let sizing = crate::config::Sizing { cload_cinv: 64.0, cin_cinv: 1.0, stage_effort: 4.0 };
        let mut report = Report::new(d.module(top), &unit, &sizing, before, plans);
        BufferInsertion { mode: BufferMode::Invertible, rho: 4.0, params }.run(&mut d).unwrap();
        LogicalEffortSizing(params).run(&mut d).unwrap();
        report.set_buffered(metrics(d.module(top), 2.0).unwrap());

        // Three stages at f = 4: 3 * (4 + 1) = 15 tau.
        let out = &report.outputs[0];
        assert_eq!((out.logic_stages, out.added_inverters), (1, 2));
        assert!((out.buffered_delay_tau.unwrap() - 15.0).abs() < 1e-6);
        let text = report.to_toml().unwrap();
        assert!(text.contains("[buffered]") && text.contains("[[outputs]]"), "{text}");
        assert!(text.contains("cload_ff = 96.0") && text.contains("1 C_inv = 1.5000 fF"), "{text}");
    }
}
