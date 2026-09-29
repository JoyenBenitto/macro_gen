//! Output writers for a sized design (after `passes::backend_pipeline`):
//! a transistor-level SPICE netlist ([`spice`]) and a structural Verilog
//! netlist ([`verilog`]). Both name each distinct (stage, drive) pair the
//! same way, so the Verilog cells line up with the SPICE subckts for LVS.

pub mod report;
pub mod spice;
pub mod tables;
pub mod verilog;

use crate::cmos::CmosStage;
use crate::ir::{CellId, IrError, Module, NetId, PinOwner};
use crate::passes::sizing::ATTR_DRIVE;

/// The characterized reference inverter every stage is sized against.
#[derive(Debug, Clone, Copy)]
pub struct UnitInverter {
    /// Widths and lengths, in um.
    pub wn: f64,
    pub wp: f64,
    pub ln: f64,
    pub lp: f64,
    /// Input capacitance in fF (the physical value of 1 C_inv), when the
    /// characterization produced gate capacitances.
    pub c_inv_ff: Option<f64>,
}

impl UnitInverter {
    pub fn gamma(&self) -> f64 {
        self.wp / self.wn
    }
}

/// Physical width in microns of a transistor of relative width `rel` in a
/// cell of drive `drive` (`w_ref`: the reference inverter's Wn or Wp),
/// clamped to `min_width` and snapped when `w_is_multiple_of_w_min` is set.
/// The flag is true when the ideal width was below `min_width`.
pub(crate) fn physical_width(config: &crate::config::Config, rel: f64, drive: f64, w_ref: f64) -> (f64, bool) {
    let env = &config.environment;
    let w = rel * drive * w_ref;
    let snapped = crate::characterization::inverter::quantize(w, env.min_width, config.reference_inverter.w_is_multiple_of_w_min);
    (snapped.max(env.min_width), w < env.min_width)
}

/// A cell's stage and drive, the drive rounded to 0.01 so near-identical
/// cells share one master.
pub(crate) fn cell_master(m: &Module, cell: CellId) -> Result<(String, &CmosStage, f64), IrError> {
    let c = &m.cells()[cell];
    let stage = m.cell_stage(cell).ok_or_else(|| {
        IrError::Unsupported(format!("cell '{}' is not a CMOS stage; run cmos-map first", c.name))
    })?;
    let drive: f64 = c
        .attrs
        .get(ATTR_DRIVE)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| IrError::Unsupported(format!("cell '{}' has no {ATTR_DRIVE}; run sizing first", c.name)))?;
    let drive = ((drive * 100.0).round() / 100.0).max(0.01);
    let name = sanitize(&format!("{}_x{}", stage.name, format!("{drive:.2}").replace('.', "p")));
    Ok((name, stage, drive))
}

/// A plain identifier (`[A-Za-z_][A-Za-z0-9_]*`), legal in both SPICE and
/// Verilog without escaping.
pub(crate) fn sanitize(name: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "n_");
    }
    s
}

/// A net's name in the output: the port's name for a port net (so no
/// `assign` is ever needed), otherwise the net's own name. `NC` if the pin
/// is unconnected.
pub(crate) fn net_name(m: &Module, net: Option<NetId>) -> String {
    let Some(net) = net else { return "NC".into() };
    let port = m.nets()[net].pins.iter().find_map(|&p| match m.pins()[p].owner {
        PinOwner::Port(id) => Some(&m.ports()[id].name),
        PinOwner::Cell(_) => None,
    });
    sanitize(port.unwrap_or(&m.nets()[net].name))
}
