//! Loading of a checkpoint's calibration config.
//!
//! Both the official `laya_config.json` and the source `rl_agent_config.json`
//! carry the `max_len`, `head_max_len`, `temperature` and
//! `temperature_by_options` keys read here (T-173); extra keys are ignored.

use std::path::Path;

use crate::{LayaConfig, error::ModelError};

/// Parse a checkpoint config (`laya_config.json`/`rl_agent_config.json`).
pub fn load(path: &Path) -> Result<LayaConfig, ModelError> {
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|source| ModelError::Config { path: path.to_path_buf(), source })?;

    let max_len = required_u32(&value, "max_len")?;
    let head_max_len = required_u32(&value, "head_max_len")?;

    let mut temperature = [1.0_f32; 3];
    if let Some(values) = value.get("temperature").and_then(serde_json::Value::as_array) {
        for (slot, entry) in temperature.iter_mut().zip(values) {
            if let Some(parsed) = as_f32(entry) {
                *slot = parsed;
            }
        }
    }

    let mut temperature_by_options = Vec::new();
    if let Some(map) = value.get("temperature_by_options").and_then(serde_json::Value::as_object) {
        for (name, entry) in map {
            if let Some(parsed) = as_f32(entry) {
                temperature_by_options.push((name.clone(), parsed));
            }
        }
    }

    Ok(LayaConfig { max_len, head_max_len, temperature, temperature_by_options })
}

fn required_u32(value: &serde_json::Value, field: &'static str) -> Result<u32, ModelError> {
    let raw = value
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or(ModelError::ConfigField(field))?;
    u32::try_from(raw).map_err(|_| ModelError::ConfigField(field))
}

// Temperatures are all order-1 floats, so narrowing `f64 -> f32` cannot lose a
// meaningful digit; the JSON parser hands us `f64` regardless.
#[allow(clippy::cast_possible_truncation)]
fn as_f32(value: &serde_json::Value) -> Option<f32> {
    value.as_f64().map(|parsed| parsed as f32)
}
