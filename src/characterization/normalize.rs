use crate::characterization::device_params::{self, DeviceParams, ExtractError};
use crate::characterization::ngspice_ffi::{FfiError, NgspiceSession};
use crate::config::Config;
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
/// each into saturation on their own terms (independent gate/drain sources,
/// not the inverter's shared-gate connectivity), sized from
/// `config.reference_inverter`.
fn write_characterization_netlist(config: &Config, build_dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let vdd = config.environment.vdd;
    let vdd_half = vdd / 2.0;
    let ri = &config.reference_inverter;

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
        pmos_w = ri.pmos_w,
        pmos_l = ri.pmos_l,
        include_path = config.environment.include_path,
        corner = config.environment.corner,
    );

    fs::create_dir_all(build_dir)?;
    let path = build_dir.join("characterize.spice");
    fs::write(&path, netlist)?;
    Ok(path)
}

fn write_csv(
    build_dir: &Path,
    params_by_device: &HashMap<String, DeviceParams>,
) -> std::io::Result<()> {
    let path = build_dir.join("device_params.csv");
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

/// Runs the normalize stage: extracts BSIM device parameters via FFI to
/// libngspice for the nmos and pmos in `config.reference_inverter`'s
/// sizing, dumps them to `build_dir/device_params.csv` for visibility, and
/// returns them keyed by "nmos"/"pmos" for in-process use by later stages.
pub fn run(config: &Config, build_dir: &Path) -> Result<HashMap<String, DeviceParams>, NormalizeError> {
    let deck_path = write_characterization_netlist(config, build_dir)?;

    let session = NgspiceSession::start()?;
    session.command(&format!("source {}", deck_path.display()))?;

    let nmos_instance = format!("m.xnmos.m{}", config.models.nmos.to_lowercase());
    let pmos_instance = format!("m.xpmos.m{}", config.models.pmos.to_lowercase());

    let nmos_params = device_params::extract(&session, &nmos_instance)?;
    let pmos_params = device_params::extract(&session, &pmos_instance)?;

    let mut params_by_device = HashMap::new();
    params_by_device.insert("nmos".to_string(), nmos_params);
    params_by_device.insert("pmos".to_string(), pmos_params);

    write_csv(build_dir, &params_by_device)?;

    Ok(params_by_device)
}
