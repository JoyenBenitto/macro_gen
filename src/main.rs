//! `macro_gen` is an automated digital IC macro generator.
//!
//! It drives ngspice in-process via FFI to characterize and size a reference
//! inverter against a PDK-supplied config, and (feature `circt`) imports CIRCT
//! `hw`+`comb` IR into a hypergraph netlist IR run through a pass pipeline.
//!
//! See the [usage guide](https://joyenbenitto.github.io/macro_gen/) for CLI usage;
//! this rustdoc reference covers the internal module structure.

mod characterization;
#[cfg(feature = "circt")]
mod circt_ffi;
mod config;
// The IR is an API for passes to build on; not every entry point has a
// caller yet, and without `circt` nothing drives it at all.
#[allow(dead_code, unused_imports)]
mod ir;
// Only driven by the CIRCT flow today.
#[cfg_attr(not(feature = "circt"), allow(dead_code))]
mod passes;
mod validator;

use characterization::device_params::SymbolTable;
use characterization::ngspice_ffi::NgspiceSession;
use characterization::{inverter, normalize};
use clap::Parser;
use config::Config;
use log::{error, info};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const ASCII_ART_LOGO: &str = r#"
 __   __  _______  _______  ______    _______    _______  _______  __    _
|  |_|  ||   _   ||       ||    _ |  |       |  |       ||       ||  |  | |
|       ||  |_|  ||       ||   | ||  |   _   |  |    ___||    ___||   |_| |
|       ||       ||       ||   |_||_ |  | |  |  |   | __ |   |___ |       |
|       ||       ||      _||    __  ||  |_|  |  |   ||  ||    ___||  _    |
| ||_|| ||   _   ||     |_ |   |  | ||       |  |   |_| ||   |___ | | |   |
|_|   |_||__| |__||_______||___|  |_||_______|  |_______||_______||_|  |__|
"#;

/// Automated digital IC macro generator
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long)]
    config: String,

    /// Directory for build output (final decks under `spice/`, intermediate
    /// experiment/report files under `.macro_gen_project/`)
    #[arg(short, long, default_value = "./build")]
    build_dir: PathBuf,
}

/// Peak resident set size (high-water mark), in kilobytes, read from
/// `/proc/self/status`. Linux-only; returns `None` elsewhere or if the
/// field can't be found/parsed.
fn peak_memory_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|rest| rest.trim().split_whitespace().next())
            .and_then(|kb| kb.parse().ok())
    })
}

fn print_run_summary(elapsed: std::time::Duration) {
    let elapsed_s = elapsed.as_secs_f64();
    let memory = peak_memory_kb()
        .map(|kb| format!("{:.2} MB", kb as f64 / 1024.0))
        .unwrap_or_else(|| "n/a".to_string());

    info!("==================== Run Summary ====================");
    info!("  Elapsed time      : {:.3}s", elapsed_s);
    info!("  Peak memory (RSS) : {}", memory);
    info!("=======================================================");
}

/// Parses `config.circt`'s MLIR input into the netlist IR, sets the top
/// module, and runs the pass pipeline over it. Needs no ngspice session or
/// PDK. `examples/130nm.toml` (no `[circt]` section) never reaches this.
#[cfg(feature = "circt")]
fn run_circt_flow(config: &Config) -> Result<(), ExitCode> {
    let circt_cfg = config.circt.as_ref().expect("checked by caller");

    let ctx = circt_ffi::CirctContext::new();
    let mlir_module = ctx
        .parse_file(std::path::Path::new(&circt_cfg.mlir_path))
        .map_err(|e| {
            error!("Failed to parse CIRCT input '{}': {}", circt_cfg.mlir_path, e);
            ExitCode::FAILURE
        })?;

    let mut design = ir::from_circt::build_design(&mlir_module).map_err(|e| {
        error!("Failed to build netlist from CIRCT IR: {}", e);
        ExitCode::FAILURE
    })?;
    let top = design.module_by_name(&circt_cfg.top_module).ok_or_else(|| {
        error!("Top module '{}' not found in '{}'", circt_cfg.top_module, circt_cfg.mlir_path);
        ExitCode::FAILURE
    })?;
    design.set_top(top);

    let mut pipeline = passes::default_pipeline();
    info!("Pass pipeline: {}", pipeline.pass_names().join(", "));
    pipeline.run(&mut design).map_err(|e| {
        error!("{}", e);
        ExitCode::FAILURE
    })?;

    println!("CIRCT netlist: {:#?}", design);
    Ok(())
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::new().filter_or("MACROGEN_LOG", "info"))
        .format(|buf, record| {
            // "YYYY-MM-DD HH:MM:SS" instead of ISO8601's "...THH:MM:SSZ",
            // and a fixed "macro_gen" target instead of the module path.
            let ts = buf.timestamp().to_string();
            let ts = ts.replacen('T', " ", 1);
            let ts = ts.trim_end_matches('Z');
            let level_style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "[{ts} {level_style}{}{level_style:#} macro_gen] {}",
                record.level(),
                record.args()
            )
        })
        .init();
    let start = Instant::now();
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

    #[cfg(not(feature = "circt"))]
    if config.circt.is_some() {
        error!(
            "Configuration '{}' has a [circt] section, but this build of \
             macro_gen was compiled without the 'circt' feature. Rebuild \
             with `--features circt` (and CIRCT_DIR set) to use it.",
            args.config
        );
        return ExitCode::FAILURE;
    }

    #[cfg(feature = "circt")]
    if config.circt.is_some() {
        if let Err(code) = run_circt_flow(&config) {
            return code;
        }
        print_run_summary(start.elapsed());
        return ExitCode::SUCCESS;
    }

    let build_dir = args.build_dir.as_path();

    let session = match NgspiceSession::start() {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to start ngspice session: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let device_params = match normalize::run(&config, build_dir, &session) {
        Ok(params) => params,
        Err(e) => {
            error!("Failed to extract device parameters: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let symbols = SymbolTable::from_device_params(&device_params);

    match inverter::generate_deck(&config, build_dir, &symbols, &session) {
        Ok((wn, wp)) => {
            info!("Reference inverter sized: Wn={:.4}um, Wp={:.4}um", wn, wp);
        }
        Err(e) => {
            error!("Failed to generate inverter SPICE deck: {}", e);
            return ExitCode::FAILURE;
        }
    }

    print_run_summary(start.elapsed());

    ExitCode::SUCCESS
}
