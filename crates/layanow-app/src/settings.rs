//! User settings persisted to `~/.config/layanow/config.toml` (E5).
//!
//! The file is optional: a missing or invalid file falls back to the default
//! settings and logs a warning, so the applet always starts.

use std::path::PathBuf;

use layanow_model::Quant;
use serde::{Deserialize, Serialize};

use crate::results::{DEFAULT_CONFIDENCE_THRESHOLD, Palette};

/// Whether the model stays resident or is unloaded between decisions
/// (ADR-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnloadPolicy {
    /// Keep the model resident (the default): instant answers, ~2 GB idle.
    #[default]
    Hot,
    /// Drop the model after each decision and reload on the next one.
    OnDemand,
}

/// A named, reusable context (the Laya `state`) template (ADR-40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedContext {
    /// The template's name (unique within [`Settings::contexts`]).
    pub name: String,
    /// The context text sent as the model's `state`.
    pub text: String,
}

/// The persisted configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Id of the checkpoint to load (ADR-7). An unknown id falls back to the
    /// default checkpoint at load time.
    pub checkpoint: String,
    /// Weight precision of the graph to load (ADR-39). A precision the
    /// checkpoint does not offer falls back to fp32 at load time.
    pub quant: Quant,
    /// Confidence below which a decision is flagged as low-confidence
    /// (ADR-27 C2). The app never refuses to answer.
    pub confidence_threshold: f32,
    /// Hot vs on-demand model residency (ADR-11).
    pub unload: UnloadPolicy,
    /// Probability-bar colour anchors.
    pub palette: Palette,
    /// Named context templates, selectable in the overlay (ADR-40).
    pub contexts: Vec<NamedContext>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            checkpoint: layanow_model::bundle::DEFAULT_ID.to_string(),
            quant: Quant::Fp32,
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            unload: UnloadPolicy::Hot,
            palette: Palette::default(),
            contexts: Vec::new(),
        }
    }
}

impl Settings {
    /// The path of the settings file:
    /// `$XDG_CONFIG_HOME/layanow/config.toml`, else `~/.config/layanow/config.toml`.
    #[must_use]
    pub fn path() -> PathBuf {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(dir).join("layanow").join("config.toml");
        }
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(".config")
            .join("layanow")
            .join("config.toml")
    }

    /// Load the persisted settings, falling back to defaults when the file is
    /// absent or cannot be parsed.
    #[must_use]
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "invalid settings; using defaults");
                Self::default()
            }
        }
    }

    /// Write the settings, creating the config directory if needed.
    ///
    /// # Errors
    /// Returns an error if the directory cannot be created or the file cannot
    /// be serialized/written.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(&path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let settings = Settings::default();
        let text = toml::to_string_pretty(&settings).expect("serialize");
        let parsed: Settings = toml::from_str(&text).expect("parse");
        assert_eq!(parsed, settings);
        assert_eq!(parsed.checkpoint, layanow_model::bundle::DEFAULT_ID);
        assert_eq!(parsed.quant, Quant::Fp32);
        assert!((parsed.confidence_threshold - DEFAULT_CONFIDENCE_THRESHOLD).abs() < f32::EPSILON);
        assert_eq!(parsed.unload, UnloadPolicy::Hot);
        assert_eq!(parsed.palette, Palette::default());
        assert!(parsed.contexts.is_empty());
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let parsed: Settings =
            toml::from_str("confidence_threshold = 0.7\nunload = \"on-demand\"\n").expect("parse");
        assert_eq!(parsed.checkpoint, layanow_model::bundle::DEFAULT_ID);
        assert_eq!(parsed.quant, Quant::Fp32);
        assert!((parsed.confidence_threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(parsed.unload, UnloadPolicy::OnDemand);
        assert_eq!(parsed.palette, Palette::default());
        assert!(parsed.contexts.is_empty());
    }

    #[test]
    fn contexts_round_trip_through_toml() {
        let settings = Settings {
            contexts: vec![NamedContext {
                name: "refund".to_string(),
                text: "Customer was billed twice.".to_string(),
            }],
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&settings).expect("serialize");
        let parsed: Settings = toml::from_str(&text).expect("parse");
        assert_eq!(parsed.contexts, settings.contexts);
    }

    #[test]
    fn precision_and_palette_round_trip_through_toml() {
        let settings = Settings {
            quant: Quant::Int8,
            palette: Palette { low: [1, 2, 3], mid: [4, 5, 6], high: [7, 8, 9] },
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&settings).expect("serialize");
        let parsed: Settings = toml::from_str(&text).expect("parse");
        assert_eq!(parsed.quant, Quant::Int8);
        assert_eq!(parsed.palette.low, [1, 2, 3]);
        assert_eq!(parsed.palette.high, [7, 8, 9]);
    }
}
