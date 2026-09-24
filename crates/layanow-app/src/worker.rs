//! The inference worker.
//!
//! Per ADR-1 the model runs on a dedicated thread and the UI talks to it over
//! channels, so the overlay never blocks on a decision. The worker owns a
//! decision engine built by an engine factory and answers one request at a
//! time.
//!
//! The factory indirection is what makes the settings real (T-113/T-117): the
//! engine can be (re)built for another checkpoint, and the on-demand policy
//! drops it after each decision. Loading therefore happens on this thread, not
//! on the applet's event loop, so a several-second `Decider::load` never freezes
//! the overlay.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use layanow_model::{Quant, RankedAnswer};

/// A unit of work for the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Run one `choice` decision.
    Decide {
        /// Caller-assigned decision id, echoed on the reply.
        id: u64,
        /// The question text.
        question: String,
        /// The candidate answers, in capture order.
        answers: Vec<String>,
    },
    /// Apply changed settings: rebuild for `checkpoint`/`quant` and set the
    /// unload policy (T-113/T-114/T-117).
    Configure {
        /// The checkpoint id to load.
        checkpoint: String,
        /// The weight precision to load.
        quant: Quant,
        /// Whether to unload the engine after each decision.
        on_demand: bool,
    },
}

/// The worker's reply to a [`Request`].
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    /// Ranked answers (highest probability first).
    Ranked {
        /// The id of the decision this replies to.
        id: u64,
        /// The ranked answers.
        ranked: Vec<RankedAnswer>,
    },
    /// The decision failed; the string is user-facing.
    Failed {
        /// The id of the decision this replies to.
        id: u64,
        /// The failure message.
        error: String,
    },
}

/// The inference backend the worker drives.
///
/// The trait lets the UI be exercised without the ~1.7 GB checkpoint.
pub trait DecisionEngine: Send + 'static {
    /// Decide which of `answers` best answers `question`.
    fn decide_choice(
        &mut self,
        question: &str,
        answers: &[String],
    ) -> Result<Vec<RankedAnswer>, String>;
}

/// Builds a decision engine for a checkpoint id and weight precision.
pub type EngineFactory =
    Box<dyn FnMut(&str, Quant) -> Result<Box<dyn DecisionEngine>, String> + Send>;

impl DecisionEngine for layanow_model::Decider {
    fn decide_choice(
        &mut self,
        question: &str,
        answers: &[String],
    ) -> Result<Vec<RankedAnswer>, String> {
        // ADR-23: an empty `state` is the v1 default.
        layanow_model::Decider::decide_choice(self, "{}", question, answers)
            .map_err(|error| error.to_string())
    }
}

/// The worker thread is no longer running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerGone;

impl std::fmt::Display for WorkerGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the inference worker has stopped")
    }
}

impl std::error::Error for WorkerGone {}

/// A running inference worker.
///
/// Dropping the `Worker` closes the request channel and joins the thread.
pub struct Worker {
    requests: Option<Sender<Request>>,
    responses: Receiver<Response>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// Spawn a worker driven by `factory`, starting on `checkpoint` at `quant`.
    ///
    /// Unless `on_demand`, the engine is loaded eagerly on the worker thread;
    /// with `on_demand` it is loaded on the first decision and dropped
    /// afterwards (T-117). Loading never blocks the caller.
    pub fn spawn(
        factory: EngineFactory,
        checkpoint: impl Into<String>,
        quant: Quant,
        on_demand: bool,
    ) -> Self {
        let (requests, request_rx) = mpsc::channel::<Request>();
        let (response_tx, responses) = mpsc::channel::<Response>();
        let checkpoint = checkpoint.into();
        let handle = thread::spawn(move || {
            run(factory, &request_rx, &response_tx, checkpoint, quant, on_demand);
        });
        Self { requests: Some(requests), responses, handle: Some(handle) }
    }

