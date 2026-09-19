use crate::characterization::ngspice_ffi::{FfiError, NgspiceSession};
use std::collections::HashMap;
use thiserror::Error;

/// Vacuum permittivity, F/m.
const EPS0: f64 = 8.854e-12;

/// Instance operating-point outputs, queryable as vectors via
/// `@<instance>[<param>]` + `ngGet_Vec_Info`.
const VECTOR_PARAMS: &[&str] = &[
    "vth", "id", "gm", "cgg", "cgs", "cgd", "cdb", "csb", "vgsteff", "weff", "leff",
];

/// Model-level parameters only available as `showmod` text; no vector path
/// exists for these.
const SHOWMOD_PARAMS: &[&str] = &["u0", "vsat", "toxe", "epsrox"];

#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceParams {
    pub vth: f64,
    /// Drain current at the characterization bias.
    pub id: f64,
    /// Raw per-bin BSIM fit parameter, not the true effective mobility
    /// (BSIM applies further degradation terms internally). Prefer `id`
    /// or `gm` over `u0` for sizing calculations.
    pub u0: f64,
    pub vsat: f64,
    pub toxe: f64,
    pub epsrox: f64,
    /// Derived: epsrox * EPS0 / toxe.
    pub cox: f64,
    /// Derived: vsat / u0.
    pub ec: f64,
    pub gm: f64,
    pub cgg: f64,
    pub cgs: f64,
    pub cgd: f64,
    pub cdb: f64,
    pub csb: f64,
    pub vgsteff: f64,
    pub weff: f64,
    pub leff: f64,
}

impl DeviceParams {
    /// (name, value) pairs in a stable order, for CSV output.
    pub fn as_named_fields(&self) -> [(&'static str, f64); 17] {
        [
            ("vth", self.vth),
            ("id", self.id),
            ("u0", self.u0),
            ("vsat", self.vsat),
            ("toxe", self.toxe),
            ("epsrox", self.epsrox),
            ("cox", self.cox),
            ("ec", self.ec),
            ("gm", self.gm),
            ("cgg", self.cgg),
            ("cgs", self.cgs),
            ("cgd", self.cgd),
            ("cdb", self.cdb),
            ("csb", self.csb),
            ("vgsteff", self.vgsteff),
            ("weff", self.weff),
            ("leff", self.leff),
        ]
    }
}

/// Flat, string-keyed view over one or more devices' `DeviceParams`, for
/// looking up a value by symbolic name (e.g. `"nmos.vth"`) instead of
/// going through the typed struct field-by-field.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable(HashMap<String, f64>);

impl SymbolTable {
    /// Builds a symbol table keyed `"<device>.<param>"` (e.g. `"nmos.vth"`).
    pub fn from_device_params(params_by_device: &HashMap<String, DeviceParams>) -> Self {
        let mut symbols = HashMap::new();
        for (device, params) in params_by_device {
            for (name, value) in params.as_named_fields() {
                symbols.insert(format!("{device}.{name}"), value);
            }
        }
        SymbolTable(symbols)
    }

    /// Looks up a symbol such as `"nmos.vth"`.
    pub fn get(&self, symbol: &str) -> Option<f64> {
        self.0.get(symbol).copied()
    }

