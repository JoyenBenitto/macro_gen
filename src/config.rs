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

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReferenceInverter {
    pub name: String,
    pub nmos_w: f64,
    pub pmos_w: f64,
    pub nmos_l: f64,
    pub pmos_l: f64,
    pub inverter_threshold: f64,
}