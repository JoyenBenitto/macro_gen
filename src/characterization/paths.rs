use std::fs;
use std::path::{Path, PathBuf};

/// Build-output layout, rooted at `build_dir` (the `--build-dir` CLI flag):
///   <build_dir>/
///     spice/                <- final decks only
///       inv.spice
///     .macro_gen_project/   <- intermediate/scratch files and reports
///       characterize.spice
///       device_params.csv
///       inv_sweep_candidate.spice
///       vout_sweep.csv
pub struct BuildLayout {
    /// Scratch/intermediate files and reports.
    pub project_dir: PathBuf,
    /// Final decks only.
    pub spice_dir: PathBuf,
}

impl BuildLayout {
    /// Paths are canonicalized to absolute: ngspice, driven in-process via
    /// FFI, resolves bare relative filenames (e.g. a `wrdata` target)
    /// against the host process's cwd, not `build_dir`.
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