    /// Spawn a worker around a single, fixed engine (tests and headless use).
    ///
    /// The engine is loaded eagerly and never rebuilt.
    pub fn spawn_fixed(engine: impl DecisionEngine) -> Self {
        let mut engine: Option<Box<dyn DecisionEngine>> = Some(Box::new(engine));
        let factory: EngineFactory = Box::new(move |_, _| {
            engine.take().ok_or_else(|| "the fixed engine was already consumed".to_string())
        });
        Self::spawn(factory, "fixed", Quant::Fp32, false)
    }

    /// Ask the worker to run a decision `id`. Returns immediately.
    ///
    /// `id` is echoed on the matching [`Response`] so the caller can tell a
    /// reply for an abandoned decision from one for the current decision.
    pub fn decide(
        &self,
        id: u64,
        question: String,
        answers: Vec<String>,
    ) -> Result<(), WorkerGone> {
        self.requests
            .as_ref()
            .ok_or(WorkerGone)?
            .send(Request::Decide { id, question, answers })
            .map_err(|_| WorkerGone)
    }

    /// Apply changed settings (checkpoint, precision and unload policy).
    pub fn configure(
        &self,
        checkpoint: impl Into<String>,
        quant: Quant,
        on_demand: bool,
    ) -> Result<(), WorkerGone> {
        self.requests
            .as_ref()
            .ok_or(WorkerGone)?
            .send(Request::Configure { checkpoint: checkpoint.into(), quant, on_demand })
            .map_err(|_| WorkerGone)
    }

    /// Take the next reply without blocking.
    pub fn try_recv(&self) -> Option<Response> {
        self.responses.try_recv().ok()
    }

    /// Block until the next reply arrives (used by tests and the headless path).
    pub fn recv(&self) -> Result<Response, WorkerGone> {
        self.responses.recv().map_err(|_| WorkerGone)
    }

