use crate::config::Config;

pub fn logi(config: &Config) {
    println!(
        "Running inverter characterization: vdd={}, corner={}, nmos={}, pmos={}",
        config.environment.vdd, config.environment.corner, config.models.nmos, config.models.pmos
    );
}