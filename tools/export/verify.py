"""Compare a Laya fp32 ONNX graph with its dynamic-int8 twin on real inputs.

Usage: verify.py <model_dir> <onnx_dir> [int8.onnx]

  model_dir : directory holding ``rl_common.py`` (the checkpoint's rendering)
  onnx_dir  : directory holding ``laya.onnx``, ``laya_int8.onnx``,
              ``tokenizer/`` and ``laya_config.json``

Reports, per sample group and overall:
  * fp32 top-1 accuracy against the known answer (A5 rendering sanity check),
  * int8-vs-fp32 top-1 agreement and Jensen-Shannon divergence (ADR-24).

Samples are neutral and self-made (ADR-12). "trivia" questions carry no state
(the ADR-23 default); "context" questions put a short passage in ``state``.
Build-time only (ADR-9).
"""

import json
import os
import sys
import time

import numpy as np
import onnxruntime as ort

sys.path.insert(0, os.path.abspath(sys.argv[1]))
from rl_common import QTYPES, build_sequence, collate_items  # noqa: E402

from transformers import AutoTokenizer  # noqa: E402

INTRA_THREADS = 4


def _choice(state, group, question, criteria, correct):
    return {
        "group": group,
        "state": state,
        "q": {"t": "choice", "ins": question, "crit": criteria},
        "correct": correct,
    }


# Neutral, self-made MCQs (ADR-12: no answer bank, no SkillsBuild coupling).
SAMPLES = [
    _choice({}, "trivia", "Which planet is known as the Red Planet?",
            {"A": "Venus", "B": "Mars", "C": "Jupiter"}, "B"),
    _choice({}, "trivia", "What is the capital of France?",
            {"A": "Berlin", "B": "Madrid", "C": "Paris", "D": "Rome"}, "C"),
    _choice({}, "trivia", "Which is the largest ocean on Earth?",
            {"A": "Atlantic", "B": "Indian", "C": "Pacific", "D": "Arctic"}, "C"),
    _choice({}, "trivia", "What is the chemical symbol for gold?",
            {"A": "Ag", "B": "Au", "C": "Gd", "D": "Go"}, "B"),
    _choice({}, "trivia", "Who wrote the play Romeo and Juliet?",
            {"A": "Charles Dickens", "B": "William Shakespeare", "C": "Jane Austen"}, "B"),
    _choice({}, "trivia", "How many continents are there on Earth?",
            {"A": "Five", "B": "Six", "C": "Seven"}, "C"),
    _choice({}, "trivia", "Quelle est la capitale de l'Italie ?",
            {"A": "Milan", "B": "Rome", "C": "Naples"}, "B"),
    _choice({}, "trivia", "¿Cuál es el océano más grande del mundo?",
            {"A": "Atlántico", "B": "Índico", "C": "Pacífico"}, "C"),
    _choice({}, "trivia", "Was ist die Hauptstadt von Frankreich?",
            {"A": "Lyon", "B": "Paris", "C": "Marseille"}, "B"),
    _choice({}, "trivia", "Qual é o maior oceano do mundo?",
            {"A": "Atlântico", "B": "Índico", "C": "Pacífico"}, "C"),
    _choice({}, "trivia", "日本の首都はどこですか？",
            {"A": "大阪", "B": "東京", "C": "京都"}, "B"),
    _choice({}, "trivia", "中国的首都是哪里？",
            {"A": "上海", "B": "北京", "C": "广州"}, "B"),
    # Passage/question/options: the intended "state + decision" shape.
    _choice({"subject": "Refund not received",
             "body": "I cancelled my subscription two weeks ago and I still have "
             "not received my refund."},
            "context", "Which team should handle this ticket?",
            {"billing": "payments, refunds, invoices",
             "support": "product help and bugs",
             "sales": "new purchases and upgrades"}, "billing"),
    _choice({"passage": "The Eiffel Tower is located in Paris, the capital of "
             "France. It was completed in 1889."},
            "context", "In which city is the Eiffel Tower?",
            {"A": "London", "B": "Paris", "C": "Berlin"}, "B"),
    _choice({"passage": "Water boils at 100 degrees Celsius at sea level and "
             "freezes at 0 degrees Celsius."},
            "context", "At what temperature does water boil at sea level?",
            {"A": "0 degrees", "B": "50 degrees", "C": "100 degrees"}, "C"),
    _choice({"passage": "Maria is a software engineer at a fintech company. She "
             "manages the payments team and reviews invoices."},
            "context", "Which department does Maria most likely work in?",
            {"billing": "payments, refunds, invoices",
             "support": "product help and bugs",
             "sales": "new purchases and upgrades"}, "billing"),
    _choice({"passage": "The mitochondria is the powerhouse of the cell; it "
             "produces ATP through respiration."},
            "context", "Which organelle produces ATP?",
            {"A": "nucleus", "B": "mitochondria", "C": "ribosome"}, "B"),
    _choice({"passage": "Le Louvre est un musée situé à Paris. Il abrite la "
             "Joconde."},
            "context", "Où se trouve le Louvre ?",
            {"A": "Lyon", "B": "Paris", "C": "Marseille"}, "B"),
    _choice({"passage": "El Amazonas es el río más caudaloso del mundo y "
             "atraviesa Brasil."},
            "context", "¿Qué río atraviesa Brasil?",
            {"A": "Nilo", "B": "Amazonas", "C": "Misisipi"}, "B"),
    _choice({"ticket": "Customer cannot log in; the two-factor authentication "
             "code never arrives."},
            "context", "Which team should handle this ticket?",
            {"billing": "payments, refunds, invoices",
             "support": "product help and bugs",
             "sales": "new purchases and upgrades"}, "support"),
]


