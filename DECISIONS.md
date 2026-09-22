# Decisions (ADR log)

Short records of the choices that shape the project.

## ADR-1 — Applet, not daemon
**Decision:** A single resident tray applet owns the model, overlay and results.
No separate daemon.
**Why:** The applet must already be resident (tray + hotkey). Keeping the model
in-process removes an IPC protocol and a process to supervise. Inference runs on
a dedicated worker thread so the UI never blocks.
**Consequence:** Restarting the applet reloads the model (seconds). Acceptable.

## ADR-2 — Rust, not Python or Go
**Decision:** Implement in Rust.
**Why:**
- The model runs via **ONNX Runtime**, so no Python is required at runtime
  (ADR-9). This removed the original "Laya is Python-only" blocker.
- Rust has the strongest ecosystem for this exact app: `ort` (ONNX Runtime),
  `tokenizers` (the official HuggingFace Rust tokenizer Laya uses), `atspi` /
  `uiautomation` / `axuielement-rs` (a11y), `tray-icon` / `ksni` (tray),
  `global-hotkey`, `egui` (overlay), `smithay-client-toolkit` (Wayland).
- Go is near-tied on resource usage and easier to cross-compile, but its
  overlay/a11y ecosystem is thinner and would need more hand-rolling.
- Python + Qt is the easiest to write but the heaviest at idle and reintroduces
  the Python dependency.
- Rust's compiler + clippy give an **objective quality gate** (see `PLAN.md`,
  "Rust quality gate"), which matters when agents write the code.

## ADR-3 — Native text, no OCR (v1)
**Decision:** Only handle text that is selectable or exposed via the
accessibility tree. No screenshots, no OCR.
**Consequence:** A future `ocr` resolver is reserved (see `RESOLVERS.md`).

## ADR-4 — Drop the browser/CDP resolver for now
**Decision:** No CDP integration in v1. Deferred, not abandoned.
**Why:** The tool is site-agnostic; browsers are handled like any other app.

## ADR-5 — Never clobber the clipboard
**Decision:** The tool must not overwrite the user's clipboard.
**How:** Prefer the accessibility resolver (reads selection without copying);
on Linux prefer the PRIMARY selection (`wl-paste -p` / `wl-clipboard-rs`). If a
simulated copy is ever unavoidable, snapshot and restore the clipboard; `wtype`
copy is opt-in.

## ADR-6 — Hotkey ownership
**Decision:** Register the hotkey natively where the OS allows; on Wayland use a
compositor bind.
**Why:** Wayland has no app-level global-hotkey protocol, and the
GlobalShortcuts portal is not implemented by wlroots/`xdpw` (MangoWC).
**How:** Linux/Wayland → a `mango` bind runs `layassist toggle`. Windows/macOS
(later) → the applet registers via `global-hotkey`.

