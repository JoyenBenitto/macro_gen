use std::fs;
use std::path::{Path, PathBuf};

/// Build-output layout (`build_dir` itself is user-controlled — the `--build-dir`
/// CLI flag, defaulting to `./build`):
///   <build_dir>/
///     spice/                     <- final, user-facing decks only
///       inv.spice
///     .macro_gen_project/        <- all intermediate/scratch files and reports
///       characterize.spice
///       device_params.csv
///       inv_sweep_candidate.spice
///       vout_sweep.csv
pub struct BuildLayout {
    /// Scratch/intermediate files and reports live here (throwaway
    /// characterization decks, per-candidate sweep decks, CSV dumps).
    pub project_dir: PathBuf,
    /// Final decks — the reference inverter after all experiments — live
    /// here, directly under `build_dir`, and nowhere else.
    pub spice_dir: PathBuf,
}

impl BuildLayout {
    /// Paths are canonicalized to absolute so that ngspice — driven
    /// in-process via FFI, which resolves bare relative filenames (e.g. a
    /// `.control` block's `wrdata` target) against the host process's cwd
    /// rather than any notion of "the build directory" — always writes
    /// where intended, regardless of the invocation directory.
    pub fn new(build_dir: &Path) -> std::io::Result<Self> {
        let project_dir = build_dir.join(".macro_gen_project");
        let spice_dir = build_dir.join("spice");
        fs::create_dir_all(&project_dir)?;
        fs::create_dir_all(&spice_dir)?;
        Ok(BuildLayout {
            project_dir: project_dir.canonicalize()?,
            spice_dir: spice_dir.canonicalize()?,
        })
    }
}
