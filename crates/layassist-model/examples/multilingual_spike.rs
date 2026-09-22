//! M1 spike: load a locally exported multilingual bundle and run two decisions.
//!
//! Point `LAYASSIST_M1_BUNDLE` at an export directory containing `laya.onnx`,
//! `laya_config.json` and `tokenizer/` (default `target/m1-export/onnx-multilingual`).
//! Set `LAYASSIST_M1_QUANT=int8` to load an int8 graph named `laya.onnx` there.
//!
//! Run it under `/usr/bin/time -v` to read the resident memory:
//!
//! ```sh
//! nix develop
//! LAYASSIST_M1_BUNDLE=target/m1-export/onnx-multilingual \
//!   /usr/bin/time -v cargo run -p layassist-model --example multilingual_spike
//! ```

use std::path::PathBuf;

use layassist_model::{Decider, Quant, bundle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("LAYASSIST_M1_BUNDLE")
        .map_or_else(|_| PathBuf::from("target/m1-export/onnx-multilingual"), PathBuf::from);
    let quant = match std::env::var("LAYASSIST_M1_QUANT").as_deref() {
        Ok("int8") => Quant::Int8,
        _ => Quant::Fp32,
    };

    let checkpoint = bundle::checkpoint_from_dir("laya-multilingual", &dir, quant)?;
    let mut decider = Decider::load(&checkpoint)?;
    eprintln!(
        "loaded {:?} checkpoint from {} (peak RSS {} MiB)",
        quant,
        dir.display(),
        peak_rss_kib().unwrap_or(0) / 1024
    );

    decide(
        &mut decider,
        "{}",
        "Which planet is known as the Red Planet?",
        &["Venus", "Mars", "Jupiter"],
    )?;
    decide(
        &mut decider,
        r#"{"passage": "The Eiffel Tower is located in Paris, the capital of France. It was completed in 1889."}"#,
        "In which city is the Eiffel Tower?",
        &["London", "Paris", "Berlin"],
    )?;
    eprintln!("peak RSS after decisions: {} MiB", peak_rss_kib().unwrap_or(0) / 1024);
    Ok(())
}

/// Peak resident set size in KiB, from `/proc/self/status` (Linux dev spike).
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
}

fn decide(
    decider: &mut Decider,
    state: &str,
    question: &str,
    answers: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let answers: Vec<String> = answers.iter().map(|answer| (*answer).to_string()).collect();
    let ranked = decider.decide_choice(state, question, &answers)?;
    println!("Q: {question}");
    for answer in &ranked {
        println!("  {:>6.2}%  {}", answer.probability * 100.0, answer.text);
    }
    Ok(())
}
