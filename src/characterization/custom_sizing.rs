//! Generalizes the sweep-based/analytical sizing flow (today: the
//! reference inverter) into a trait, so [`crate::sizing::size_netlist`]
//! can dispatch a config-tagged "custom cell" to it instead of logical
//! effort, without depending on the inverter's specific algorithm.

use crate::characterization::device_params::SymbolTable;
use crate::characterization::inverter::{self, GenerateError as InverterGenerateError};
use crate::characterization::ngspice_ffi::{FfiError, NgspiceSession};
use crate::characterization::paths::BuildLayout;
use crate::config::Config;
use crate::sizing::GateSizing;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Ffi(#[from] FfiError),
}

impl From<InverterGenerateError> for GenerateError {
    fn from(e: InverterGenerateError) -> Self {
        match e {
            InverterGenerateError::Io(e) => GenerateError::Io(e),
            InverterGenerateError::Ffi(e) => GenerateError::Ffi(e),
        }
    }
}

/// A cell sized by a custom, non-logical-effort flow (analytical seed +
/// simulation-based refinement, as today's inverter does) rather than the
/// generic logical-effort path.
pub trait CustomSizer {
    fn generate_deck(
        &self,
        config: &Config,
        layout: &BuildLayout,
        symbols: &SymbolTable,
        session: &NgspiceSession,
    ) -> Result<GateSizing, GenerateError>;
}

/// Wraps [`inverter::generate_deck`]'s existing sweep-based flow so it can
/// be dispatched to generically as one instance of [`CustomSizer`]. The
/// sizing algorithm itself is unchanged.
pub struct InverterSizer;

impl CustomSizer for InverterSizer {
    fn generate_deck(
        &self,
        config: &Config,
        layout: &BuildLayout,
        symbols: &SymbolTable,
        session: &NgspiceSession,
    ) -> Result<GateSizing, GenerateError> {
        let (wn, wp) = inverter::generate_deck_in_layout(config, layout, symbols, session)?;
        let l = config.reference_inverter.nmos_l;
        let lp = config.reference_inverter.pmos_l_or_default();
        Ok(GateSizing { wn, ln: l, wp, lp })
    }
}
