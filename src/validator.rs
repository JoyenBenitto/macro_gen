use crate::config::Config;
use std::path::Path;
use thiserror::Error;

const ALLOWED_CORNERS: &[&str] = &["tt", "ff", "ss", "sf", "fs"];
const REQUIRED_PLACEHOLDERS: &[&str] = &[
    "{{name}}", "{{d}}", "{{g}}", "{{s}}", "{{b}}", "{{model}}", "{{w}}", "{{l}}",
];
const MAX_SANE_VDD: f64 = 10.0;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("environment.vdd must be positive, got {0}")]
    VddNotPositive(f64),
    #[error("environment.vdd = {0} is outside sane range (0, {MAX_SANE_VDD}]")]
    VddOutOfRange(f64),
    #[error("environment.min_length must be positive, got {0}")]
    MinLengthNotPositive(f64),
    #[error("environment.include_path does not exist on disk: {0}")]
    IncludePathMissing(String),
    #[error("environment.corner '{0}' is not one of the allowed corners: {1:?}")]
    InvalidCorner(String, &'static [&'static str]),
    #[error("models.nmos must not be empty")]
    NmosEmpty,
    #[error("models.pmos must not be empty")]
    PmosEmpty,
    #[error("syntax.device_template is missing required placeholder(s): {0:?}")]
    TemplateMissingPlaceholders(Vec<&'static str>),
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

    let missing: Vec<&'static str> = REQUIRED_PLACEHOLDERS
        .iter()
        .filter(|p| !config.syntax.device_template.contains(*p))
        .copied()
        .collect();
    if !missing.is_empty() {
        errors.push(ValidationError::TemplateMissingPlaceholders(missing));
    }
    
    // Return the result of the validation
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}
