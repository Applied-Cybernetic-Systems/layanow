//! Ask one checkpoint a `choice` question from the command line.
//!
//! A small diagnostic companion to the app: it renders and runs exactly the
//! same path (`Decider::decide_choice`) so a surprising answer can be
//! reproduced without the overlay.
//!
//! ```sh
//! nix develop
//! cargo run -p layanow-model --example ask -- \
//!   "What color is Mars?" "Red" "Blue" "Green"
//! ```
//!
//! `LAYANOW_CHECKPOINT` selects the checkpoint (default `english`),
//! `LAYANOW_QUANT` the precision (`fp32`/`fp16`/`int8`), and `LAYANOW_STATE`
//! the raw state JSON (default `{}`, the empty state of ADR-23). The bundle is
//! downloaded only if `LAYANOW_ALLOW_MODEL_DOWNLOAD=1` is set.

use layanow_model::{Decider, Quant, bundle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let question = args.next().ok_or("usage: ask <question> <answer> [answer...]")?;
    let answers: Vec<String> = args.collect();
    if answers.is_empty() {
        return Err("at least one answer is required".into());
    }

    let checkpoint_id =
        std::env::var("LAYANOW_CHECKPOINT").unwrap_or_else(|_| bundle::DEFAULT_ID.to_string());
    let quant = match std::env::var("LAYANOW_QUANT").as_deref() {
        Ok("fp16") => Quant::Fp16,
        Ok("int8") => Quant::Int8,
        _ => Quant::Fp32,
    };

    let spec = bundle::checkpoint(&checkpoint_id)
        .ok_or_else(|| format!("unknown checkpoint {checkpoint_id:?}"))?;
    let variant = bundle::variant_or_default(spec, quant)
        .ok_or_else(|| format!("{} has no graph variants", spec.id))?;
    let dir = bundle::ensure_bundle(spec, variant)?;
    let checkpoint = bundle::checkpoint_from_dir(spec, variant, &dir)?;
    let mut decider = Decider::load(&checkpoint)?;

    let state = std::env::var("LAYANOW_STATE").unwrap_or_else(|_| "{}".to_string());
    let ranked = decider.decide_choice(&state, &question, &answers)?;
    println!("Q: {question}  [{:?} · {} · state={state}]", variant.quant, spec.id);
    for answer in &ranked {
        println!("  {:>6.2}%  {}", answer.probability * 100.0, answer.text);
    }
    Ok(())
}
