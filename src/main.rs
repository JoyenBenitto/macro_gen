mod characterization;
mod config;
mod validator;

use characterization::device_params::SymbolTable;
use characterization::{inverter, normalize};
use clap::Parser;
use config::Config;
use log::{error, info};
use std::fs;
use std::process::ExitCode;
use std::path::Path;

const ASCII_ART_LOGO: &str = r#"
 __   __  _______  _______  ______    _______    _______  _______  __    _
|  |_|  ||   _   ||       ||    _ |  |       |  |       ||       ||  |  | |
|       ||  |_|  ||       ||   | ||  |   _   |  |    ___||    ___||   |_| |
|       ||       ||       ||   |_||_ |  | |  |  |   | __ |   |___ |       |
|       ||       ||      _||    __  ||  |_|  |  |   ||  ||    ___||  _    |
| ||_|| ||   _   ||     |_ |   |  | ||       |  |   |_| ||   |___ | | |   |
|_|   |_||__| |__||_______||___|  |_||_______|  |_______||_______||_|  |__|
"#;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long)]
    config: String,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::new().filter_or("MACROGEN_LOG", "info")).init();
    let args = Args::parse();

    info!("{}", ASCII_ART_LOGO);
    info!("Configuration file: {}", args.config);

    let raw = match fs::read_to_string(&args.config) {
        Ok(s) => s,
        Err(e) => {
            error!("Could not read config file '{}': {}", args.config, e);
            return ExitCode::FAILURE;
        }
    };

    let config: Config = match toml::from_str(&raw) {
        Ok(c) => c,
        Err(e) => {
            error!("'{}' is not a valid config file:\n{}", args.config, e);
            return ExitCode::FAILURE;
        }
    };

    if let Err(errs) = validator::validate(&config) {
        error!("Configuration '{}' failed validation:", args.config);
        for e in &errs.0 {
            error!("{}", e);
        }
        return ExitCode::FAILURE;
    }

    info!("Configuration validated successfully.");

    let build_dir = Path::new("build");

    let device_params = match normalize::run(&config, build_dir) {
        Ok(params) => params,
        Err(e) => {
            error!("Failed to extract device parameters: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let symbols = SymbolTable::from_device_params(&device_params);
    info!(
        "Extracted {} device parameter symbol(s) (e.g. nmos.vth={:.4}V, pmos.cox={:.4e})",
        symbols.len(),
        symbols.get("nmos.vth").unwrap_or_default(),
        symbols.get("pmos.cox").unwrap_or_default(),
    );

    // starting the characterization process
    if let Err(e) = inverter::generate_deck(&config, build_dir) {
        error!("Failed to generate inverter SPICE deck: {}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
