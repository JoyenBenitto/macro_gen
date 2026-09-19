use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub environment: Environment,
    pub models: Models,
    pub reference_inverter: ReferenceInverter,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub vdd: f64,
    pub min_length: f64,
    pub min_width: f64,
    pub include_path: String,
    pub corner: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Models {
    pub nmos: String,
    pub pmos: String,
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReferenceInverter {
    pub name: String,
    pub nmos_w: f64,
    pub nmos_l: f64,
    /// Defaults to `nmos_l` when omitted (Ln == Lp is the common case).
    #[serde(default)]
    pub pmos_l: Option<f64>,
    pub inverter_threshold: f64,
    /// Step size (in microns) between candidate Wp values during the
    /// sweep-to-target-Vth refinement. Defaults to `environment.min_width`
    /// when omitted. When `w_is_multiple_of_w_min` is set, this must be at
    /// least `environment.min_width` — a smaller step would just quantize
    /// every candidate back to the same grid point.
    #[serde(default)]
    pub wl_sweep_granularity: Option<f64>,
    /// Total number of candidate Wp values evaluated during the sweep
    /// (centered on the analytical seed). Defaults to 11 when omitted.
    #[serde(default)]
    pub wl_sweep_sample_count: Option<u32>,
    /// When true, Wn/Wp are snapped to the nearest integer multiple of
    /// `environment.min_width`.
    #[serde(default = "default_false")]
    pub w_is_multiple_of_w_min: bool,
    /// When true, Ln/Lp are snapped to the nearest integer multiple of
    /// `environment.min_length`.
    #[serde(default = "default_false")]
    pub l_is_multiple_of_l_min: bool,
}

impl ReferenceInverter {
    /// Ln == Lp is the common case, so Lp defaults to Ln when not given.
    pub fn pmos_l_or_default(&self) -> f64 {
        self.pmos_l.unwrap_or(self.nmos_l)
    }
}