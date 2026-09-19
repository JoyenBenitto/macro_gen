use crate::characterization::device_params::SymbolTable;
use crate::characterization::ngspice_ffi::{FfiError, NgspiceSession};
use crate::characterization::paths::BuildLayout;
use crate::config::Config;
use log::{debug, info, warn};
use minijinja::{context, Environment};
use std::fs;
use std::path::Path;
use thiserror::Error;

const INVERTER_TEMPLATE: &str = include_str!("constants/inv.spice.j2");

/// Default total number of candidate Wp values swept when
/// `wl_sweep_sample_count` isn't set.
const DEFAULT_SWEEP_SAMPLE_COUNT: u32 = 11;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Ffi(#[from] FfiError),
}

/// Snaps `value` to the nearest positive integer multiple of `unit` when
/// `enabled`; otherwise passes it through unchanged.
fn quantize(value: f64, unit: f64, enabled: bool) -> f64 {
    if !enabled || unit <= 0.0 {
        return value;
    }
    let multiples = (value / unit).round().max(1.0);
    multiples * unit
}

/// Renders the inverter deck for a given sizing to `out_path`. `vout_csv_path`
/// must be absolute: ngspice (driven in-process via FFI) resolves a bare
/// relative `wrdata` filename against the host process's cwd, not
/// `out_path`'s directory.
fn render_deck(
    config: &Config,
    out_path: &Path,
    vout_csv_path: &Path,
    wn: f64,
    ln: f64,
    wp: f64,
    lp: f64,
) -> std::io::Result<()> {
    let mut env = Environment::new();
    env.add_template("inverter", INVERTER_TEMPLATE)
        .expect("inverter template is a compile-time constant and must be valid");
    let tmpl = env.get_template("inverter").unwrap();

    let rendered = tmpl
        .render(context! {
            vdd => config.environment.vdd,
            model_nmos => config.models.nmos,
            model_pmos => config.models.pmos,
            wn => wn,
            ln => ln,
            wp => wp,
            lp => lp,
            include_path => config.environment.include_path,
            corner => config.environment.corner,
            vout_sweep_path => vout_csv_path.display().to_string(),
        })
        .expect("template rendering failed with a valid, validated Config");

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, rendered)?;
    Ok(())
}

/// Measures the Vin=Vout switching threshold of a rendered deck by sourcing
/// it fresh and running `.meas dc ... WHEN v(net_out)=v(net_in) FALL=1`.
///
/// Regenerates and re-sources the whole deck per candidate rather than
/// `alter`ing a live device's W: this PDK's subckt-wrapped MOSFETs don't
/// support in-place width alteration (neither the subckt-level parameter
/// nor the internal BSIM instance's `w` can be altered without leaving
/// geometry parameters like nrd/nrs/ad/as inconsistent).
fn measure_switching_threshold(
    config: &Config,
    project_dir: &Path,
    session: &NgspiceSession,
    wn: f64,
    ln: f64,
    wp: f64,
    lp: f64,
) -> Result<f64, GenerateError> {
    let deck_path = project_dir.join("inv_sweep_candidate.spice");
    let vout_csv_path = project_dir.join("vout_sweep.csv");
    render_deck(config, &deck_path, &vout_csv_path, wn, ln, wp, lp)?;
    session.command(&format!("source {}", deck_path.display()))?;
    session.command(&format!("dc Vin 0 {} 0.01", config.environment.vdd))?;
    session.command("meas dc vth_meas WHEN v(net_out)=v(net_in) FALL=1")?;
    let vth = session.get_real("vth_meas")?;
    Ok(vth)
}

/// Sweeps `wl_sweep_sample_count` candidate Wp values around `wp_seed`
/// (step = `wl_sweep_granularity`, default `min_width`), measures each
/// one's actual switching threshold, and returns the `(wp, measured_vth)`
/// closest to `inverter_threshold`. A candidate whose measurement fails
/// (e.g. no crossing in the swept range) is logged and skipped rather than
/// aborting the sweep.
fn sweep_wp_for_target_vth(
    config: &Config,
    project_dir: &Path,
    session: &NgspiceSession,
    wn: f64,
    ln: f64,
    lp: f64,
    wp_seed: f64,
) -> Result<(f64, f64), GenerateError> {
    let ri = &config.reference_inverter;
    let min_width = config.environment.min_width;
    let step = ri.wl_sweep_granularity.unwrap_or(min_width);
    let sample_count = ri.wl_sweep_sample_count.unwrap_or(DEFAULT_SWEEP_SAMPLE_COUNT);
    let half = (sample_count / 2) as i32;
    let target = ri.inverter_threshold;

    let mut best: Option<(f64, f64, f64)> = None; // (wp, vth, |diff|)

    for i in -half..=half {
        let candidate_raw = (wp_seed + f64::from(i) * step).max(min_width);
        let candidate = quantize(candidate_raw, min_width, ri.w_is_multiple_of_w_min);

        match measure_switching_threshold(config, project_dir, session, wn, ln, candidate, lp) {
            Ok(measured) => {
                let diff = (measured - target).abs();
                debug!(
                    "sweep: Wp={:.4}um -> measured Vth={:.4}V (target={:.4}V, |diff|={:.4}V)",
                    candidate, measured, target, diff
                );
                if best.is_none_or(|(_, _, best_diff)| diff < best_diff) {
                    best = Some((candidate, measured, diff));
                }
            }
            Err(e) => {
                warn!(
                    "sweep: Wp={:.4}um failed to measure (no crossing in swept range?): {}",
                    candidate, e
                );
            }
        }
    }

    best.map(|(wp, vth, _)| (wp, vth))
        .ok_or_else(|| GenerateError::Ffi(FfiError::VectorUnavailable("vth_meas".to_string())))
}

