use crate::characterization::device_params::{self, DeviceParams, ExtractError};
use crate::characterization::ngspice_ffi::{FfiError, NgspiceSession};
use crate::characterization::paths::BuildLayout;
use crate::config::Config;
use log::{debug, warn};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormalizeError {
    #[error(transparent)]
    Ffi(#[from] FfiError),
    #[error(transparent)]
    Extract(#[from] ExtractError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Writes a throwaway characterization netlist biasing the nmos and pmos
/// each into saturation on their own terms (independent gate/drain
/// sources, not the inverter's shared-gate connectivity).
fn write_characterization_netlist(config: &Config, project_dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let vdd = config.environment.vdd;
    let vdd_half = vdd / 2.0;
    let ri = &config.reference_inverter;
    // Wp isn't known yet (the sizing sweep solves for it later); nmos_w is
    // a reasonable stand-in purely for biasing this throwaway deck.
    warn!(
        "characterization deck: assuming Wp = nmos_w ({:.4}um) for bias purposes only; the real Wp is solved for later",
        ri.nmos_w
    );
    let pmos_w = ri.nmos_w;
    if ri.pmos_l.is_none() {
        warn!(
            "reference_inverter.pmos_l not set; assuming Lp = nmos_l ({:.4}um) for the characterization deck",
            ri.nmos_l
        );
    }
    let pmos_l = ri.pmos_l_or_default();

    let netlist = format!(
        "* throwaway deck for device-parameter extraction; not for user inspection\n\
         Vdd VDD GND {vdd}\n\
         Vgn gate_n GND {vdd}\n\
         Vdn drain_n GND {vdd_half}\n\
         XNMOS drain_n gate_n GND GND {nmos_model} w={nmos_w} l={nmos_l}\n\
         \n\
         Vgp gate_p GND 0\n\
         Vdp drain_p GND {vdd_half}\n\
         XPMOS drain_p gate_p VDD VDD {pmos_model} w={pmos_w} l={pmos_l}\n\
         \n\
         .lib {include_path} {corner}\n\
         .GLOBAL GND\n\
         .end\n",
        vdd = vdd,
        vdd_half = vdd_half,
        nmos_model = config.models.nmos,
        nmos_w = ri.nmos_w,
        nmos_l = ri.nmos_l,
        pmos_model = config.models.pmos,
        pmos_w = pmos_w,
        pmos_l = pmos_l,
        include_path = config.environment.include_path,
        corner = config.environment.corner,
    );

    let path = project_dir.join("characterize.spice");
    fs::write(&path, netlist)?;
    Ok(path)
}

fn write_csv(
    project_dir: &Path,
    params_by_device: &HashMap<String, DeviceParams>,
) -> std::io::Result<()> {
    let path = project_dir.join("device_params.csv");
    let mut file = fs::File::create(&path)?;
    writeln!(file, "device,param,value")?;

    let mut devices: Vec<&String> = params_by_device.keys().collect();
    devices.sort();

    for device in devices {
        let params = &params_by_device[device];
        for (name, value) in params.as_named_fields() {
            writeln!(file, "{device},{name},{value}")?;
        }
    }
    Ok(())
}

/// Extracts BSIM device parameters for the nmos and pmos via FFI to
/// libngspice, writes them to `build_dir/.macro_gen_project/device_params.csv`,
/// and returns them keyed by `"nmos"`/`"pmos"`. Reuses the process-wide
/// `session` since `ngSpice_Init` should only be called once per process.
pub fn run(
    config: &Config,
    build_dir: &Path,
    session: &NgspiceSession,
) -> Result<HashMap<String, DeviceParams>, NormalizeError> {
    let layout = BuildLayout::new(build_dir)?;
    let deck_path = write_characterization_netlist(config, &layout.project_dir)?;

    session.command(&format!("source {}", deck_path.display()))?;

    let nmos_instance = format!("m.xnmos.m{}", config.models.nmos.to_lowercase());
    let pmos_instance = format!("m.xpmos.m{}", config.models.pmos.to_lowercase());

    let nmos_params = device_params::extract(&session, &nmos_instance)?;
    let pmos_params = device_params::extract(&session, &pmos_instance)?;

    debug!(
        "nmos: vth={:.4}V id={:.4e}A u0={:.4} gm={:.4e}",
        nmos_params.vth, nmos_params.id, nmos_params.u0, nmos_params.gm
    );
    debug!(
        "pmos: vth={:.4}V id={:.4e}A u0={:.4} gm={:.4e}",
        pmos_params.vth, pmos_params.id, pmos_params.u0, pmos_params.gm
    );

    let mut params_by_device = HashMap::new();
    params_by_device.insert("nmos".to_string(), nmos_params);
    params_by_device.insert("pmos".to_string(), pmos_params);

    write_csv(&layout.project_dir, &params_by_device)?;

    Ok(params_by_device)
}
