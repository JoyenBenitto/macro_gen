//! `macro_gen` is an automated digital IC macro generator.
//!
//! It drives ngspice in-process via FFI to characterize and size a reference
//! inverter against a PDK-supplied config. With feature `circt` it also imports
//! CIRCT `hw`+`comb` IR into a hypergraph netlist IR, runs it through a pass
//! pipeline, and uses the CMOS backend to lower it to pull-up/pull-down
//! stages ([`cmos`]). The backend sizes those stages by logical effort
//! against that inverter, optionally buffers the outputs, and writes SPICE and
//! structural Verilog ([`backend`]).
//!
//! See the [usage guide](https://joyenbenitto.github.io/macro_gen/) for CLI usage;
//! this rustdoc reference covers the internal module structure.

// The CMOS backend (transistor networks, SPICE/Verilog writers) is only
// driven by the CIRCT flow.
#[cfg_attr(not(feature = "circt"), allow(dead_code))]
mod backend;
mod characterization;
#[cfg(feature = "circt")]
mod circt_ffi;
#[cfg_attr(not(feature = "circt"), allow(dead_code))]
mod cmos;
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

    /// CIRCT flow: buffer every output to the delay-optimal
    /// round(log_rho F) stages (rho = sizing.stage_effort) and write a
    /// structural Verilog netlist under `verilog/`. `invertible` (the
    /// default) allows any number of added inverters, so an output may come
    /// out inverted; `non-invertible` adds them in inv+inv pairs.
    #[arg(long, value_enum, value_name = "MODE", num_args = 0..=1, default_missing_value = "invertible")]
    add_buffer: Option<passes::buffer::BufferMode>,

    /// CIRCT flow: also write the buffered netlist as SPICE
    /// (`spice/<top>_buffered.spice`). Implies `--add-buffer`.
    #[arg(long)]
    emit_buffered_spice: bool,
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

/// Parses `config.circt`'s MLIR input into the netlist IR, runs the pass
/// pipeline, then the CMOS backend: lowers the top module to
/// pull-up/pull-down stages, sizes them by logical effort against the
/// characterized reference inverter `unit`, and writes
/// `spice/<top>.spice`. With `--add-buffer` / `--emit-buffered-spice` it
/// then buffers the outputs and writes `verilog/<top>.v` (+ `<top>_cells.v`)
/// and optionally `spice/<top>_buffered.spice`.
#[cfg(feature = "circt")]
fn run_circt_flow(
    config: &Config,
    args: &Args,
    layout: &characterization::paths::BuildLayout,
    unit: backend::UnitInverter,
) -> Result<(), ExitCode> {
    use passes::buffer::BufferMode;
    use passes::sizing::SizingParams;

    let circt_cfg = config.circt.as_ref().expect("checked by caller");
    let sizing = config.sizing.as_ref().expect("validator requires [sizing] with [circt]");
    let fail = |what: &str, e: &dyn std::fmt::Display| {
        error!("{what}: {e}");
        ExitCode::FAILURE
    };

    let ctx = circt_ffi::CirctContext::new();
    let mlir_module = ctx
        .parse_file(std::path::Path::new(&circt_cfg.mlir_path))
        .map_err(|e| fail(&format!("Failed to parse CIRCT input '{}'", circt_cfg.mlir_path), &e))?;

    let mut design = ir::from_circt::build_design(&mlir_module)
        .map_err(|e| fail("Failed to build netlist from CIRCT IR", &e))?;
    let top = design.module_by_name(&circt_cfg.top_module).ok_or_else(|| {
        error!("Top module '{}' not found in '{}'", circt_cfg.top_module, circt_cfg.mlir_path);
        ExitCode::FAILURE
    })?;
    design.set_top(top);

    let params = SizingParams { gamma: unit.gamma(), cload: sizing.cload_cinv, cin_max: sizing.cin_cinv };
    let mut pipeline = passes::default_pipeline();
    let mut backend_pipeline = passes::backend_pipeline(params);
    info!(
        "Pass pipeline: {}, {}",
        pipeline.pass_names().join(", "),
        backend_pipeline.pass_names().join(", ")
    );
    pipeline.run(&mut design).map_err(|e| fail("Pass pipeline failed", &e))?;
    backend_pipeline.run(&mut design).map_err(|e| fail("Pass pipeline failed", &e))?;

    let top_name = backend::sanitize(&design.module(top).name);
    let write = |dir: &std::path::Path, file: String, text: Result<String, ir::IrError>| {
        let text = text.map_err(|e| fail(&format!("Failed to render {file}"), &e))?;
        let path = dir.join(&file);
        fs::create_dir_all(dir)
            .and_then(|_| fs::write(&path, text))
            .map_err(|e| fail(&format!("Failed to write {}", path.display()), &e))?;
        info!("Wrote {}", path.display());
        Ok::<_, ExitCode>(())
    };

    info!(
        "Sized '{}' (gamma = {:.3}, load = {} C_inv, max input = {} C_inv): stage effort {}",
        top_name,
        unit.gamma(),
        sizing.cload_cinv,
        sizing.cin_cinv,
        design.module(top).attrs.get(passes::sizing::ATTR_STAGE_EFFORT).map_or("-", String::as_str)
    );
    write(&layout.spice_dir, format!("{top_name}.spice"), backend::spice::render(design.module(top), config, &unit))?;

    let mode = args.add_buffer.or(args.emit_buffered_spice.then_some(BufferMode::Invertible));
    if let Some(mode) = mode {
        let mut buffering = passes::buffer_pipeline(mode, sizing.stage_effort, params);
        info!("Buffer pipeline ({mode:?}): {}", buffering.pass_names().join(", "));
        buffering.run(&mut design).map_err(|e| fail("Buffer pipeline failed", &e))?;

        let verilog_dir = args.build_dir.join("verilog");
        let m = design.module(top);
        write(&verilog_dir, format!("{top_name}.v"), backend::verilog::render(m))?;
        write(&verilog_dir, format!("{top_name}_cells.v"), backend::verilog::render_cells(m))?;
        if args.emit_buffered_spice {
            write(&layout.spice_dir, format!("{top_name}_buffered.spice"), backend::spice::render(m, config, &unit))?;
        }
    }

    log::debug!("Sized netlist: {:#?}", design);
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

    #[cfg_attr(not(feature = "circt"), allow(unused_variables))]
    let (wn, wp) = match inverter::generate_deck(&config, build_dir, &symbols, &session) {
        Ok((wn, wp)) => {
            info!("Reference inverter sized: Wn={:.4}um, Wp={:.4}um", wn, wp);
            (wn, wp)
        }
        Err(e) => {
            error!("Failed to generate inverter SPICE deck: {}", e);
            return ExitCode::FAILURE;
        }
    };

    #[cfg(feature = "circt")]
    if config.circt.is_some() {
        let (ln, lp) = inverter::reference_lengths(&config);
        let unit = backend::UnitInverter { wn, wp, ln, lp };
        let layout = match characterization::paths::BuildLayout::new(build_dir) {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to create build directory '{}': {}", build_dir.display(), e);
                return ExitCode::FAILURE;
            }
        };
        if let Err(code) = run_circt_flow(&config, &args, &layout, unit) {
            return code;
        }
    }
    print_run_summary(start.elapsed());

    ExitCode::SUCCESS
}