/// Generates the analytical CMOS inverter deck: seeds the PMOS width from
/// `id_based_w_l_n_p_ratio`, sweeps around that seed to find the Wp whose
/// simulated switching threshold is closest to `inverter_threshold`, and
/// writes the result to `build_dir/spice/inv.spice`. Returns `(wn, wp)`
/// in microns.
pub fn generate_deck(
    config: &Config,
    build_dir: &Path,
    symbols: &SymbolTable,
    session: &NgspiceSession,
) -> Result<(f64, f64), GenerateError> {
    let layout = BuildLayout::new(build_dir)?;
    let ri = &config.reference_inverter;
    let min_width = config.environment.min_width;
    let min_length = config.environment.min_length;

    if ri.pmos_l.is_none() {
        warn!(
            "reference_inverter.pmos_l not set; assuming Lp = nmos_l ({:.4}um)",
            ri.nmos_l
        );
    }
    let ln = quantize(ri.nmos_l, min_length, ri.l_is_multiple_of_l_min);
    let lp = quantize(ri.pmos_l_or_default(), min_length, ri.l_is_multiple_of_l_min);

    let ratio = id_based_w_l_n_p_ratio(symbols);

    let wn = quantize(ri.nmos_w, min_width, ri.w_is_multiple_of_w_min);
    let wp_seed = quantize(wn * ratio, min_width, ri.w_is_multiple_of_w_min);

    debug!(
        "Running inverter characterization: vdd={}, corner={}, nmos={}, pmos={}",
        config.environment.vdd, config.environment.corner, config.models.nmos, config.models.pmos
    );
    debug!(
        "Analytical seed (Wp/Wn ratio={:.4}): Wn={:.4}um Ln={:.4}um, Wp_seed={:.4}um Lp={:.4}um",
        ratio, wn, ln, wp_seed, lp,
    );

    let (wp, measured_vth) =
        sweep_wp_for_target_vth(config, &layout.project_dir, session, wn, ln, lp, wp_seed)?;

    debug!(
        "Estimated inverter sizing (closest measured Vth={:.4}V to target={:.4}V): Wn={:.4}um Ln={:.4}um, Wp={:.4}um Lp={:.4}um",
        measured_vth, ri.inverter_threshold, wn, ln, wp, lp,
    );

    let out_path = layout.spice_dir.join("inv.spice");
    let vout_csv_path = layout.project_dir.join("vout_sweep.csv");
    render_deck(config, &out_path, &vout_csv_path, wn, ln, wp, lp)?;
    info!("Wrote inverter SPICE deck to {}", out_path.display());

    Ok((wn, wp))
}

/// Returns Wp/Wn as the ratio of the two devices' simulated drain currents
/// (`id`) at the normalize stage's characterization bias — both devices
/// share the same W/L there, so the ratio is meaningful without needing
/// to know it.
///
/// Deliberately uses measured `id`, not a reconstruction from `u0`/`cox`:
/// raw `u0` is a per-bin BSIM fit constant, not the true effective
/// mobility, and produces badly wrong ratios. This is only a seed for
/// `sweep_wp_for_target_vth`, which measures the real switching threshold
/// directly, so it only needs to be in the right ballpark.
pub fn id_based_w_l_n_p_ratio(symbols: &SymbolTable) -> f64 {
    let id_n = symbols.get("nmos.id").unwrap_or_else(|| {
        warn!("symbol 'nmos.id' missing from extracted device parameters; assuming 0.0");
        0.0
    }).abs();
    let id_p = symbols.get("pmos.id").unwrap_or_else(|| {
        warn!("symbol 'pmos.id' missing from extracted device parameters; assuming 0.0");
        0.0
    }).abs();
    id_n / id_p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characterization::device_params::DeviceParams;
    use std::collections::HashMap;

    #[test]
    fn ratio_matches_measured_id_ratio() {
        let mut params_by_device = HashMap::new();
        params_by_device.insert(
            "nmos".to_string(),
            DeviceParams {
                id: 1.641133e-4,
                ..Default::default()
            },
        );
        params_by_device.insert(
            "pmos".to_string(),
            DeviceParams {
                id: -6.116485e-5, // sign shouldn't matter; ratio uses abs()
                ..Default::default()
            },
        );
        let symbols = SymbolTable::from_device_params(&params_by_device);

        let ratio = id_based_w_l_n_p_ratio(&symbols);

        assert!(ratio.is_finite());
        assert!((ratio - 2.6829).abs() < 1e-3);
    }

    #[test]
    fn quantize_snaps_to_nearest_multiple() {
        assert_eq!(quantize(1.0, 0.42, true), 0.84);
        assert_eq!(quantize(0.05, 0.42, true), 0.42); // clamps up to 1x
        assert_eq!(quantize(1.05, 0.42, false), 1.05); // passthrough
    }
}
