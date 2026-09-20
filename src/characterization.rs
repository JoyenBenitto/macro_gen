//! Drives ngspice to extract device parameters and size cells against a PDK config.
//!
//! `device_params` reads SPICE model parameters via ngspice, `ngspice_ffi` wraps the
//! in-process ngspice session, `inverter` runs the sizing/characterization sweep for a
//! reference inverter, `normalize` post-processes sweep results, and `paths` resolves
//! the output directory layout under a run's build directory.

pub mod device_params;
pub mod inverter;
pub mod ngspice_ffi;
pub mod normalize;
pub mod paths;
