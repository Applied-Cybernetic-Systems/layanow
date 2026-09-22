#!/usr/bin/env python3
"""Build-time generator for the rendering/calibration golden fixtures (M2).

The Rust port of Laya's `rl_common.py` (`build_sequence` / `render_options`,
plus temperature scaling and Jev confidence) must match the reference exactly.
This script runs the *actual* `rl_common.py` against a checkpoint's real
tokenizer and writes a small JSON fixture that `crates/layanow-model/tests/
render_golden.rs` replays. Python is build-time only (ADR-9); the fixture is
committed so the Rust test needs neither Python nor the weights.

Usage (inside `nix develop`, with the M1 export or a cached bundle present):

    tools/golden/gen_render_fixtures.py \
        --tokenizer target/m1-export/onnx-multilingual/tokenizer \
        --rl-common target/m1-export/multilingual/rl_common.py \
        --out crates/layanow-model/tests/fixtures/render_golden.json

The tokenizer directory must contain `tokenizer.json` + `tokenizer_config.json`.
`rl_common.py` is fetched by `tools/export/run.sh` (from the model repo).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Dict, List


def load_rl_common(path: str):
    """Import `build_sequence`, `render_options`, `confidence_from_probs`."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(path)))
    import rl_common  # type: ignore  # noqa: PLC0415

    return rl_common


class RecordingTokenizer:
    """Wrap a HF fast tokenizer and remember every `text -> input_ids` call.

    `build_sequence` tokenizes the head, each option (with a leading space), and
    the serialized state. Recording the exact strings pins both the text the port
    constructs and the assembled ids/markers.
    """

    def __init__(self, tokenizer):
        self._tok = tokenizer
        self.calls: Dict[str, List[int]] = {}

    @property
    def mask_token(self):
        return self._tok.mask_token

    @property
    def mask_token_id(self):
        return self._tok.mask_token_id

    @property
    def cls_token_id(self):
        return self._tok.cls_token_id

    @property
    def sep_token_id(self):
        return self._tok.sep_token_id

    def __call__(self, text, add_special_tokens=True):
        ids = self._tok(text, add_special_tokens=add_special_tokens)["input_ids"]
        self.calls.setdefault(text, list(ids))
        return {"input_ids": list(ids)}


def choice_question(instructions: str, answers: List[str]) -> Dict:
    labels = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    crit = {}
    for index, answer in enumerate(answers):
        label = labels[index] if index < len(labels) else str(index + 1)
        crit[label] = answer
    return {"t": "choice", "ins": instructions, "crit": crit}


def render_case(rl_common, tokenizer, name, instructions, answers, state, max_len, head_max_len):
    q = choice_question(instructions, answers)
    options = rl_common.render_options(q)
    recorder = RecordingTokenizer(tokenizer)
    ids, markers = rl_common.build_sequence(
        recorder, state, q, max_len, head_max_len
    )
    return {
        "name": name,
        "max_len": max_len,
        "head_max_len": head_max_len,
        "state": state,
        "instructions": instructions,
        "criteria": [[label, text] for label, text in q["crit"].items()],
        "options": options,
        "tokens": recorder.calls,
        "expected_ids": ids,
        "expected_markers": markers,
    }


