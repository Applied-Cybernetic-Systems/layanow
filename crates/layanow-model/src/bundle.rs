//! Locating, verifying, and (on first run) downloading an ONNX bundle.
//!
//! Per ADR-17 weights are never bundled: the app fetches the selected
//! checkpoint from Hugging Face into an XDG cache on first use, verifies every
//! file against the repository manifest, and prints the model's attribution.
//! Downloading is opt-in via [`ALLOW_DOWNLOAD_ENV`] so nothing touches the
//! network by accident.
//!
//! The bundle layout matches the published `receptron/laya-onnx` repo:
//! `laya.onnx` + `laya.onnx.data` (graph and external weights),
//! `laya_config.json`, and `tokenizer/`.

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
/// Default Hugging Face repo: the published English **fp32** bundle
/// (`receptron/laya-onnx`). The multilingual checkpoint and int8 graphs are
/// exported locally or selected via settings (ADR-28).
pub const DEFAULT_REPO: &str = "receptron/laya-onnx";

/// Files that make up a bundle, relative to its directory.
pub const BUNDLE_FILES: &[&str] = &[
    "laya.onnx",
    "laya.onnx.data",
    "laya_config.json",
    "tokenizer/tokenizer.json",
    "tokenizer/tokenizer_config.json",
];

/// Apache-2.0 attribution for the model weights (ADR-17).
pub const ATTRIBUTION: &str = "Laya model weights: \u{a9} Convai Innovations, Apache-2.0 \
(https://huggingface.co/convaiinnovations/laya). ONNX export: Receptron \
(https://github.com/receptron/laya, MIT).";

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

/// Ensure the default bundle is cached, downloading and verifying it only if
/// permitted.
pub fn ensure_bundle() -> Result<PathBuf, ModelError> {
    ensure_bundle_from(DEFAULT_REPO)
}

/// Ensure the bundle for `repo` is cached, downloading and verifying it only if
/// permitted.
///
/// Returns [`ModelError::BundleMissing`] when the files are absent and
/// [`ALLOW_DOWNLOAD_ENV`] is unset.
pub fn ensure_bundle_from(repo: &str) -> Result<PathBuf, ModelError> {
    let dir = bundle_dir(repo);
    if is_complete(&dir) {
        return Ok(dir);
    }
    if non_empty_env(ALLOW_DOWNLOAD_ENV).is_none() {
        return Err(ModelError::BundleMissing { path: dir, env: ALLOW_DOWNLOAD_ENV });
    }
    download_bundle(repo, &dir)?;
    Ok(dir)
}

/// Verify the cached default bundle's size and digests against the manifest.
pub fn verify_bundle() -> Result<(), ModelError> {
    verify_bundle_from(DEFAULT_REPO, &bundle_dir(DEFAULT_REPO))
}

/// Verify the bundle in `dir` against the manifest published for `repo`.
///
/// Hashes every file (SHA-256 for LFS blobs, git-object SHA-1 for small
/// non-LFS files), so it reads the whole bundle.
pub fn verify_bundle_from(repo: &str, dir: &Path) -> Result<(), ModelError> {
    let manifest = fetch_manifest(repo)?;
    for &file in BUNDLE_FILES {
        let expected = expect_file(&manifest, file)?;
        verify_file(&dir.join(file), expected, file)?;
    }
    Ok(())
}

/// Build a [`Checkpoint`] descriptor from an unpacked bundle directory.
///
/// The graph is always `laya.onnx` inside `dir`; `quant` only records the
/// intended precision (the bundle directory decides which graph that file is —
/// e.g. an int8 export renames `laya_int8.onnx` to `laya.onnx`). See `LAYA.md`.
/// Does not load the graph or tokenizer.
pub fn checkpoint_from_dir(name: &str, dir: &Path, quant: Quant) -> Result<Checkpoint, ModelError> {
    let config = crate::config::load(&dir.join("laya_config.json"))?;
    Ok(Checkpoint {
        name: name.to_string(),
        graph: dir.join("laya.onnx"),
        tokenizer: dir.join("tokenizer").join("tokenizer.json"),
        config,
        quant,
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

fn download_bundle(repo: &str, dir: &Path) -> Result<(), ModelError> {
    let manifest = fetch_manifest(repo)?;
    std::fs::create_dir_all(dir)?;
    for &file in BUNDLE_FILES {
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
        // The graph's external weights are ~1.7 GB: stream to a `.part` file and
        // rename only after the digest verifies, so an interrupted or corrupt
        // download is never accepted as complete.
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

fn is_complete(dir: &Path) -> bool {
    BUNDLE_FILES.iter().all(|file| is_present(&dir.join(file)))
}

fn is_present(path: &Path) -> bool {
    file_size(path).is_some_and(|size| size > 0)
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}