## ADR-7 — Model selection is a settings toggle
**Decision:** Default to one checkpoint; expose checkpoint choice in settings.
**Why:** The right checkpoint depends on language/task. Auto-routing (porting
Laya's `Router`) is optional future work.

## ADR-8 — Single-answer only (v1)
**Decision:** `choice` decisions with exactly one correct answer.
**Consequence:** Multi-answer (threshold over probabilities) is deferred; the
data model stores a list of answers so it can be added.

## ADR-9 — ONNX Runtime, not PyTorch / the `laya` package
**Decision:** Run Laya via ONNX Runtime in Rust (`ort`), using ONNX exports of
the checkpoints. Python is used only at build time to export/quantize.
**Why:** Removes the runtime Python dependency and is far lighter than torch.
The export format is documented and there is a reference implementation
(`receptron/laya`, MIT). See `LAYA.md`.
**Consequence:** We own the port of Laya's input rendering, decision head, and
temperature calibration, and must verify parity against the PyTorch reference.

## ADR-10 — egui for the UI
**Decision:** `egui`/`eframe` for the overlay and settings.
**Why:** Fully cross-platform (winit + wgpu/glow); more mature and lighter for a
custom transparent always-on-top overlay than `iced`; immediate-mode drawing maps
well to the drag-to-mark UX; idle CPU is near zero when not repainting.

## ADR-11 — Resource policy
**Decision:** Default profile = **hot** (model resident), **int8**, **CPU-only**,
with a documented soft budget of **≤ 3 GB** RSS. On-demand unload is a setting.
**Why:** Hot gives instant answers; int8 keeps the resident footprint low
(~0.5–0.8 GB including the shell) and well under the budget. CPU-only avoids a
CUDA context and VRAM. No runtime enforcement — documented default only.
**Consequence:** ORT arena/thread settings are tuned in config; GPU is opt-in.

## ADR-12 — No SkillsBuild coupling
**Decision:** Do not special-case IBM SkillsBuild or bundle the old answer bank.
**Why:** The target is now a general utility; the MISSION.md bank is not an
evaluation set. Any accuracy evaluation uses a neutral, self-made sample.

## ADR-13 — v1 text source: the native selection buffer only
**Decision:** In v1 the only text source is the operating system's **selection
buffer** (PRIMARY on Linux; clipboard via an explicit copy on Windows/macOS).
No accessibility-tree resolver and no OCR in v1.
**Clarification:** the "3 resolvers" are therefore the **three platform
backends** (Linux, Windows, macOS) that read the selection — not three
different text sources.
**Why:** Native selection works wherever text is selectable, is portable, and
needs no accessibility setup.
**Consequence:** the accessibility resolver and OCR remain future work; AT-SPI
is not required for v1. See `RESOLVERS.md`.

## ADR-14 — Click-through overlay + native selection
**Decision:** The overlay never captures the pointer. The user selects text
natively in the target application; the overlay is a visual layer only.
**Why:** Keeps the tool app-agnostic and avoids re-implementing text selection.
**Consequence:** the mechanism that captures a selection into the item list
(poll the selection buffer vs a "commit" hotkey) and how `Enter` is delivered
must be defined — tracked in `OPEN-QUESTIONS.md`.

## ADR-15 — Freeze/dim overlay; results in the same overlay; click to dismiss
**Decision:** The overlay dims ("freezes") the screen; results render in the same
overlay; a click dismisses the results and returns to the live screen.
**Open conflict:** a *visual dim* is compatible with click-through selection, but
a *true screenshot freeze* is not (it would stop the app receiving the mouse).
Resolve in `OPEN-QUESTIONS.md` (A3b).

## ADR-16 — Default checkpoint: English int8, multilingual via settings
**Decision:** Ship the English checkpoint as the default, int8; expose the
multilingual checkpoint (and others) through settings (ADR-7).

## ADR-17 — Model acquisition: download on first run
**Decision:** Do not bundle weights. Download the selected checkpoint from
Hugging Face on first use into an XDG cache
(`~/.cache/layassist/models/`), verify checksums, and show the Apache-2.0
attribution. Allow a user-provided path override in settings.
**Why:** Best practice for large models; keeps the app small and updatable.

## ADR-18 — Failure UX: applet popup error
**Decision:** When no text can be resolved (or the model fails), show an error
popup from the applet rather than failing silently.

## ADR-19 — Hotkey is user-managed
**Decision:** The applet documents the hotkey and the `layassist toggle`
command, but does **not** modify the compositor config. The user wires the bind
(e.g. in `~/projects/nix` mango config).

## ADR-20 — Tray `tray-icon`; overlay windowing is per-platform
**Decision:** Use `tray-icon` (cross-platform: Windows/macOS/Linux SNI). The
overlay windowing is `cfg`-gated: Wayland layer-shell on Linux, native
always-on-top windows on Windows/macOS. The renderer (`wgpu`/`glow`) is uniform.
**Why:** `tray-icon` is the only mature cross-platform option; layer-shell is
Wayland-only, so a single overlay implementation cannot be cross-platform.

## ADR-21 — Support Wayland and X11
**Decision:** Linux support covers both Wayland and X11.
**Why:** Broadens applicability; X11 also has simpler global-hotkey/selection
APIs.

## ADR-22 — Repository
**Decision:** Source lives at `github.com/Uiyx/layassist` (private), with a local
git repo at `~/projects/layassist`.

## ADR-23 — MCQ → Laya rendering (A5)
**Decision:** Map an MCQ as `ins` = question text, `crit` = letter-labelled
answers (`{"A": ans0, "B": ans1, …}`), `state` = empty `{}` by default (or the
question / any extra context the user selected).
**Why:** Closest to Laya's Jev-style training distribution (state = document,
`ins` = question, `crit` = labelled options). Keep rendering behind config and
confirm by evaluation in M1 — do not hardcode beyond the default.

## ADR-24 — Quantization: dynamic int8 (A7)
**Decision:** Default to **dynamic int8**
(`onnxruntime.quantization.quantize_dynamic`). Keep the fp32 graph as a settings
option and as the fallback if int8 regresses.
**Acceptance:** int8 vs fp32 top-1 agreement ≥ 99% and small JS/KL divergence on
a sample of real inputs; if a labelled sample exists, top-1 accuracy within
~1–2 points of fp32.

