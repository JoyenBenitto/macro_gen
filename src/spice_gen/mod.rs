//! Emits a multi-gate SPICE deck from a [`SizedNetlist`], generalizing the
//! header/footer conventions already used by `inv.spice.j2` (`.lib`,
//! `Vdd`, `.GLOBAL GND`) to an arbitrary set of primitive-gate instances.
//!
//! v1 scope: primitive gates only (`InstanceKind::Gate`), matching what
//! `ir::from_circt` currently produces (`inv`/`nand2`/`nor2`/`and2`/`or2`
//! have templates; `xor2`/`xnor2`/`mux2` are declared in the gate library
//! but not yet given SPICE templates -- see `SpiceGenError::NoTemplate`).
//! `InstanceKind::ModuleInstance` (hierarchy / `.subckt` emission) is left
//! for a follow-up once a single module's gates round-trip end to end.

use crate::characterization::paths::BuildLayout;
use crate::config::Config;
use crate::ir::{GateType, InstanceKind};
use crate::sizing::SizedNetlist;
use minijinja::{context, Environment};
use std::path::PathBuf;
use thiserror::Error;

macro_rules! gate_template {
    ($name:literal) => {
        include_str!(concat!("../characterization/constants/gates/", $name, ".spice.j2"))
    };
}

const INV_TEMPLATE: &str = gate_template!("inv");
const NAND2_TEMPLATE: &str = gate_template!("nand2");
const NOR2_TEMPLATE: &str = gate_template!("nor2");
const AND2_TEMPLATE: &str = gate_template!("and2");
const OR2_TEMPLATE: &str = gate_template!("or2");

#[derive(Debug, Error)]
pub enum SpiceGenError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Render(#[from] minijinja::Error),
    #[error("no SPICE template exists yet for gate type '{0}'")]
    NoTemplate(&'static str),
    #[error("instance '{0}' has no sizing recorded; size_netlist should have sized every gate instance")]
    MissingSizing(String),
    #[error("hierarchy (module instances) is not yet supported by spice_gen; found instance of '{0}'")]
    HierarchyUnsupported(String),
}

fn template_for(gate: GateType) -> Result<&'static str, SpiceGenError> {
    match gate {
        GateType::Inv => Ok(INV_TEMPLATE),
        GateType::Nand2 => Ok(NAND2_TEMPLATE),
        GateType::Nor2 => Ok(NOR2_TEMPLATE),
        GateType::And2 => Ok(AND2_TEMPLATE),
        GateType::Or2 => Ok(OR2_TEMPLATE),
        GateType::Xor2 | GateType::Xnor2 | GateType::Mux2 => {
            Err(SpiceGenError::NoTemplate(gate.library_name()))
        }
    }
}

/// Renders every primitive-gate instance in `sized.netlist`'s (single, for
/// v1) module into one flat deck and writes it to
/// `<build_dir>/spice/<module_name>.spice`.
pub fn emit_netlist(
    sized: &SizedNetlist,
    config: &Config,
    layout: &BuildLayout,
) -> Result<PathBuf, SpiceGenError> {
    let module = sized
        .netlist
        .modules
        .first()
        .expect("size_netlist always returns a netlist with exactly the top module");

    let mut body = String::new();
    for inst in &module.instances {
        let gate = match &inst.kind {
            InstanceKind::Gate(g) => *g,
            InstanceKind::ModuleInstance(name) => {
                return Err(SpiceGenError::HierarchyUnsupported(name.clone()));
            }
        };
        let sizing = sized
            .sizing
            .get(&inst.id)
            .ok_or_else(|| SpiceGenError::MissingSizing(inst.name.clone()))?;

        let in_nets: Vec<String> = inst
            .inputs
            .iter()
            .map(|id| module.nets[id.0 as usize].name.clone())
            .collect();
        let out_net = module
            .nets
            .get(inst.outputs.first().map(|id| id.0 as usize).unwrap_or(usize::MAX))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("{}_y", inst.name));

        let mut env = Environment::new();
        let tmpl_src = template_for(gate)?;
        env.add_template("gate", tmpl_src)?;
        let tmpl = env.get_template("gate")?;
        let rendered = tmpl.render(context! {
            instance_name => inst.name,
            in_nets => in_nets,
            out_net => out_net,
            wn => sizing.wn,
            ln => sizing.ln,
            wp => sizing.wp,
            lp => sizing.lp,
            model_nmos => config.models.nmos,
            model_pmos => config.models.pmos,
        })?;
        body.push_str(&rendered);
        body.push('\n');
    }

    let deck = format!(
        "{header}\n{body}\n.lib {include_path} {corner}\n.GLOBAL GND\n\n.end\n",
        header = format!(
            "Vdd VDD GND {vdd}\n",
            vdd = config.environment.vdd
        ),
        body = body,
        include_path = config.environment.include_path,
        corner = config.environment.corner,
    );

    let out_path = layout.spice_dir.join(format!("{}.spice", module.name));
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out_path, deck)?;
    Ok(out_path)
}
