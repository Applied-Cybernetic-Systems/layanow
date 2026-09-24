//! Precision parity check (T-121, ADR-24/39).
//!
//! Compares a checkpoint's fp16 and int8 graphs against its fp32 reference on a
//! small, self-made MCQ set (ADR-12): top-1 agreement and probability drift.
//! This is the evidence behind offering a lower-precision graph in settings;
//! it is a dev tool, not a CI test (it needs the multi-gigabyte bundles).
//!
//! ```sh
//! nix develop
//! # first run only: fetch the graphs into the XDG cache
//! LAYANOW_ALLOW_MODEL_DOWNLOAD=1 \
//!   cargo run -p layanow-model --example precision_parity
//! LAYANOW_PARITY_CHECKPOINT=multilingual \
//!   cargo run -p layanow-model --example precision_parity
//! ```

// The counts here are tiny (dozens), so casting them to float for a report
// cannot lose a meaningful digit; this is a dev-only example.
#![allow(clippy::cast_precision_loss)]

use layanow_model::{Decider, Quant, bundle};

/// One self-made multiple-choice case (neutral, no answer bank — ADR-12).
struct Case {
    state: &'static str,
    question: &'static str,
    answers: &'static [&'static str],
}

const EMPTY: &str = "{}";

const CASES: &[Case] = &[
    Case {
        state: EMPTY,
        question: "Which planet is known as the Red Planet?",
        answers: &["Venus", "Mars", "Jupiter"],
    },
    Case {
        state: EMPTY,
        question: "What is the capital of France?",
        answers: &["Berlin", "Madrid", "Paris", "Rome"],
    },
    Case {
        state: EMPTY,
        question: "Which is the largest ocean on Earth?",
        answers: &["Atlantic", "Indian", "Pacific", "Arctic"],
    },
    Case {
        state: EMPTY,
        question: "What is the chemical symbol for gold?",
        answers: &["Ag", "Au", "Gd", "Go"],
    },
    Case {
        state: EMPTY,
        question: "Who wrote the play Romeo and Juliet?",
        answers: &["Charles Dickens", "William Shakespeare", "Jane Austen"],
    },
    Case {
        state: EMPTY,
        question: "How many continents are there on Earth?",
        answers: &["Five", "Six", "Seven"],
    },
    Case {
        state: EMPTY,
        question: "Which language has the most native speakers?",
        answers: &["English", "Mandarin Chinese", "Spanish", "Hindi"],
    },
    Case {
        state: EMPTY,
        question: "What gas do plants absorb from the atmosphere?",
        answers: &["Oxygen", "Carbon dioxide", "Nitrogen"],
    },
    Case {
        state: r#"{"passage": "The Eiffel Tower is located in Paris, the capital of France. It was completed in 1889."}"#,
        question: "In which city is the Eiffel Tower?",
        answers: &["London", "Paris", "Berlin"],
    },
    Case {
        state: r#"{"passage": "Water boils at 100 degrees Celsius at sea level and freezes at 0 degrees Celsius."}"#,
        question: "At what temperature does water boil at sea level?",
        answers: &["0 degrees", "50 degrees", "100 degrees"],
    },
    Case {
        state: r#"{"subject": "Refund not received", "body": "I cancelled my subscription two weeks ago and I still have not received my refund."}"#,
        question: "Which team should handle this ticket?",
        answers: &[
            "billing: payments, refunds, invoices",
            "support: product help and bugs",
            "sales: new purchases and upgrades",
        ],
    },
    Case {
        state: r#"{"ticket": "Customer cannot log in; the two-factor authentication code never arrives."}"#,
        question: "Which team should handle this ticket?",
        answers: &[
            "billing: payments, refunds, invoices",
            "support: product help and bugs",
            "sales: new purchases and upgrades",
        ],
    },
    // A few non-English cases so the multilingual checkpoint is exercised too.
    Case {
        state: EMPTY,
        question: "Quelle est la capitale de l'Italie ?",
        answers: &["Milan", "Rome", "Naples"],
    },
    Case {
        state: EMPTY,
        question: "¿Cuál es el océano más grande del mundo?",
        answers: &["Atlántico", "Índico", "Pacífico"],
    },
    Case {
        state: EMPTY, question: "日本の首都はどこですか？", answers: &["大阪", "東京", "京都"]
    },
    Case {
        state: EMPTY,
        question: "Was ist die Hauptstadt von Frankreich?",
        answers: &["Lyon", "Paris", "Marseille"],
    },
    // Bulgarian and German, to check whether the multilingual graph behaves
    // differently for languages other than English.
    Case {
        state: EMPTY,
        question: "Коя е столицата на България?",
        answers: &["София", "Пловдив", "Варна"],
    },
    Case {
        state: EMPTY,
        question: "Коя е най-дългата река в България?",
        answers: &["Дунав", "Марица", "Искър"],
    },
    Case {
        state: EMPTY,
        question: "Was ist die Hauptstadt von Deutschland?",
        answers: &["Berlin", "München", "Hamburg"],
    },
    Case {
        state: EMPTY,
        question: "Welche Farbe hat der Himmel an einem klaren Tag?",
        answers: &["Blau", "Rot", "Grün"],
    },
];

