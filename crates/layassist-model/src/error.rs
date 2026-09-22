//! Errors returned by the model layer.

use std::path::PathBuf;

/// Errors from acquiring, loading, or running a Laya checkpoint.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// A filesystem operation failed.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// The ONNX Runtime session could not be built or run.
    #[error("onnx runtime error: {0}")]
    Ort(#[from] ort::Error),
    /// The ONNX Runtime session *builder* rejected an option. Boxed because a
    /// raw `ort::Error<SessionBuilder>` is ~144 bytes, which would make every
    /// `Result<_, ModelError>` oversized (`clippy::result_large_err`).
    #[error("onnx runtime builder error: {0}")]
    OrtBuilder(#[from] Box<ort::Error<ort::session::builder::SessionBuilder>>),
    /// The tokenizer could not be loaded or applied.
    #[error("tokenizer error: {0}")]
    Tokenizer(#[from] tokenizers::Error),
    /// `laya_config.json` could not be parsed.
    #[error("could not parse {path}: {source}")]
    Config {
        /// Path to the offending file.
        path: PathBuf,
        /// The underlying JSON error.
        #[source]
        source: serde_json::Error,
    },
    /// `laya_config.json` parsed but is missing a required field.
    #[error("laya_config.json is missing or malformed: {0}")]
    ConfigField(&'static str),
    /// A token the renderer needs is absent from the tokenizer vocabulary.
    #[error("tokenizer is missing the required special token {0}")]
    MissingSpecialToken(String),
    /// A `choice` decision was requested with no candidate answers.
    #[error("a choice decision needs at least one answer")]
    NoAnswers,
    /// The graph returned an output of an unexpected type or shape.
    #[error("unexpected model output: {0}")]
    UnexpectedOutput(&'static str),
    /// The bundle is not cached and first-run downloading is not enabled.
    #[error(
        "Laya bundle not found at {path}; rerun with {env}=1 to download it, \
         or place the bundle files there manually"
    )]
    BundleMissing {
        /// Expected bundle directory.
        path: PathBuf,
        /// Environment variable that enables downloading.
        env: &'static str,
    },
    /// A bundle file could not be downloaded.
    #[error("model download failed: {0}")]
    Download(String),
    /// The remote file manifest could not be read.
    #[error("could not read the model file manifest: {0}")]
    Manifest(String),
    /// The manifest has no entry for a required bundle file.
    #[error("model manifest has no entry for {0}")]
    ManifestMissing(String),
    /// A downloaded or cached file did not match its manifest digest/size.
    #[error("integrity check failed for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Bundle-relative file name.
        file: String,
        /// Expected digest or size.
        expected: String,
        /// Actual digest or size.
        actual: String,
    },
}
