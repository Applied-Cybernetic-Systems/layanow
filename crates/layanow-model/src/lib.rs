//! The Laya ONNX model: tokenizer, input rendering, calibration, and the
//! checkpoint registry.
//!
//! The ONNX graph contract and per-checkpoint export steps are documented in
//! `LAYA.md`. The pieces are:
//!
//! - [`bundle`] locates (and optionally downloads) a checkpoint bundle;
//! - [`render`] is the pure port of Laya's sequence/temperature rendering;
//! - [`decider::Decider`] loads a bundle and runs a `choice` decision.
//!
//! The `ort` session loads the system `libonnxruntime` through
//! `ORT_DYLIB_PATH` (set by the Nix dev shell via the `load-dynamic` feature).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod bundle;
mod config;
pub mod decider;
pub mod error;
pub mod render;

pub use decider::Decider;
pub use error::ModelError;

/// Weight precision of a checkpoint graph (ADR-24/28/36/39).
///
/// Each [`bundle::CheckpointSpec`] offers one or more of these as
/// [`bundle::GraphVariant`]s; the settings UI picks one and `Quant` selects
/// which graph file is loaded. The default is [`Quant::Fp32`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quant {
    /// 32-bit floating point weights (largest, highest fidelity); the default.
    #[default]
    Fp32,
    /// 16-bit floating point weights (half the size; software-emulated on CPUs
    /// without native FP16, so slower — opt-in per ADR-39).
    Fp16,
    /// 8-bit quantized weights (smallest, fastest; opt-in, gated on the ADR-24
    /// agreement bar — see ADR-28/36/39).
    Int8,
}

/// Per-checkpoint calibration and length configuration, from `laya_config.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayaConfig {
    /// Maximum total sequence length.
    pub max_len: u32,
    /// Token budget for the question head and options.
    pub head_max_len: u32,
    /// Per-question-type temperature scaling.
    pub temperature: [f32; 3],
    /// Optional per-cardinality-bucket temperatures.
    pub temperature_by_options: Vec<(String, f32)>,
}

/// A loadable Laya checkpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    /// Human-readable checkpoint name.
    pub name: String,
    /// Path to the ONNX graph (`laya.onnx`).
    pub graph: PathBuf,
    /// Path to `tokenizer.json`.
    pub tokenizer: PathBuf,
    /// Calibration/length configuration.
    pub config: LayaConfig,
    /// Weight precision.
    pub quant: Quant,
}

/// One ranked answer with its calibrated probability and confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedAnswer {
    /// Index into the caller-supplied answer list.
    pub index: usize,
    /// The answer text.
    pub text: String,
    /// Calibrated probability in `0.0..=1.0`.
    pub probability: f32,
    /// Jev-style confidence (`1 - normalized entropy`).
    pub confidence: f32,
}
