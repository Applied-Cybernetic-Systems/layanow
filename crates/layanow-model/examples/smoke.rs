//! M0 smoke test: ask Laya one 3-option `choice` question and print the ranked
//! probabilities.
//!
//! ```sh
//! nix develop
//! # first run only: fetch the ~1.7 GB bundle into the XDG cache
//! LAYANOW_ALLOW_MODEL_DOWNLOAD=1 cargo run -p layanow-model --example smoke
//! # later runs reuse the cache and touch the network only in the example below
//! cargo run -p layanow-model --example smoke
//! ```

use layanow_model::{Decider, bundle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = &bundle::ENGLISH;
    let dir = bundle::ensure_bundle(spec)?;
    eprintln!("using bundle at {}", dir.display());
    println!("{}", bundle::ATTRIBUTION);

    let checkpoint = bundle::checkpoint_from_dir(spec, &dir)?;
    let mut decider = Decider::load(&checkpoint)?;

    let question = "Which planet is known as the Red Planet?";
    let answers = vec!["Venus".to_string(), "Mars".to_string(), "Jupiter".to_string()];

    let ranked = decider.decide_choice("{}", question, &answers)?;
    println!("question: {question}");
    for answer in &ranked {
        println!("{:>6.2}%  {}", answer.probability * 100.0, answer.text);
    }
    if let Some(top) = ranked.first() {
        println!("top: {} (confidence {:.4})", top.text, top.confidence);
    }
    Ok(())
}
