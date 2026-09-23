//! Locating, verifying, and (on first run) downloading an ONNX bundle.
//!
//! Per ADR-17 weights are never bundled: the app fetches the selected
//! checkpoint from Hugging Face into an XDG cache on first use, verifies every
//! file against the repository manifest, and prints the model's attribution.
//! Downloading is opt-in via [`ALLOW_DOWNLOAD_ENV`] so nothing touches the
//! network by accident.
//!
//! Which files make up a bundle, and what they are called, differs between
//! exports: the official `receptron/laya-onnx` uses `laya.onnx` +
//! `laya.onnx.data` + `laya_config.json`, while the community multilingual
//! export uses a single `model-fp32.onnx` + `rl_agent_config.json`. A
//! [`CheckpointSpec`] records that layout so the same download/verify/load path
//! serves every checkpoint.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha1::Sha1;
use sha2::Sha256;
use sha2::digest::DynDigest;

use crate::{Checkpoint, Quant, error::ModelError};

/// Environment variable that must be set (to any value) to permit a download.
pub const ALLOW_DOWNLOAD_ENV: &str = "LAYANOW_ALLOW_MODEL_DOWNLOAD";
/// Optional environment variable overriding the cache root.
pub const CACHE_DIR_ENV: &str = "LAYANOW_MODEL_CACHE";

/// Apache-2.0 attribution for the model weights (ADR-17).
pub const ATTRIBUTION: &str = "Laya model weights: \u{a9} Convai Innovations, Apache-2.0 \
(https://huggingface.co/convaiinnovations/laya). ONNX export: Receptron \
(https://github.com/receptron/laya, MIT).";

/// A known checkpoint: where to fetch it and how its bundle is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointSpec {
    /// Stable id used in settings and as the settings-file value.
    pub id: &'static str,
    /// Human-readable name for the settings UI.
    pub name: &'static str,
    /// Hugging Face repo holding the ONNX bundle.
    pub repo: &'static str,
    /// Weight precision of the shipped graph.
    pub quant: Quant,
    /// Bundle-relative graph file name.
    pub graph: &'static str,
    /// Bundle-relative external-weights file, when the graph uses one.
    pub data: Option<&'static str>,
    /// Bundle-relative calibration config (`laya_config.json` or the source
    /// `rl_agent_config.json` — both carry the same keys we read).
    pub config: &'static str,
    /// Bundle-relative tokenizer directory (holds `tokenizer.json` and
    /// `tokenizer_config.json`).
    pub tokenizer_dir: &'static str,
}

/// English (ModernBERT-large, 421M), the official fp32 ONNX export (ADR-36).
pub const ENGLISH: CheckpointSpec = CheckpointSpec {
    id: "english",
    name: "English · ModernBERT-large",
    repo: "receptron/laya-onnx",
    quant: Quant::Fp32,
    graph: "laya.onnx",
    data: Some("laya.onnx.data"),
    config: "laya_config.json",
    tokenizer_dir: "tokenizer",
};

/// Multilingual (mmBERT-base, 322M), the community fp32 ONNX export (T-173).
pub const MULTILINGUAL: CheckpointSpec = CheckpointSpec {
    id: "multilingual",
    name: "Multilingual · mmBERT-base",
    repo: "soyelmismo/laya-multilingual-onnx",
    quant: Quant::Fp32,
    graph: "model-fp32.onnx",
    data: None,
    config: "rl_agent_config.json",
    tokenizer_dir: "tokenizer",
};

/// Every checkpoint the settings UI may offer.
pub const CHECKPOINTS: &[CheckpointSpec] = &[ENGLISH, MULTILINGUAL];

/// The checkpoint loaded when settings do not choose one (ADR-36).
pub const DEFAULT_ID: &str = "english";

/// Look up a known checkpoint by id.
#[must_use]
pub fn checkpoint(id: &str) -> Option<&'static CheckpointSpec> {
    CHECKPOINTS.iter().find(|spec| spec.id == id)
}

/// The bundle-relative files `spec` needs, in download order.
#[must_use]
pub fn files(spec: &CheckpointSpec) -> Vec<String> {
    let mut files = vec![spec.graph.to_string()];
    if let Some(data) = spec.data {
        files.push(data.to_string());
    }
    files.push(spec.config.to_string());
    files.push(format!("{}/tokenizer.json", spec.tokenizer_dir));
    files.push(format!("{}/tokenizer_config.json", spec.tokenizer_dir));
    files
}