def softmax(logits: np.ndarray) -> np.ndarray:
    shifted = logits - logits.max()
    exps = np.exp(shifted)
    return exps / exps.sum()


def js_divergence(p: np.ndarray, q: np.ndarray) -> float:
    m = 0.5 * (p + q)
    p = np.maximum(p, 1e-12)
    q = np.maximum(q, 1e-12)
    return float(0.5 * np.sum(p * np.log(p / m)) + 0.5 * np.sum(q * np.log(q / m)))


def make_batch(tok, cfg, samples):
    items = []
    correct = []
    groups = []
    variant = os.environ.get("LAYANOW_A5_VARIANT")
    for sample in samples:
        question = sample["q"]
        keys = list(question["crit"])
        state = sample["state"]
        if variant == "state_question" and not state:
            state = question["ins"]
        ids, markers = build_sequence(
            tok, state, question, cfg["max_len"], cfg["head_max_len"]
        )
        assert len(markers) == len(keys), (len(markers), len(keys))
        items.append(
            {
                "ids": ids,
                "markers": markers,
                "qtype": QTYPES[question["t"]],
                "target": [0.0] * len(keys),
                "label": -1,
                "episode": 0,
                "ep_step": 0,
                "ep_len": 1,
            }
        )
        correct.append(keys.index(sample["correct"]))
        groups.append(sample["group"])
    return collate_items([items], 0), np.array(correct), groups


def session_for(path: str) -> ort.InferenceSession:
    options = ort.SessionOptions()
    options.intra_op_num_threads = INTRA_THREADS
    options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    return ort.InferenceSession(
        path, sess_options=options, providers=["CPUExecutionProvider"]
    )


def run(session: ort.InferenceSession, batch) -> tuple[np.ndarray, float]:
    feeds = {
        "input_ids": batch["input_ids"].numpy().astype(np.int64),
        "attention_mask": batch["attention_mask"].numpy().astype(np.int64),
        "marker_pos": batch["marker_pos"].numpy().astype(np.int64),
        "marker_mask": batch["marker_mask"].numpy().astype(np.bool_),
        "qtype": batch["qtype"].numpy().astype(np.int64),
    }
    start = time.perf_counter()
    logits = session.run(["logits"], feeds)[0]
    return logits, time.perf_counter() - start


def main() -> int:
    onnx_dir = os.path.abspath(sys.argv[2])
    fp32_name = os.environ.get("LAYANOW_FP32", "laya.onnx")
    int8_name = sys.argv[3] if len(sys.argv) > 3 else "laya_int8.onnx"
    with open(os.path.join(onnx_dir, "laya_config.json")) as handle:
        cfg = json.load(handle)
    tok = AutoTokenizer.from_pretrained(os.path.join(onnx_dir, "tokenizer"))
    batch, correct, groups = make_batch(tok, cfg, SAMPLES)
    valid = batch["marker_mask"].numpy().sum(axis=1)

    fp32 = session_for(os.path.join(onnx_dir, fp32_name))
    int8 = session_for(
        int8_name if os.path.isabs(int8_name) else os.path.join(onnx_dir, int8_name)
    )
    fp32_logits, fp32_ms = run(fp32, batch)
    int8_logits, int8_ms = run(int8, batch)

    stats: dict[str, list[int]] = {}
    js_total = 0.0
    max_dp = 0.0
    for row, k in enumerate(valid):
        k = int(k)
        p = softmax(fp32_logits[row, :k])
        q = softmax(int8_logits[row, :k])
        fp32_ok = int(p.argmax() == correct[row])
        agree = int(p.argmax() == q.argmax())
        js_total += js_divergence(p, q)
        max_dp = max(max_dp, float(np.abs(p - q).max()))
        if os.environ.get("LAYANOW_SHOW_PROBS"):
            print(f"sample {row:2d} [{groups[row]:7s}] fp32={np.round(p, 4).tolist()}")
        for group in (groups[row], "ALL"):
            bucket = stats.setdefault(group, [0, 0, 0])
            bucket[0] += fp32_ok
            bucket[1] += agree
            bucket[2] += 1

    for group in ("trivia", "context", "ALL"):
        if group not in stats:
            continue
        fp32_ok, agree, count = stats[group]
        print(
            f"{group:8s} n={count:2d}  fp32 acc={fp32_ok / count:.3f}  "
            f"int8 agreement={agree / count:.3f}"
        )
    print(f"mean JS divergence: {js_total / len(valid):.6f}")
    print(f"max |p-q|:          {max_dp:.6f}")
    print(f"fp32 latency:       {fp32_ms * 1000:.1f} ms")
    print(f"int8 latency:       {int8_ms * 1000:.1f} ms")

    overall = stats["ALL"]
    if overall[1] / overall[2] < 0.99:
        print("FAIL: int8 top-1 agreement below ADR-24 threshold (0.99)")
        return 1
    print("PASS: int8 matches fp32 within the ADR-24 tolerance")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