/// One case's result: the winning answer index and the per-option probabilities.
struct Outcome {
    top: usize,
    top_text: String,
    probabilities: Vec<f32>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let checkpoint_id = std::env::var("LAYANOW_PARITY_CHECKPOINT")
        .unwrap_or_else(|_| bundle::DEFAULT_ID.to_string());
    let spec = bundle::checkpoint(&checkpoint_id)
        .ok_or_else(|| format!("unknown checkpoint {checkpoint_id:?}"))?;

    println!("checkpoint: {} ({})", spec.name, spec.id);
    let reference = decide_all(&mut load(spec, Quant::Fp32)?)?;

    for quant in [Quant::Fp16, Quant::Int8] {
        if bundle::variant(spec, quant).is_none() {
            continue;
        }
        let candidate = decide_all(&mut load(spec, quant)?)?;
        report(quant, &reference, &candidate);
    }
    println!(
        "\nDone. fp16 is expected to match fp32; a precision below the bar is offered \
         only with a lower-accuracy warning (ADR-39)."
    );
    Ok(())
}

/// Load `spec` at `quant`, downloading and verifying the bundle if needed.
fn load(
    spec: &bundle::CheckpointSpec,
    quant: Quant,
) -> Result<Decider, Box<dyn std::error::Error>> {
    let variant = bundle::variant(spec, quant)
        .ok_or_else(|| format!("{} has no {quant:?} variant", spec.id))?;
    let dir = bundle::ensure_bundle(spec, variant)?;
    let checkpoint = bundle::checkpoint_from_dir(spec, variant, &dir)?;
    println!("  loaded {:?} from {}", quant, dir.display());
    Ok(Decider::load(&checkpoint)?)
}

fn decide_all(decider: &mut Decider) -> Result<Vec<Outcome>, Box<dyn std::error::Error>> {
    let mut outcomes = Vec::with_capacity(CASES.len());
    for case in CASES {
        let answers: Vec<String> =
            case.answers.iter().map(|answer| (*answer).to_string()).collect();
        let ranked = decider.decide_choice(case.state, case.question, &answers)?;
        let mut probabilities = vec![0.0_f32; answers.len()];
        for answer in &ranked {
            if let Some(slot) = probabilities.get_mut(answer.index) {
                *slot = answer.probability;
            }
        }
        let top = ranked.first().map_or(0, |answer| answer.index);
        let top_text = ranked.first().map_or_else(String::new, |answer| answer.text.clone());
        outcomes.push(Outcome { top, top_text, probabilities });
    }
    Ok(outcomes)
}

/// Print the agreement/drift for `quant` and whether it meets the ADR-24 bar.
fn report(quant: Quant, reference: &[Outcome], candidate: &[Outcome]) {
    let mut agree = 0;
    let mut max_delta = 0.0_f32;
    let mut sum_delta = 0.0_f32;
    let mut slots = 0;
    for (index, (want, got)) in reference.iter().zip(candidate).enumerate() {
        let same = want.top == got.top;
        if same {
            agree += 1;
        }
        for (a, b) in want.probabilities.iter().zip(&got.probabilities) {
            let delta = (a - b).abs();
            max_delta = max_delta.max(delta);
            sum_delta += delta;
            slots += 1;
        }
        println!(
            "  case {index:2}: {}  (fp32 {:.3} {:.3} -> {quant:?} {:.3} {:.3})",
            if same { "agree " } else { "DIFFER" },
            want.probabilities.get(want.top).copied().unwrap_or(0.0),
            want.top_text,
            got.probabilities.get(got.top).copied().unwrap_or(0.0),
            got.top_text,
        );
    }
    let agreement =
        if reference.is_empty() { 0.0 } else { f64::from(agree) / reference.len() as f64 };
    let mean_delta = if slots == 0 { 0.0 } else { sum_delta / slots as f32 };
    let verdict = if agreement >= 0.99 {
        "meets the ADR-24 bar"
    } else {
        "BELOW the ADR-24 bar (offered only with a warning, ADR-39)"
    };
    println!(
        "{quant:?}: top-1 agreement {:.3} ({agree}/{}), mean |p-q| {mean_delta:.5}, \
         max |p-q| {max_delta:.5} — {verdict}",
        agreement,
        reference.len(),
    );
}