/// The cache root for model bundles.
///
/// Honours [`CACHE_DIR_ENV`], then `XDG_CACHE_HOME`, then `~/.cache`, always
/// under `layanow/models` (ADR-17).
#[must_use]
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = non_empty_env(CACHE_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = non_empty_env("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("layanow").join("models");
    }
    PathBuf::from(non_empty_env("HOME").unwrap_or_default())
        .join(".cache")
        .join("layanow")
        .join("models")
}

/// The cache directory for a specific repo, e.g. `receptron--laya-onnx`.
#[must_use]
pub fn bundle_dir(repo: &str) -> PathBuf {
    cache_dir().join(repo.replace('/', "--"))
}

/// Ensure `spec`'s bundle is cached, downloading and verifying it only if
/// permitted.
///
/// Returns [`ModelError::BundleMissing`] when the files are absent and
/// [`ALLOW_DOWNLOAD_ENV`] is unset.
pub fn ensure_bundle(spec: &CheckpointSpec) -> Result<PathBuf, ModelError> {
    let dir = bundle_dir(spec.repo);
    let wanted = files(spec);
    if wanted.iter().all(|file| is_present(&dir.join(file))) {
        return Ok(dir);
    }
    if non_empty_env(ALLOW_DOWNLOAD_ENV).is_none() {
        return Err(ModelError::BundleMissing { path: dir, env: ALLOW_DOWNLOAD_ENV });
    }
    download_bundle(spec.repo, &wanted, &dir)?;
    Ok(dir)
}

/// Verify the cached bundle for `spec` against the published manifest.
pub fn verify_bundle(spec: &CheckpointSpec) -> Result<(), ModelError> {
    verify_bundle_from(spec, &bundle_dir(spec.repo))
}

/// Verify the bundle in `dir` against the manifest published for `spec`.
///
/// Hashes every file (SHA-256 for LFS blobs, git-object SHA-1 for small
/// non-LFS files), so it reads the whole bundle.
pub fn verify_bundle_from(spec: &CheckpointSpec, dir: &Path) -> Result<(), ModelError> {
    let manifest = fetch_manifest(spec.repo)?;
    for file in files(spec) {
        let expected = expect_file(&manifest, &file)?;
        verify_file(&dir.join(&file), expected, &file)?;
    }
    Ok(())
}

/// Build a [`Checkpoint`] descriptor from an unpacked bundle directory.
///
/// `spec` names the graph and config files inside `dir`; `quant` is `spec.quant`
/// (the bundle chooses which graph to place at `spec.graph`). Does not load the
/// graph or tokenizer.
pub fn checkpoint_from_dir(spec: &CheckpointSpec, dir: &Path) -> Result<Checkpoint, ModelError> {
    let config = crate::config::load(&dir.join(spec.config))?;
    Ok(Checkpoint {
        name: spec.name.to_string(),
        graph: dir.join(spec.graph),
        tokenizer: dir.join(spec.tokenizer_dir).join("tokenizer.json"),
        config,
        quant: spec.quant,
    })
}

/// Expected size and digest of one remote file.
struct ExpectedFile {
    size: u64,
    checksum: Checksum,
}

/// How a file is hashed. Hugging Face stores large weights in LFS (content
/// SHA-256) and small files as git blobs (git-object SHA-1).
enum Checksum {
    Sha256(String),
    GitBlobSha1(String),
}

fn fetch_manifest(repo: &str) -> Result<HashMap<String, ExpectedFile>, ModelError> {
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true");
    let response =
        ureq::get(&url).call().map_err(|error| ModelError::Manifest(format!("{url}: {error}")))?;
    let body =
        response.into_string().map_err(|error| ModelError::Manifest(format!("{url}: {error}")))?;
    let entries: Vec<Value> = serde_json::from_str(&body)
        .map_err(|error| ModelError::Manifest(format!("{url}: {error}")))?;

    let mut manifest = HashMap::new();
    for entry in entries {
        if entry.get("type").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(size) = entry.get("size").and_then(Value::as_u64) else {
            continue;
        };
        let checksum = if let Some(oid) =
            entry.get("lfs").and_then(|lfs| lfs.get("oid")).and_then(Value::as_str)
        {
            Checksum::Sha256(oid.to_string())
        } else if let Some(oid) = entry.get("oid").and_then(Value::as_str) {
            Checksum::GitBlobSha1(oid.to_string())
        } else {
            continue;
        };
        manifest.insert(path.to_string(), ExpectedFile { size, checksum });
    }
    Ok(manifest)
}

