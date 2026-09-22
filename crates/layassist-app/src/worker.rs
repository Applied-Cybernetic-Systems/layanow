//! The inference worker.
//!
//! Per ADR-1 the model runs on a dedicated thread and the UI talks to it over
//! channels, so the overlay never blocks on a decision. The worker owns the
//! [`DecisionEngine`] (in production a [`layassist_model::Decider`]) and answers
//! one [`Request`] at a time.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use layassist_model::RankedAnswer;

/// A unit of work for the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Run one `choice` decision.
    Decide {
        /// The question text.
        question: String,
        /// The candidate answers, in capture order.
        answers: Vec<String>,
    },
}

/// The worker's reply to a [`Request`].
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    /// Ranked answers (highest probability first).
    Ranked(Vec<RankedAnswer>),
    /// The decision failed; the string is user-facing.
    Failed(String),
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

impl DecisionEngine for layassist_model::Decider {
    fn decide_choice(
        &mut self,
        question: &str,
        answers: &[String],
    ) -> Result<Vec<RankedAnswer>, String> {
        // ADR-23: an empty `state` is the v1 default.
        layassist_model::Decider::decide_choice(self, "{}", question, answers)
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
    /// Spawn a worker that owns `engine`.
    pub fn spawn(engine: impl DecisionEngine) -> Self {
        let (requests, request_rx) = mpsc::channel::<Request>();
        let (response_tx, responses) = mpsc::channel::<Response>();
        let handle = thread::spawn(move || run(engine, &request_rx, &response_tx));
        Self { requests: Some(requests), responses, handle: Some(handle) }
    }

    /// Ask the worker to run a decision. Returns immediately.
    pub fn decide(&self, question: String, answers: Vec<String>) -> Result<(), WorkerGone> {
        self.requests
            .as_ref()
            .ok_or(WorkerGone)?
            .send(Request::Decide { question, answers })
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
    mut engine: impl DecisionEngine,
    requests: &Receiver<Request>,
    responses: &Sender<Response>,
) {
    while let Ok(request) = requests.recv() {
        let Request::Decide { question, answers } = request;
        let response = match engine.decide_choice(&question, &answers) {
            Ok(ranked) => Response::Ranked(ranked),
            Err(error) => Response::Failed(error),
        };
        if responses.send(response).is_err() {
            break;
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
        let worker = Worker::spawn(ByLength);
        worker.decide("q".to_string(), vec!["aa".to_string(), "b".to_string()]).unwrap();
        let Response::Ranked(ranked) = worker.recv().unwrap() else {
            panic!("expected ranked answers");
        };
        assert_eq!(ranked[0].text, "aa");
        assert_eq!(ranked[1].text, "b");
    }

    #[test]
    fn worker_propagates_failures() {
        let worker = Worker::spawn(Broken);
        worker.decide("q".to_string(), vec!["a".to_string()]).unwrap();
        assert_eq!(worker.recv().unwrap(), Response::Failed("no model".to_string()));
    }
}
