//! Sizes every gate in a [`Netlist`] -- via logical effort by default, or
//! via a [`CustomSizer`](crate::characterization::custom_sizing::CustomSizer)
//! for instances explicitly tagged in `config.custom_cells`.

pub mod fanout_tree;
pub mod gate_library;
pub mod logical_effort;

use crate::characterization::custom_sizing::{CustomSizer, InverterSizer};
use crate::characterization::device_params::SymbolTable;
use crate::characterization::ngspice_ffi::NgspiceSession;
use crate::characterization::paths::BuildLayout;
use crate::config::Config;
use crate::ir::{GateType, InstanceId, InstanceKind, Module, Netlist};
use gate_library::GateLibrary;
use logical_effort::{back_solve_capacitances, path_effort, stage_effort_best_number};
use std::collections::HashMap;
use thiserror::Error;

/// Default electrical effort assumed at a path's primary output when no
/// downstream load is known (a design boundary, not another gate) -- the
/// textbook "fanout-of-4" convention, a reasonable default for a stage
/// that will drive an unknown external load.
const DEFAULT_OUTPUT_ELECTRICAL_EFFORT: f64 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct GateSizing {
    pub wn: f64,
    pub ln: f64,
    pub wp: f64,
    pub lp: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SizedNetlist {
    pub netlist: Netlist,
    pub sizing: HashMap<InstanceId, GateSizing>,
}

#[derive(Debug, Error)]
pub enum SizingError {
    #[error("top module '{0}' was not found in the parsed netlist")]
    TopModuleNotFound(String),
    #[error("custom_cells names '{0}' but no matching hw.module/instance exists in the netlist")]
    CustomCellUnknown(String),
    #[error("custom cell '{0}' has {1} input(s)/{2} output(s); only 1-in/1-out (inverter-shaped) custom cells are supported")]
    CustomCellShapeUnsupported(String, usize, usize),
    #[error("gate '{0}' has no logical-effort model yet (supported: inv, nand2, nor2, and2, or2)")]
    UnknownGateType(String),
    #[error("reference inverter's Wp/Wn ratio is {0}, which is not a finite positive number; check that device parameters were extracted for both nmos and pmos")]
    BadBetaRatio(f64),
    #[error(transparent)]
    CustomSizerFailed(#[from] crate::characterization::custom_sizing::GenerateError),
}

/// Sizes every primitive gate instance in `config.circt.top_module`.
/// Instances named in `config.custom_cells.names` are routed to
/// [`InverterSizer`] instead of logical-effort sizing (v1: assumes any
/// custom-tagged cell is inverter-shaped -- see
/// `SizingError::CustomCellShapeUnsupported`).
pub fn size_netlist(
    netlist: &Netlist,
    config: &Config,
    build_dir: &std::path::Path,
    symbols: &SymbolTable,
    session: &NgspiceSession,
) -> Result<SizedNetlist, SizingError> {
    let circt_cfg = config
        .circt
        .as_ref()
        .expect("size_netlist is only called when config.circt is Some");
    let module = netlist
        .find_module(&circt_cfg.top_module)
        .ok_or_else(|| SizingError::TopModuleNotFound(circt_cfg.top_module.clone()))?;

    for name in &config.custom_cells.names {
        if netlist.find_module(name).is_none()
            && !module.instances.iter().any(|i| &i.name == name)
        {
            return Err(SizingError::CustomCellUnknown(name.clone()));
        }
    }

    // Logical effort is relative to the reference inverter, so derive every
    // gate's g/p from its measured Wp/Wn ratio.
    let beta_ratio = crate::characterization::inverter::id_based_w_l_n_p_ratio(symbols);
    if !beta_ratio.is_finite() || beta_ratio <= 0.0 {
        return Err(SizingError::BadBetaRatio(beta_ratio));
    }
    let lib = GateLibrary::from_beta_ratio(beta_ratio);

    // Fail up front (rather than silently sizing with g=1) if the design
    // uses a gate we have no model for.
    for inst in gate_instances_of(module) {
        if lib.get(inst).is_none() {
            return Err(SizingError::UnknownGateType(inst.library_name().to_string()));
        }
    }

    let layout = BuildLayout::new(build_dir).map_err(|e| {
        // BuildLayout::new only fails on filesystem errors; surfaced via
        // the custom-sizer error path since it shares GenerateError's Io
        // variant and this fn has no dedicated IO variant.
        crate::characterization::custom_sizing::GenerateError::Io(e)
    })?;

    let mut sizing = HashMap::new();

    for path in fanout_tree::enumerate_paths(module) {
        if path.stages.is_empty() {
            continue;
        }
        let f = path_effort(&path.stages, &lib, DEFAULT_OUTPUT_ELECTRICAL_EFFORT);
        let f_hat = stage_effort_best_number(f, path.stages.len());
        let caps = back_solve_capacitances(
            &path.stages,
            &lib,
            f_hat,
            DEFAULT_OUTPUT_ELECTRICAL_EFFORT,
        );

        for (stage_idx, &instance_id) in path.instances.iter().enumerate() {
            if sizing.contains_key(&instance_id) {
                continue; // already sized via another path reaching it first
            }
            let inst_name = module
                .instances
                .iter()
                .find(|i| i.id == instance_id)
                .map(|i| i.name.as_str())
                .unwrap_or("");

            if config.custom_cells.names.iter().any(|n| n == inst_name) {
                let gs = size_custom_cell(config, &layout, symbols, session, inst_name)?;
                sizing.insert(instance_id, gs);
                continue;
            }

            let gate = caps[stage_idx].gate;
            let entry = lib
                .get(gate)
                .ok_or_else(|| SizingError::UnknownGateType(gate.library_name().to_string()))?;
            let gs = width_from_capacitance(entry, beta_ratio, gate, caps[stage_idx].c_in, config);
            sizing.insert(instance_id, gs);
        }
    }

    Ok(SizedNetlist {
        netlist: Netlist { modules: vec![module.clone()] },
        sizing,
    })
}

fn size_custom_cell(
    config: &Config,
    layout: &BuildLayout,
    symbols: &SymbolTable,
    session: &NgspiceSession,
    _instance_name: &str,
) -> Result<GateSizing, SizingError> {
    // v1: any custom-tagged cell is assumed to be the configured reference
    // inverter (see SizingError::CustomCellShapeUnsupported at the call
    // site's TODO for generalizing beyond a single custom cell type).
    let sizer = InverterSizer;
    Ok(sizer.generate_deck(config, layout, symbols, session)?)
}

/// Converts a logical-effort-normalized input capacitance (multiples of a
/// unit inverter's input cap) into physical Wn/Wp, using the configured
/// `reference_inverter`'s sizing as the "unit" and `nmos_stack`/
/// `pmos_stack` to compensate series-transistor on-resistance (each
/// transistor in a stack of height `k` is widened by `k` so the stack's
/// effective drive strength matches an unstacked gate of the same nominal
/// size -- the standard logical-effort stacking convention).
fn width_from_capacitance(
    entry: &gate_library::GateParams,
    beta_ratio: f64,
    gate: GateType,
    c_in_normalized: f64,
    config: &Config,
) -> GateSizing {
    let ri = &config.reference_inverter;
    let wn_ref = ri.nmos_w;
    let wp_ref = wn_ref * beta_ratio;
    let l = ri.nmos_l;

    // Split the total width budget between NMOS and PMOS in the reference
    // inverter's Wp/Wn ratio.
    let w_total = c_in_normalized * (wn_ref + wp_ref);
    let wp_unit = w_total * beta_ratio / (1.0 + beta_ratio);
    let wn_unit = w_total - wp_unit;

    let min_width = config.environment.min_width;
    let wn = (wn_unit * entry.nmos_stack as f64).max(min_width);
    let wp = (wp_unit * entry.pmos_stack as f64).max(min_width);

    let _ = gate; // reserved for gate-specific topology adjustments later
    GateSizing { wn, ln: l, wp, lp: l }
}

/// The primitive gate types used by `module`'s (non-hierarchical) instances.
fn gate_instances_of(module: &Module) -> impl Iterator<Item = GateType> + '_ {
    module.instances.iter().filter_map(|i| match i.kind {
        InstanceKind::Gate(g) => Some(g),
        InstanceKind::ModuleInstance(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characterization::device_params::DeviceParams;
    use crate::config::{Config, CustomCells, Environment, Models, ReferenceInverter};
    use std::collections::HashMap as Map;

    fn test_config() -> Config {
        Config {
            environment: Environment {
                vdd: 1.8,
                min_length: 0.15,
                min_width: 0.42,
                include_path: "/dev/null".to_string(),
                corner: "tt".to_string(),
            },
            models: Models { nmos: "n".to_string(), pmos: "p".to_string() },
            reference_inverter: ReferenceInverter {
                name: "inv".to_string(),
                nmos_w: 1.0,
                nmos_l: 0.15,
                pmos_l: None,
                inverter_threshold: 0.9,
                wl_sweep_granularity: None,
                wl_sweep_sample_count: None,
                w_is_multiple_of_w_min: false,
                l_is_multiple_of_l_min: false,
            },
            circt: None,
            custom_cells: CustomCells::default(),
        }
    }

    fn test_symbols() -> SymbolTable {
        let mut m = Map::new();
        m.insert("nmos".to_string(), DeviceParams { id: 2.0, ..Default::default() });
        m.insert("pmos".to_string(), DeviceParams { id: -1.0, ..Default::default() });
        SymbolTable::from_device_params(&m)
    }

    #[test]
    fn width_from_capacitance_scales_with_stack_height() {
        let config = test_config();
        let symbols = test_symbols();
        let r = crate::characterization::inverter::id_based_w_l_n_p_ratio(&symbols);
        let lib = GateLibrary::from_beta_ratio(r);

        let inv = width_from_capacitance(lib.get(GateType::Inv).unwrap(), r, GateType::Inv, 1.0, &config);
        let nand = width_from_capacitance(lib.get(GateType::Nand2).unwrap(), r, GateType::Nand2, 1.0, &config);

        // Same normalized C_in, but NAND2's 2-high NMOS stack should widen
        // Wn relative to the unstacked inverter; PMOS is unchanged.
        assert!(nand.wn > inv.wn);
        assert!((nand.wp - inv.wp).abs() < 1e-12);
    }

    #[test]
    fn unit_inverter_reproduces_the_reference_inverter() {
        let config = test_config();
        let symbols = test_symbols();
        let r = crate::characterization::inverter::id_based_w_l_n_p_ratio(&symbols);
        let lib = GateLibrary::from_beta_ratio(r);

        let inv = width_from_capacitance(lib.get(GateType::Inv).unwrap(), r, GateType::Inv, 1.0, &config);
        assert!((inv.wn - config.reference_inverter.nmos_w).abs() < 1e-12);
        assert!((inv.wp - config.reference_inverter.nmos_w * r).abs() < 1e-12);
    }
}
