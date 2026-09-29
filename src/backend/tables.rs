//! Human-readable sizing tables, logged at `info`: the assumptions a run
//! sizes against, every cell's logical-effort numbers and physical widths,
//! each output's critical path (G, B, H, F, f, N), and a before/after
//! summary. They show the calculations behind the SPICE and Verilog, so a
//! run can be checked by hand.

use crate::backend::report::{Metrics, Report, arrivals, stage_effort};
use crate::backend::{UnitInverter, cell_master, net_name, physical_width};
use crate::config::Config;
use crate::ir::{CellId, Direction, IrError, Module};
use crate::passes::buffer::{BufferMode, OutputPlan};
use crate::passes::sizing::{driver_cell, topo_order};
use log::info;

/// Logs `rows` under `title` as an aligned table.
pub fn log_table(title: &str, header: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let line = |cells: &mut dyn Iterator<Item = &str>| -> String {
        cells
            .zip(&widths)
            .map(|(c, &w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join(" | ")
            .trim_end()
            .to_string()
    };
    info!("");
    info!("{title}");
    info!("  {}", line(&mut header.iter().copied()));
    info!("  {}", widths.iter().map(|&w| "-".repeat(w)).collect::<Vec<_>>().join("-+-"));
    for row in rows {
        info!("  {}", line(&mut row.iter().map(String::as_str)));
    }
}

/// Everything the sizing takes as given.
pub fn log_assumptions(config: &Config, unit: &UnitInverter, buffer: Option<BufferMode>) {
    let env = &config.environment;
    let sizing = config.sizing.as_ref().expect("validator requires [sizing] with [circt]");
    let snap = if config.reference_inverter.w_is_multiple_of_w_min { ", snapped to multiples" } else { "" };
    let ff = |cinv: f64| unit.c_inv_ff.map_or(String::new(), |c| format!(" ({:.2} fF)", cinv * c));
    let c_inv = match unit.c_inv_ff {
        Some(c) => format!("C_inv = {c:.3} fF, the reference inverter's input capacitance (gate cap from ngspice)"),
        None => "C_inv = the reference inverter's input capacitance (no gate cap extracted)".into(),
    };
    let rows = vec![
        vec!["Process".into(), format!("{} / {} @ {} V, corner {}", config.models.nmos, config.models.pmos, env.vdd, env.corner)],
        vec![
            "Reference inverter".into(),
            format!("Wn = {:.4} um, Wp = {:.4} um, Ln = {:.4} um, Lp = {:.4} um", unit.wn, unit.wp, unit.ln, unit.lp),
        ],
        vec!["gamma = Wp / Wn".into(), format!("{:.4} (dimensionless)", unit.gamma())],
        vec!["Unit capacitance".into(), c_inv],
        vec!["Unit drive".into(), "drive s = 1 drives like the reference inverter; widths = relative width x s x Wn (or Wp)".into()],
        vec!["Output load".into(), format!("{} C_inv{} on each module output port; internal nets see only their fanout", sizing.cload_cinv, ff(sizing.cload_cinv))],
        vec!["Input limit".into(), format!("heaviest primary input presents {} C_inv{}", sizing.cin_cinv, ff(sizing.cin_cinv))],
        vec!["Target stage effort".into(), format!("rho = {} (optimal stages N = round(log_rho F))", sizing.stage_effort)],
        vec!["Buffering".into(), buffer.map_or_else(|| "off".into(), |m| format!("{m:?}"))],
        vec!["Min width".into(), format!("{} um (narrower transistors are clamped up, marked *){snap}", env.min_width)],
        vec!["Delay model".into(), "d = f + p per stage, in tau (1 FO4 = 5 tau); efforts g, f, F are dimensionless".into()],
    ];
    log_table("Sizing assumptions", &["parameter", "value"], &rows);
}

/// Groups equal widths: `0.84x2, 0.42`. A trailing `*` marks clamped ones.
fn widths(list: &[(f64, bool)]) -> String {
    let mut groups: Vec<(String, usize)> = Vec::new();
    for &(w, clamped) in list {
        let label = format!("{w:.2}{}", if clamped { "*" } else { "" });
        match groups.last_mut() {
            Some((l, n)) if *l == label => *n += 1,
            _ => groups.push((label, 1)),
        }
    }
    groups
        .into_iter()
        .map(|(l, n)| if n > 1 { format!("{l}x{n}") } else { l })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One row per cell, in signal order: logical effort, capacitances, drive
/// and the resulting transistor widths.
pub fn log_cells(title: &str, m: &Module, config: &Config, unit: &UnitInverter) -> Result<(), IrError> {
    let f = stage_effort(m)?;
    let gamma = unit.gamma();
    let mut rows = Vec::new();
    for cell in topo_order(m)? {
        let (master, stage, drive) = cell_master(m, cell)?;
        let c = &m.cells()[cell];
        let exact: f64 = c.attrs.get(crate::passes::sizing::ATTR_DRIVE).and_then(|s| s.parse().ok()).unwrap_or(drive);
        let ins = 0..stage.n_inputs;
        let g: Vec<String> = ins.clone().map(|i| format!("{:.3}", stage.logical_effort(i, gamma))).collect();
        let cin: Vec<String> = ins
            .map(|i| c.attrs.get(&format!("macro_gen.cin.in{i}")).and_then(|s| s.parse::<f64>().ok()))
            .map(|v| v.map_or("-".into(), |v| format!("{v:.3}")))
            .collect();
        let wn: Vec<(f64, bool)> = stage.nmos().iter().map(|t| physical_width(config, t.width, drive, unit.wn)).collect();
        let wp: Vec<(f64, bool)> = stage.pmos().iter().map(|t| physical_width(config, t.width, drive, unit.wp)).collect();
        rows.push(vec![
            c.name.clone(),
            master,
            g.join(" / "),
            format!("{:.3}", stage.parasitic(gamma)),
            cin.join(" / "),
            format!("{:.3}", exact * f),
            unit.c_inv_ff.map_or("-".into(), |c| format!("{:.2}", exact * f * c)),
            format!("{exact:.3}"),
            widths(&wn),
            widths(&wp),
        ]);
    }
    log_table(
        &format!("{title}: every stage at f = C_out / s = {f:.3}, C_in(pin) = g x s"),
        &["cell", "master", "g (per input)", "p (tau)", "C_in (C_inv)", "C_out (C_inv)", "C_out (fF)", "drive s (x inv)", "NMOS W (um)", "PMOS W (um)"],
        &rows,
    );
    Ok(())
}

/// The critical path into each output, traced back through the latest
/// arriving input: `(cell, input pin index)` from the output backwards.
fn critical_path(m: &Module, arrival: &std::collections::HashMap<CellId, f64>, out: CellId) -> Vec<(CellId, usize)> {
    let mut path = Vec::new();
    let mut cell = out;
    loop {
        let ins = m.input_nets(cell);
        let from_cell = ins
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| driver_cell(m, n).map(|d| (i, d)))
            .max_by(|a, b| arrival[&a.1].total_cmp(&arrival[&b.1]));
        match from_cell {
            Some((i, prev)) => {
                path.push((cell, i));
                cell = prev;
            }
            None => {
                // First stage: the path enters on its heaviest input pin.
                let heaviest = (0..ins.len())
                    .max_by(|&a, &b| {
                        let cap = |i: usize| {
                            m.cells()[cell].attrs.get(&format!("macro_gen.cin.in{i}")).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
                        };
                        cap(a).total_cmp(&cap(b))
                    })
                    .unwrap_or(0);
                path.push((cell, heaviest));
                break;
            }
        }
    }
    path.reverse();
    path
}

/// One row per output: its critical path and the logical-effort terms
/// `F = G x B x H`, `f = F^(1/N)`, `N_opt = log_rho F`.
pub fn log_paths(
    title: &str,
    m: &Module,
    config: &Config,
    gamma: f64,
    plans: Option<&[OutputPlan]>,
) -> Result<(), IrError> {
    let sizing = config.sizing.as_ref().expect("validator requires [sizing] with [circt]");
    let f = stage_effort(m)?;
    let arrival = arrivals(m, f, gamma)?;
    let mut rows = Vec::new();
    for (pid, port) in m.ports().iter().filter(|(_, p)| p.dir == Direction::Output) {
        let Some(out) = m.port_net(pid).and_then(|n| driver_cell(m, n)) else { continue };
        let path = critical_path(m, &arrival, out);
        let (first, pin) = path[0];
        let first_pin = m.cells()[first].pins[pin];
        let mut names = vec![net_name(m, m.pins()[first_pin].net)];
        let mut g_path = 1.0;
        let mut p_path = 0.0;
        for &(cell, pin) in &path {
            let stage = m.cell_stage(cell).expect("sized cells are CMOS");
            g_path *= stage.logical_effort(pin, gamma);
            p_path += stage.parasitic(gamma);
            names.push(m.cells()[cell].name.clone());
        }
        names.push(port.name.clone());
        let n = path.len();
        let c_in: f64 = m.cells()[first].attrs.get(&format!("macro_gen.cin.in{pin}")).and_then(|s| s.parse().ok()).unwrap_or(f64::NAN);
        let h = sizing.cload_cinv / c_in;
        let big_f = f.powi(n as i32);
        let b = big_f / (g_path * h);
        let n_opt = big_f.ln() / sizing.stage_effort.ln();
        let added = plans
            .and_then(|ps| ps.iter().find(|p| p.port == port.name))
            .map_or("-".to_string(), |p| p.added_inverters.to_string());
        rows.push(vec![
            port.name.clone(),
            names.join(" > "),
            n.to_string(),
            format!("{g_path:.3}"),
            format!("{b:.3}"),
            format!("{h:.2}"),
            format!("{big_f:.4}"),
            format!("{f:.3}"),
            format!("{n_opt:.2}"),
            added,
            format!("{p_path:.2}"),
            format!("{:.2}", arrival[&out]),
        ]);
    }
    log_table(
        &format!("{title}: F = G x B x H, f = F^(1/N), N_opt = log_{} F, delay = N f + P", sizing.stage_effort),
        &["output", "critical path", "N", "G", "B", "H", "F", "f", "N_opt", "added inv", "P (tau)", "delay (tau)"],
        &rows,
    );
    Ok(())
}

/// Before/after totals.
pub fn log_summary(report: &Report) {
    let columns = |m: &Metrics| {
        [
            format!("{:.3}", m.stage_effort),
            m.cells.to_string(),
            m.transistors.to_string(),
            format!("{:.2}", m.delay_tau),
            format!("{:.2}", m.delay_tau / 5.0),
        ]
    };
    let before = columns(&report.unbuffered);
    let after = report.buffered.as_ref().map(columns);
    let names = ["stage effort f", "cells", "transistors", "worst delay (tau)", "worst delay (FO4)"];
    let rows: Vec<Vec<String>> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            vec![name.to_string(), before[i].clone(), after.as_ref().map_or("-".into(), |a| a[i].clone())]
        })
        .collect();
    log_table(&format!("Summary: {}", report.top), &["metric", "unbuffered", "buffered"], &rows);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_group_repeats_and_mark_clamps() {
        assert_eq!(widths(&[(0.84, false), (0.84, false), (0.42, true)]), "0.84x2, 0.42*");
    }
}
