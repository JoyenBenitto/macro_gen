use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub environment: Environment,
    pub models: Models,
    pub syntax: Syntax,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Environment {
    pub vdd: f64,
    pub min_length: f64,
    pub include_path: String,
    pub corner: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Models {
    pub nmos: String,
    pub pmos: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Syntax {
    pub device_template: String,
}
