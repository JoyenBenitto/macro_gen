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

/// Renders the inverter deck for a given sizing to `out_path`. The
/// `.control` block's `wrdata` target (`vout_csv_path`) is rendered as an
/// absolute path, since ngspice (driven in-process via FFI) resolves bare
/// relative filenames against the host process's cwd, not `out_path`'s
/// directory — that mismatch is why `vout_sweep.csv` was previously
/// landing in the repo root instead of the build directory.
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
/// Deliberately does *not* try to `alter` a live device's W in place:
/// tested interactively against this PDK, both `alter @<inst>[w]` (subckt
/// top-level parameter — fails outright, "no such device or model name")
/// and altering the internal BSIM instance's `w` directly (parses, but
/// leaves the subckt's derived geometry parameters — nrd/nrs/ad/as, baked
/// in from the original W — inconsistent, producing "Effective channel
/// width <= 0"). Regenerating and re-sourcing the whole deck per candidate
/// is the reliable path for this PDK's subckt-wrapped devices.
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

/// Sweeps candidate Wp values around `wp_seed` (step = `wl_sweep_granularity`,
/// or `min_width` if unset; `wl_sweep_sample_count` total candidates,
/// centered on the seed), measures each one's actual switching threshold,
/// and returns the `(wp, measured_vth)` closest to `inverter_threshold`.
/// Candidates whose measurement fails (e.g. no crossing in the swept
/// range) are logged and skipped rather than aborting the whole sweep.
///
/// `wl_sweep_granularity` below `min_width` while `w_is_multiple_of_w_min`
/// is set (every candidate would quantize back to the same width) is
/// rejected in `validator::validate`, not handled here.
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
/// actual simulated switching threshold is closest to
/// `inverter_threshold`, logs the estimated W/L for both devices, and
/// returns `(wn, wp)` in microns.
///
/// All intermediate/scratch files (per-candidate sweep decks, the sweep's
/// `vout_sweep.csv`) live under `build_dir/.macro_gen_project/`; only the
/// final, chosen deck — the reference inverter after all experiments —
/// lands in `build_dir/spice/inv.spice`.
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

/// Returns Wp/Wn as the ratio of the two devices' *actually simulated*
/// drain currents (`id`) at the normalize stage's characterization bias
/// (matched W/L for both devices there, so the ratio is directly
/// meaningful without needing to know that W/L).
///
/// This used to be reconstructed analytically from the velocity-saturation
/// square-law model — Id = (u0*cox/2)*(W/L)*(Ec*L/(Ec*L+Vov))*Vov^2 —
/// using `nmos.u0`/`pmos.u0` from `showmod`. That gave badly wrong results
/// on sky130: raw `u0` is a per-bin BSIM fit constant, not the real
/// bias-dependent effective mobility (BSIM applies additional degradation
/// terms — ua/ub/uc etc. — internally that only show up in simulated
/// outputs). Measured: nmos.u0/pmos.u0 ratio ≈ 53x, but the actually
/// simulated id/gm ratio ≈ 2.3-2.7x — sky130's expected ~2-3x range. This
/// is only a seed for `sweep_wp_for_target_vth` regardless, which measures
/// the real switching threshold directly, so exactness here matters less
/// than starting in a physically sane ballpark.
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
