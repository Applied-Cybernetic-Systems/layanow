//! The Laya ONNX model: tokenizer, input rendering, calibration, and the
//! checkpoint registry.
//!
//! The ONNX graph contract and per-checkpoint export steps are documented in
//! `LAYA.md`. This crate is a skeleton until M2; the types below fix the shape
//! of the registry and the decision result.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::PathBuf;

/// Weight precision of a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quant {
    /// 8-bit quantized weights (default; smallest footprint).
    Int8,
    /// 32-bit floating point weights (larger, highest fidelity).
    Fp32,
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
