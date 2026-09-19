use crate::config::Config;
use log::info;
use minijinja::{context, Environment};
use std::fs;
use std::path::Path;

const INVERTER_TEMPLATE: &str = include_str!("constants/inv.spice.j2");

/// No width field exists in Config yet, so we default to a sane starting
/// width (in microns) until sizing is driven by the config/CLI.
const DEFAULT_WIDTH_UM: f64 = 0.42;

pub fn logi(config: &Config, build_dir: &Path) -> std::io::Result<()> {
    info!(
        "Running inverter characterization: vdd={}, corner={}, nmos={}, pmos={}",
        config.environment.vdd, config.environment.corner, config.models.nmos, config.models.pmos
    );

    let mut env = Environment::new();
    env.add_template("inverter", INVERTER_TEMPLATE)
        .expect("inverter template is a compile-time constant and must be valid");
    let tmpl = env.get_template("inverter").unwrap();

    let rendered = tmpl
        .render(context! {
            vdd => config.environment.vdd,
            model_nmos => config.models.nmos,
            w => DEFAULT_WIDTH_UM,
            l => config.environment.min_length,
            include_path => config.environment.include_path,
            corner => config.environment.corner,
        })
        .expect("template rendering failed with a valid, validated Config");

    fs::create_dir_all(build_dir)?;
    let out_path = build_dir.join("inv.spice");
    fs::write(&out_path, rendered)?;

    info!("Wrote inverter SPICE deck to {}", out_path.display());
    Ok(())
}