## ADR-25 — No freeze (A3b)
**Decision:** The overlay is a transparent/dim **click-through** layer over live
content. No screenshot freeze.
**Why:** A true freeze would prevent the native text selection the capture model
depends on (ADR-14).

## ADR-26 — Capture model: auto-capture + keyboard interactivity (A14)
**Decision:** Auto-capture every **selection-buffer change** (`watch()`); each new
selection becomes an item (first = question, rest = answers). The overlay holds
**keyboard interactivity** (Wayland layer-shell keyboard mode) so `Enter`
(decide) and `Esc` (cancel) are delivered while the pointer stays click-through.
**Why:** Removes a per-item hotkey press and keeps the pointer usable in the
target app.

## ADR-27 — Accepted defaults for C1–C7
- **C1:** manual checkpoint toggle in v1; no auto language router.
- **C2:** confidence threshold default `0.5`; flag low-confidence, never refuse.
- **C3:** ignore the `act_probs` head in v1.
- **C4:** cap `intra_op_num_threads` to 2–4; tune in M1.
- **C5:** CPU-only now; GPU is a future opt-in.
- **C6:** multi-answer deferred; later, mark options above a probability
  threshold.
- **C7:** the M1 spike includes exporting `multilingual` to ONNX + int8 and
  verifying parity/accuracy.

## ADR-28 — M1 results: multilingual ONNX + quantization (A5/A7)
**Decision:** Ship the multilingual checkpoint (`convaiinnovations/laya` →
`multilingual/`, encoder `jhu-clsp/mmBERT-base`) as **fp32 ONNX**; dynamic int8
stays an opt-in setting. The v1 graph is **logits-only** (the unused `act_probs`
head is pruned, ADR-27 C3), and special tokens are read from the checkpoint's
`tokenizer_config.json` instead of being hardcoded (ModernBERT spells them
`[CLS]`/`[MASK]`, mmBERT `<bos>`/`<mask>`).
**Evidence** (20 neutral, self-made MCQs; `tools/export/run.sh`):
- fp32 ↔ PyTorch parity: `max |dlogits| = 8.6e-6`.
- dynamic int8 vs fp32 top-1 agreement: **100% with context in `state`, 50%
  state-less, 70% overall** — below ADR-24's ≥ 99%. `per_channel=True` was far
  worse (27%); MatMul-only 73%.
- peak RSS (ORT session, debug process): fp32 ≈ 2.1 GB, int8 ≈ 0.7 GB. Both are
  within ADR-11's ≤ 3 GB budget.
- A5: `state = {}` default confirmed — passage in `state` 8/8, empty 5/12,
  question duplicated into `state` 1/12.
**Why:** int8 is the resource win but misses the accuracy bar under the default
(empty-state) rendering; ADR-24 already allows the fp32 fallback.
**Consequence:** multilingual ships fp32 (still within budget); int8 is offered
for context-heavy inputs. English keeps its published int8 default (not
re-checked here). Revisit int8 if static/calibrated quantization is added.
Weights are Convai Innovations' (Apache-2.0); see `bundle::ATTRIBUTION`.

## ADR-29 — Golden rendering/calibration fixtures are committed (E3)
**Decision:** Commit a small JSON golden fixture
(`crates/layassist-model/tests/fixtures/render_golden.json`) generated once at
build time by `tools/golden/gen_render_fixtures.py`. The generator runs the
reference `rl_common.build_sequence` / `render_options` /
`confidence_from_probs` against a checkpoint's real tokenizer and records, per
case, the exact tokenizer inputs, the assembled `input_ids`, and the `[MASK]`
`marker_pos`. The Rust test (`tests/render_golden.rs`) replays the fixture
offline: a recorded-call closure stands in for the tokenizer, so neither the
weights nor `tokenizer.json` ship and CI stays hermetic.
**Why:** The risk register calls for golden tests against `rl_common.py`, but
ADR-9 forbids Python at runtime and the bundle is far too large to commit. A
generated fixture pins the reference behaviour in a few tens of KiB.
**Consequence:** An intentional rendering/calibration change requires
regenerating and reviewing the fixture; an accidental divergence fails
`cargo test`. The generator is a dev-shell tool only (it needs the M1 export or
a cached tokenizer), never a runtime dependency.
