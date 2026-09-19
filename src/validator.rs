use crate::config::Config;
use std::path::Path;
use thiserror::Error;

const ALLOWED_CORNERS: &[&str] = &["tt", "ff", "ss", "sf", "fs"];
const MAX_SANE_VDD: f64 = 10.0;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("environment.vdd must be positive, got {0}")]
    VddNotPositive(f64),
    #[error("environment.vdd = {0} is outside sane range (0, {MAX_SANE_VDD}]")]
    VddOutOfRange(f64),
    #[error("environment.min_length must be positive, got {0}")]
    MinLengthNotPositive(f64),
    #[error("environment.min_width must be positive, got {0}")]
    MinWidthNotPositive(f64),
    #[error("environment.include_path does not exist on disk: {0}")]
    IncludePathMissing(String),
    #[error("environment.corner '{0}' is not one of the allowed corners: {1:?}")]
    InvalidCorner(String, &'static [&'static str]),
    #[error("models.nmos must not be empty")]
    NmosEmpty,
    #[error("models.pmos must not be empty")]
    PmosEmpty,
    // checks if reference inverter size is greater or equal to the environment defaults
    #[error("reference_inverter.{0} is smaller than environment.{1}: {2} < {3}")]
    InverterMosSizeTooSmall(&'static str, &'static str, f64, f64),
    #[error(
        "reference_inverter.inverter_threshold = {0} must lie strictly between 0 and environment.vdd = {1}"
    )]
    InverterThresholdOutOfRange(f64, f64),
}

#[derive(Debug, Error)]
#[error("configuration failed validation with {} error(s)", .0.len())]
pub struct ValidationErrors(pub Vec<ValidationError>);

pub fn validate(config: &Config) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();

    if config.environment.vdd <= 0.0 {
        errors.push(ValidationError::VddNotPositive(config.environment.vdd));
    } else if config.environment.vdd > MAX_SANE_VDD {
        errors.push(ValidationError::VddOutOfRange(config.environment.vdd));
    }

    if config.environment.min_length <= 0.0 {
        errors.push(ValidationError::MinLengthNotPositive(
            config.environment.min_length,
        ));
    }

    if config.environment.min_length <= 0.0 {
        errors.push(ValidationError::MinLengthNotPositive(
            config.environment.min_length,
        ));
    }

    if config.environment.min_width <= 0.0 {
        errors.push(ValidationError::MinWidthNotPositive(
            config.environment.min_width,
        ));
    }

    if !Path::new(&config.environment.include_path).exists() {
        errors.push(ValidationError::IncludePathMissing(
            config.environment.include_path.clone(),
        ));
    }

    if !ALLOWED_CORNERS.contains(&config.environment.corner.as_str()) {
        errors.push(ValidationError::InvalidCorner(
            config.environment.corner.clone(),
            ALLOWED_CORNERS,
        ));
    }

    if config.models.nmos.trim().is_empty() {
        errors.push(ValidationError::NmosEmpty);
    }
    if config.models.pmos.trim().is_empty() {
        errors.push(ValidationError::PmosEmpty);
    }

    let ri = &config.reference_inverter;
    let min_width = config.environment.min_width;
    let min_length = config.environment.min_length;

    if ri.nmos_w < min_width {
        errors.push(ValidationError::InverterMosSizeTooSmall(
            "nmos_w", "min_width", ri.nmos_w, min_width,
        ));
    }
    if ri.pmos_w < min_width {
        errors.push(ValidationError::InverterMosSizeTooSmall(
            "pmos_w", "min_width", ri.pmos_w, min_width,
        ));
    }
    if ri.nmos_l < min_length {
        errors.push(ValidationError::InverterMosSizeTooSmall(
            "nmos_l", "min_length", ri.nmos_l, min_length,
        ));
    }
    if ri.pmos_l < min_length {
        errors.push(ValidationError::InverterMosSizeTooSmall(
            "pmos_l", "min_length", ri.pmos_l, min_length,
        ));
    }

    if ri.inverter_threshold <= 0.0 || ri.inverter_threshold >= config.environment.vdd {
        errors.push(ValidationError::InverterThresholdOutOfRange(
            ri.inverter_threshold,
            config.environment.vdd,
        ));
    }

    // Return the result of the validation
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}