    /// All symbol names currently defined, sorted.
    pub fn symbols(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.0.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error(transparent)]
    Ffi(#[from] FfiError),
    #[error("showmod output for '{0}' did not contain the required parameter(s): {1:?}")]
    MissingShowmodParams(String, Vec<&'static str>),
}

/// Parses `showmod`'s whitespace-formatted `key value` lines into a
/// name -> value map. Strips the `stdout `/`stderr ` prefix ngspice's
/// shared-library callback adds to each captured line before matching.
pub fn parse_showmod_output(lines: &[String]) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    for line in lines {
        let stripped = line
            .strip_prefix("stdout ")
            .or_else(|| line.strip_prefix("stderr "))
            .unwrap_or(line);

        let tokens: Vec<&str> = stripped.split_whitespace().collect();
        if tokens.len() != 2 {
            continue;
        }
        if let Ok(value) = tokens[1].parse::<f64>() {
            values.insert(tokens[0].to_string(), value);
        }
    }
    values
}

/// Runs `op` on the current circuit and extracts `DeviceParams` for the
/// given device instance (e.g. `"m.xnmos.msky130_fd_pr__nfet_01v8"`).
pub fn extract(session: &NgspiceSession, instance_path: &str) -> Result<DeviceParams, ExtractError> {
    session.command("op")?;

    let mut params = DeviceParams::default();

    for &name in VECTOR_PARAMS {
        let vec_name = format!("{name}_val");
        session.command(&format!("let {vec_name} = @{instance_path}[{name}]"))?;
        let value = session.get_real(&vec_name)?;
        assign_vector_param(&mut params, name, value);
    }

    session.command(&format!("showmod {instance_path}"))?;
    let showmod_lines = session.drain_output();
    let showmod_values = parse_showmod_output(&showmod_lines);

    let missing: Vec<&'static str> = SHOWMOD_PARAMS
        .iter()
        .filter(|p| !showmod_values.contains_key(**p))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(ExtractError::MissingShowmodParams(
            instance_path.to_string(),
            missing,
        ));
    }

    params.u0 = showmod_values["u0"];
    params.vsat = showmod_values["vsat"];
    params.toxe = showmod_values["toxe"];
    params.epsrox = showmod_values["epsrox"];

    params.cox = params.epsrox * EPS0 / params.toxe;
    params.ec = params.vsat / params.u0;

    Ok(params)
}

fn assign_vector_param(params: &mut DeviceParams, name: &str, value: f64) {
    match name {
        "vth" => params.vth = value,
        "id" => params.id = value,
        "gm" => params.gm = value,
        "cgg" => params.cgg = value,
        "cgs" => params.cgs = value,
        "cgd" => params.cgd = value,
        "cdb" => params.cdb = value,
        "csb" => params.csb = value,
        "vgsteff" => params.vgsteff = value,
        "weff" => params.weff = value,
        "leff" => params.leff = value,
        _ => unreachable!("VECTOR_PARAMS and assign_vector_param must stay in sync"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_showmod_key_value_lines() {
        let lines: Vec<String> = [
            " BSIM4v5: Berkeley Short Channel IGFET Model-4",
            "     device m.xm1.msky130_fd_pr__",
            "      model   xm1:nshort_model.48",
            "       vsat                236293",
            "       toxe             4.148e-09",
            "     epsrox                   3.9",
            "         u0             0.0935552",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let values = parse_showmod_output(&lines);

        assert_eq!(values.get("vsat"), Some(&236293.0));
        assert_eq!(values.get("toxe"), Some(&4.148e-09));
        assert_eq!(values.get("epsrox"), Some(&3.9));
        assert_eq!(values.get("u0"), Some(&0.0935552));
        // Header lines with non-numeric second tokens must not be picked up.
        assert!(!values.contains_key("BSIM4v5:"));
    }

    #[test]
    fn symbol_table_flattens_device_params_by_dotted_key() {
        let mut params_by_device = HashMap::new();
        params_by_device.insert(
            "nmos".to_string(),
            DeviceParams {
                vth: 0.78,
                cox: 8.3e-3,
                ..Default::default()
            },
        );

        let symbols = SymbolTable::from_device_params(&params_by_device);

        assert_eq!(symbols.get("nmos.vth"), Some(0.78));
        assert_eq!(symbols.get("nmos.cox"), Some(8.3e-3));
        assert_eq!(symbols.get("pmos.vth"), None);
        assert_eq!(symbols.len(), DeviceParams::default().as_named_fields().len());
    }

    #[test]
    fn derives_cox_and_ec() {
        let mut params = DeviceParams {
            u0: 0.0935552,
            vsat: 236293.0,
            toxe: 4.148e-9,
            epsrox: 3.9,
            ..Default::default()
        };
        params.cox = params.epsrox * EPS0 / params.toxe;
        params.ec = params.vsat / params.u0;

        assert!((params.cox - 8.325e-3).abs() < 1e-5);
        assert!((params.ec - 2.5257e6).abs() < 1e3);
    }
}