    fn stop(&mut self) {
        // Close the request channel so the run loop ends, then join.
        drop(self.requests.take());
        if let Some(handle) = self.handle.take() {
            drop(handle.join());
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    mut factory: EngineFactory,
    requests: &Receiver<Request>,
    responses: &Sender<Response>,
    mut checkpoint: String,
    mut quant: Quant,
    mut on_demand: bool,
) {
    let mut engine = if on_demand { None } else { load(&mut factory, &checkpoint, quant) };
    while let Ok(request) = requests.recv() {
        match request {
            Request::Decide { id, question, answers } => {
                if engine.is_none() {
                    engine = load(&mut factory, &checkpoint, quant);
                }
                let response = match engine.as_mut() {
                    Some(engine) => match engine.decide_choice(&question, &answers) {
                        Ok(ranked) => Response::Ranked { id, ranked },
                        Err(error) => Response::Failed { id, error },
                    },
                    None => {
                        Response::Failed { id, error: "the model could not be loaded".to_string() }
                    }
                };
                if responses.send(response).is_err() {
                    break;
                }
                if on_demand {
                    engine = None;
                }
            }
            Request::Configure {
                checkpoint: next,
                quant: next_quant,
                on_demand: next_on_demand,
            } => {
                if next != checkpoint || next_quant != quant {
                    checkpoint = next;
                    quant = next_quant;
                    engine = None;
                    if !next_on_demand {
                        engine = load(&mut factory, &checkpoint, quant);
                    }
                }
                on_demand = next_on_demand;
                if on_demand {
                    engine = None;
                }
            }
        }
    }
}

/// Build the engine for `checkpoint` at `quant`, logging (and swallowing) a load
/// failure so the next decision can report it to the UI.
fn load(
    factory: &mut EngineFactory,
    checkpoint: &str,
    quant: Quant,
) -> Option<Box<dyn DecisionEngine>> {
    match factory(checkpoint, quant) {
        Ok(engine) => Some(engine),
        Err(error) => {
            tracing::error!(%error, checkpoint, ?quant, "could not load the model");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic engine that ranks answers by length.
    struct ByLength;

    impl DecisionEngine for ByLength {
        fn decide_choice(
            &mut self,
            _question: &str,
            answers: &[String],
        ) -> Result<Vec<RankedAnswer>, String> {
            let length = |text: &String| f32::from(u16::try_from(text.len()).unwrap_or(u16::MAX));
            let total: f32 = answers.iter().map(length).sum();
            let mut ranked: Vec<RankedAnswer> = answers
                .iter()
                .enumerate()
                .map(|(index, text)| RankedAnswer {
                    index,
                    text: text.clone(),
                    probability: length(text) / total,
                    confidence: 0.25,
                })
                .collect();
            ranked.sort_by(|a, b| b.probability.total_cmp(&a.probability));
            Ok(ranked)
        }
    }

    /// An engine that always fails, to check error propagation.
    struct Broken;

    impl DecisionEngine for Broken {
        fn decide_choice(&mut self, _q: &str, _a: &[String]) -> Result<Vec<RankedAnswer>, String> {
            Err("no model".to_string())
        }
    }

    #[test]
    fn worker_returns_ranked_answers() {
        let worker = Worker::spawn_fixed(ByLength);
        worker.decide(7, "q".to_string(), vec!["aa".to_string(), "b".to_string()]).unwrap();
        let Response::Ranked { id, ranked } = worker.recv().unwrap() else {
            panic!("expected ranked answers");
        };
        assert_eq!(id, 7);
        assert_eq!(ranked[0].text, "aa");
        assert_eq!(ranked[1].text, "b");
    }

    #[test]
    fn worker_propagates_failures() {
        let worker = Worker::spawn_fixed(Broken);
        worker.decide(3, "q".to_string(), vec!["a".to_string()]).unwrap();
        assert_eq!(
            worker.recv().unwrap(),
            Response::Failed { id: 3, error: "no model".to_string() }
        );
    }

    #[test]
    fn on_demand_rebuilds_the_engine_for_each_decision() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        let builds = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&builds);
        let factory: EngineFactory = Box::new(move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(ByLength))
        });
        let worker = Worker::spawn(factory, "test", Quant::Fp32, true);
        for id in 0..2 {
            worker.decide(id, "q".to_string(), vec!["a".to_string()]).unwrap();
            drop(worker.recv().unwrap());
        }
        assert_eq!(builds.load(Ordering::Relaxed), 2, "on-demand loads once per decision");
    }

    #[test]
    fn reconfigure_swaps_the_checkpoint() {
        use std::sync::{Arc, Mutex};

        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let factory: EngineFactory = Box::new(move |checkpoint, _| {
            log.lock().expect("test lock").push(checkpoint.to_string());
            Ok(Box::new(ByLength))
        });
        let worker = Worker::spawn(factory, "english", Quant::Fp32, false);
        worker.decide(0, "q".to_string(), vec!["a".to_string()]).unwrap();
        drop(worker.recv().unwrap());
        worker.configure("multilingual", Quant::Fp32, false).unwrap();
        worker.decide(1, "q".to_string(), vec!["a".to_string()]).unwrap();
        drop(worker.recv().unwrap());
        assert_eq!(&*seen.lock().expect("test lock"), &["english", "multilingual"]);
    }

    #[test]
    fn reconfigure_swaps_the_precision() {
        use std::sync::{Arc, Mutex};

        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let factory: EngineFactory = Box::new(move |_, quant| {
            log.lock().expect("test lock").push(quant);
            Ok(Box::new(ByLength))
        });
        let worker = Worker::spawn(factory, "english", Quant::Fp32, false);
        worker.decide(0, "q".to_string(), vec!["a".to_string()]).unwrap();
        drop(worker.recv().unwrap());
        worker.configure("english", Quant::Int8, false).unwrap();
        worker.decide(1, "q".to_string(), vec!["a".to_string()]).unwrap();
        drop(worker.recv().unwrap());
        assert_eq!(&*seen.lock().expect("test lock"), &[Quant::Fp32, Quant::Int8]);
    }
}