def softmax(values: List[float]) -> List[float]:
    top = max(values)
    exps = [math.exp(value - top) for value in values]
    total = sum(exps)
    return [value / total for value in exps]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tokenizer",
        default="target/m1-export/onnx-multilingual/tokenizer",
        help="directory with tokenizer.json + tokenizer_config.json",
    )
    parser.add_argument(
        "--rl-common",
        default="target/m1-export/multilingual/rl_common.py",
        help="path to the reference rl_common.py",
    )
    parser.add_argument(
        "--out",
        default="crates/layanow-model/tests/fixtures/render_golden.json",
        help="fixture output path",
    )
    parser.add_argument("--checkpoint", default="laya-multilingual")
    args = parser.parse_args()

    if not os.path.isfile(os.path.join(args.tokenizer, "tokenizer.json")):
        parser.error(f"no tokenizer.json under {args.tokenizer}")
    if not os.path.isfile(args.rl_common):
        parser.error(f"no rl_common.py at {args.rl_common}")

    rl_common = load_rl_common(args.rl_common)
    from transformers import PreTrainedTokenizerFast  # noqa: PLC0415

    tokenizer = PreTrainedTokenizerFast.from_pretrained(args.tokenizer)

    max_len, head_max_len = 1024, 256
    cases = [
        render_case(
            rl_common,
            tokenizer,
            "three_options_empty_state",
            "Which planet is known as the Red Planet?",
            ["Venus", "Mars", "Jupiter"],
            "{}",
            max_len,
            head_max_len,
        ),
        render_case(
            rl_common,
            tokenizer,
            "single_option",
            "Is the sky blue?",
            ["yes"],
            "{}",
            max_len,
            head_max_len,
        ),
        render_case(
            rl_common,
            tokenizer,
            "six_options_with_context",
            "Which of these is a prime number?",
            ["4", "6", "7", "9", "10", "12"],
            '{"passage": "A prime number has exactly two divisors."}',
            max_len,
            head_max_len,
        ),
        # Long options exercise the per-option `[:48]` cap and the even-shrink path.
        render_case(
            rl_common,
            tokenizer,
            "long_options_shrink",
            "Pick the best description.",
            [
                " ".join(["very"] * 80),
                " ".join(["quite"] * 80),
                " ".join(["rather"] * 80),
            ],
            "{}",
            128,
            64,
        ),
        # A long state exercises the `max_len` state truncation.
        render_case(
            rl_common,
            tokenizer,
            "long_state_truncated",
            "What colour is the ball?",
            ["red", "green", "blue"],
            "context " * 400,
            96,
            64,
        ),
        # A literal mask token in user text must be scrubbed, not turned into a marker.
        render_case(
            rl_common,
            tokenizer,
            "mask_token_scrubbed",
            "Explain [MASK] please",
            ["first [MASK] option", "second option"],
            "state [MASK] here",
            max_len,
            head_max_len,
        ),
    ]

    calibration = {
        "temperature_cases": [
            {
                "name": "choice_flat",
                "temperature": 1.0,
                "logits": [2.0, 1.0, 0.0],
                "expected_probs": softmax([2.0, 1.0, 0.0]),
            },
            {
                "name": "choice_hot",
                "temperature": 1.76,
                "logits": [2.0, 1.0, 0.0],
                "expected_probs": softmax([2.0 / 1.76, 1.0 / 1.76, 0.0]),
            },
            {
                "name": "choice_cold",
                "temperature": 0.5,
                "logits": [-1.0, 3.0, 0.0, 2.0],
                "expected_probs": softmax([-1.0 / 0.5, 3.0 / 0.5, 0.0, 2.0 / 0.5]),
            },
        ],
        "confidence_cases": [
            {"name": "uniform", "probs": [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]},
            {"name": "peaked", "probs": [0.9, 0.05, 0.05]},
            {"name": "binary", "probs": [1.0, 0.0]},
            {"name": "single", "probs": [1.0]},
        ],
        "bucket_cases": [
            {"qtype": "choice", "options": 2, "expected": "choice:2"},
            {"qtype": "choice", "options": 3, "expected": "choice:3-5"},
            {"qtype": "choice", "options": 6, "expected": "choice:6-10"},
            {"qtype": "choice", "options": 11, "expected": "choice:11+"},
            {"qtype": "score", "options": 2, "expected": "score:2"},
            {"qtype": "noul", "options": 2, "expected": "noul:2"},
        ],
    }
    for entry in calibration["confidence_cases"]:
        entry["expected"] = rl_common.confidence_from_probs(
            __import__("numpy").array(entry["probs"]), len(entry["probs"])
        )

    fixture = {
        "generator": "tools/golden/gen_render_fixtures.py",
        "source": "rl_common.py build_sequence/render_options/confidence_from_probs",
        "checkpoint": args.checkpoint,
        "special": {
            "cls": tokenizer.cls_token_id,
            "sep": tokenizer.sep_token_id,
            "mask": tokenizer.mask_token_id,
            "pad": tokenizer.pad_token_id,
            "mask_tok": tokenizer.mask_token,
        },
        "cases": cases,
        "calibration": calibration,
    }

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"wrote {args.out}: {len(cases)} render cases")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