fn expect_file<'a>(
    manifest: &'a HashMap<String, ExpectedFile>,
    file: &str,
) -> Result<&'a ExpectedFile, ModelError> {
    manifest.get(file).ok_or_else(|| ModelError::ManifestMissing(file.to_string()))
}

fn download_bundle(repo: &str, files: &[String], dir: &Path) -> Result<(), ModelError> {
    let manifest = fetch_manifest(repo)?;
    std::fs::create_dir_all(dir)?;
    for file in files {
        let expected = expect_file(&manifest, file)?;
        let destination = dir.join(file);
        if is_present(&destination) {
            if file_size(&destination) == Some(expected.size) {
                continue;
            }
            std::fs::remove_file(&destination)?;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // The graph and its external weights can be over a gigabyte: stream to a
        // `.part` file and rename only after the digest verifies, so an
        // interrupted or corrupt download is never accepted as complete.
        let temporary = dir.join(format!("{file}.part"));
        let url = format!("https://huggingface.co/{repo}/resolve/main/{file}");
        tracing::info!(file, bytes = expected.size, "downloading model file");
        if let Err(error) = download_file(&url, &temporary, expected, file) {
            // Best-effort cleanup of the partial file; report the download error.
            drop(std::fs::remove_file(&temporary));
            return Err(error);
        }
        std::fs::rename(&temporary, &destination)?;
    }
    Ok(())
}

fn download_file(
    url: &str,
    path: &Path,
    expected: &ExpectedFile,
    file: &str,
) -> Result<(), ModelError> {
    let response =
        ureq::get(url).call().map_err(|error| ModelError::Download(format!("{url}: {error}")))?;
    let mut reader = response.into_reader();
    {
        let mut writer = std::fs::File::create(path)?;
        std::io::copy(&mut reader, &mut writer)?;
        writer.sync_all()?;
    }
    verify_file(path, expected, file)
}

fn verify_file(path: &Path, expected: &ExpectedFile, file: &str) -> Result<(), ModelError> {
    let actual_size = file_size(path).unwrap_or(0);
    if actual_size != expected.size {
        return Err(ModelError::ChecksumMismatch {
            file: file.to_string(),
            expected: format!("{} bytes", expected.size),
            actual: format!("{actual_size} bytes"),
        });
    }
    let reader = std::fs::File::open(path)?;
    let actual = hash_reader(reader, expected)?;
    let expected_hex = digest_hex(&expected.checksum);
    if actual != expected_hex {
        return Err(ModelError::ChecksumMismatch {
            file: file.to_string(),
            expected: expected_hex.to_string(),
            actual,
        });
    }
    Ok(())
}

fn hash_reader(mut reader: impl Read, expected: &ExpectedFile) -> Result<String, ModelError> {
    let mut hasher = hasher_for(&expected.checksum, expected.size);
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

fn hasher_for(checksum: &Checksum, size: u64) -> Box<dyn DynDigest> {
    match checksum {
        Checksum::Sha256(_) => Box::new(Sha256::default()),
        Checksum::GitBlobSha1(_) => {
            // A git blob hash covers `"blob <len>\0"` followed by the content.
            let mut hasher = Sha1::default();
            hasher.update(format!("blob {size}\0").as_bytes());
            Box::new(hasher)
        }
    }
}

fn digest_hex(checksum: &Checksum) -> &str {
    match checksum {
        Checksum::Sha256(hex) | Checksum::GitBlobSha1(hex) => hex,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn is_present(path: &Path) -> bool {
    file_size(path).is_some_and(|size| size > 0)
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids_resolve_to_specs() {
        assert_eq!(checkpoint("english"), Some(&ENGLISH));
        assert_eq!(checkpoint("multilingual"), Some(&MULTILINGUAL));
        assert_eq!(checkpoint("nope"), None);
        assert!(checkpoint(DEFAULT_ID).is_some());
    }

    #[test]
    fn file_lists_follow_each_layout() {
        assert_eq!(
            files(&ENGLISH),
            [
                "laya.onnx",
                "laya.onnx.data",
                "laya_config.json",
                "tokenizer/tokenizer.json",
                "tokenizer/tokenizer_config.json",
            ]
        );
        assert_eq!(
            files(&MULTILINGUAL),
            [
                "model-fp32.onnx",
                "rl_agent_config.json",
                "tokenizer/tokenizer.json",
                "tokenizer/tokenizer_config.json",
            ]
        );
    }
}
