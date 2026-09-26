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
GlobalShortcuts portal is not implemented by wlroots/`xdpw`.
**How:** Linux/Wayland → a compositor bind runs `layanow toggle`. Windows/macOS
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

> **Amended by ADR-36:** the default quantization is **fp32**; dynamic int8 is an
> opt-in setting.

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

> **Superseded by ADR-36:** the English default is **fp32**; int8 is opt-in.

## ADR-17 — Model acquisition: download on first run
**Decision:** Do not bundle weights. Download the selected checkpoint from
Hugging Face on first use into an XDG cache
(`~/.cache/layanow/models/`), verify checksums, and show the Apache-2.0
attribution. Allow a user-provided path override in settings.
**Why:** Best practice for large models; keeps the app small and updatable.

## ADR-18 — Failure UX: applet popup error
**Decision:** When no text can be resolved (or the model fails), show an error
popup from the applet rather than failing silently.

## ADR-19 — Hotkey is user-managed
**Decision:** The applet documents the hotkey and the `layanow toggle`
command, but does **not** modify the compositor config. The user wires the bind
(e.g. in the compositor config).

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
**Decision:** Source lives at `github.com/Applied-Cybernetic-Systems/layanow`
(**public**). It was transferred to the Applied Cybernetic Systems organization
on 2026-09-22 and made public on 2026-09-23.

## ADR-23 — MCQ → Laya rendering (A5)
**Decision:** Map an MCQ as `ins` = question text, `crit` = letter-labelled
answers (`{"A": ans0, "B": ans1, …}`), `state` = empty `{}` by default (or the
question / any extra context the user selected).
**Why:** Closest to Laya's Jev-style training distribution (state = document,
`ins` = question, `crit` = labelled options). Keep rendering behind config and
confirm by evaluation — do not hardcode beyond the default.

## ADR-24 — Quantization: dynamic int8 (A7)
**Decision:** Default to **dynamic int8**
(`onnxruntime.quantization.quantize_dynamic`). Keep the fp32 graph as a settings
option and as the fallback if int8 regresses.
**Acceptance:** int8 vs fp32 top-1 agreement ≥ 99% and small JS/KL divergence on
a sample of real inputs; if a labelled sample exists, top-1 accuracy within
~1–2 points of fp32.

> **Amended by ADR-28/ADR-36:** int8 is **not** the default for either
> checkpoint; it is an opt-in setting. The acceptance bar still governs whether
> it may be offered.

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
- **C4:** cap `intra_op_num_threads` to 2–4; tune empirically.
- **C5:** CPU-only now; GPU is a future opt-in.
- **C6:** multi-answer deferred; later, mark options above a probability
  threshold.
- **C7:** the export spike includes exporting `multilingual` to ONNX + int8 and
  verifying parity/accuracy.

## ADR-28 — Export-spike results: multilingual ONNX + quantization (A5/A7)
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
for context-heavy inputs. English also ships fp32 (ADR-36; not re-checked
here). Revisit int8 if static/calibrated quantization is added.
Weights are Convai Innovations' (Apache-2.0); see `bundle::ATTRIBUTION`.

## ADR-29 — Golden rendering/calibration fixtures are committed (E3)
**Decision:** Commit a small JSON golden fixture
(`crates/layanow-model/tests/fixtures/render_golden.json`) generated once at
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
`cargo test`. The generator is a dev-shell tool only (it needs a local export or
a cached tokenizer), never a runtime dependency.

## ADR-30 — Overlay: eframe/glow, mouse passthrough, manual-capture stub
**Decision:** The overlay is an `eframe`/`egui` window (ADR-10) using the
`glow` (OpenGL) renderer, pinned to `eframe` 0.32 — the newest release that
still honours the workspace MSRV of 1.85 (E8); 0.36 requires Rust 1.95. The
window is transparent, undecorated, always-on-top, fullscreen, and
`MousePassthrough` is **on while capturing** (ADR-14) and **off only while
results are shown**, so the dismissing click can be seen (ADR-15). Until the
platform selection resolver lands, a single-line text field auto-focuses
and `Tab` commits its contents through `StubResolver` — the same path a real
auto-captured selection takes — so the capture/decide/results flow is testable
without the OS selection buffer. `Enter` decides, `Esc` cancels, a click
dismisses.
**Why:** This delivers the item model, results panel, and inference wiring
while respecting the click-through invariant. `glow` avoids the `wgpu` tree.
**Consequence / open:** true Wayland layer-shell *keyboard interactivity* and
click-through (ADR-26) are not yet wired — eframe's winit window relies on X11
focus for keys — so the overlay is compile- and unit-tested but not yet
visually verified; that, and removing the `Tab`-capture stub, is follow-up work. The
"take the pointer to dismiss results" interpretation of ADR-15 is provisional.

## ADR-31 — Wayland overlay is a wlr-layer-shell host (supersedes ADR-30's window)
**Decision:** Replace the eframe window with a purpose-built Wayland host in
`layanow-platform` (`overlay::run`). It creates a `wlr-layer-shell` surface
(`Layer::Overlay`, anchored to every edge, `exclusive_zone = -1`,
`KeyboardInteractivity::Exclusive`), renders egui through `egui_glow` on an
EGL/GLES context bound to that surface (`glutin`, with the raw `wl_display` /
`wl_surface` from `smithay-client-toolkit`), and drives its own event loop
(`calloop`-free: `libc::poll` on the connection fd with a 16 ms timeout). The
pointer is **click-through** via an empty `wl_surface` input region (ADR-14);
while results are shown the input region is set to exactly the results-panel
rectangle, so only the panel is clickable and everything else stays
click-through (ADR-15). The translucent backdrop is painted
only behind the UI panel, not across the whole surface. The UI stays in `layanow-app` behind the `OverlayApp` trait.
`eframe` is dropped from the dependency tree.
**Why:** `winit`/`eframe` only create `xdg_toplevel` windows, which cannot be
always-on-top, click-through, *and* keyboard-interactive on Wayland; that
combination is exactly what `wlr-layer-shell` provides (ADR-20/26). All render
dependencies (`egui_glow`, `glutin`, `glow`, `smithay-client-toolkit`) were
already present transitively via eframe, so this is a re-pointing, not a new
stack. The `system` feature of `wayland-client` is required so EGL receives real
libwayland pointers (the pure-Rust backend exposes none); the dev shell adds
`wayland`, `libGL`, `mesa`, `libxkbcommon` to `LD_LIBRARY_PATH` so those
libraries load at runtime.
**Consequence:** Linux/Wayland only for now; `overlay::run` returns
`OverlayError::Unsupported` elsewhere, and the Wayland deps are `cfg`-gated to
Linux. Verified to map a 1920x1200 layer surface and initialise GL;
the `Tab`-to-capture stub still stands in until the selection backends.

## ADR-32 — Overlay theme: gruvbox medium contrast, shadowed bright text
**Decision:** The overlay uses the gruvbox "medium" contrast palette
(`bg0 = #282828`, `fg0 = #fbf1c7`), installed into egui's visuals by
`layanow-app::theme`. Primary text is `fg0` (the brightest foreground) and is
drawn with a soft drop shadow (black at ~78% opacity, offset 1.5 pt) so it stays
legible over arbitrary desktop content; question/answer accents are gruvbox
`blue`/`green`, warnings `yellow`, errors `red`. The probability bars run
gruvbox red → orange → green.
**Why:** The overlay floats over unpredictable content; a bright, shadowed
foreground and one coherent theme improve legibility. egui has no text-shadow
option, so `theme::shadowed_text` lays the galley out once and paints it twice
(shadow, then text).
**Consequence:** UI colours live in `theme`; `results::probability_colour`
mirrors the same gruvbox anchors and stays `egui`-free. Palette changes are a
one-file edit.

## ADR-33 — Capture: `Tab` commits the highlight or a typed item
**Decision:** The overlay captures items two ways, both committed with **`Tab`**:
(a) the current Wayland **PRIMARY** selection — the highlight buffer, never the
regular clipboard — read once through `layanow-platform::selection` (a
`TextResolver` over `wl-clipboard-rs`); and (b) the contents of the text field,
so typing an item remains available as a fallback. If the field is non-empty it
wins; otherwise the PRIMARY selection is used. The captured text is appended to
the session (first = question, rest = answers), and consecutive repeats of the
most recent item are ignored. `Enter` decides, `Esc` cancels (or quits when
nothing is captured). This revises ADR-26, which specified auto-capture on
every selection change; the keyboard-interactivity part of ADR-26 still holds.
**Why:** The windowless `ext-data-control` / `wlr-data-control` protocols used
to read PRIMARY expose no change notification, so `TextResolver::watch` returns
`None`; a manual commit is the simplest reliable capture. PRIMARY is the
highlight buffer and is separate from the regular clipboard, so this never
clobbers what the user copied (ADR-5), and resolving on demand avoids the
infinite "current selection" re-capture a drain loop would produce. Keeping the
typed field means capture still works when nothing is selectable (and keeps the
flow testable headlessly).
**Consequence:** The earlier `StubResolver` indirection is removed — the overlay
builds a `Selection` for typed text directly and uses the platform resolver for
highlights. The platform selection resolver is wired in `main.rs`. True
auto-capture (observing selection changes through the overlay surface's own
`wl_data_device`) remains future work, as do the X11/Windows/macOS backends. A
blocking pipe read happens on the UI thread when `Tab` is pressed; moving it to
a worker is deferred until a backend can stall.

## ADR-34 — Control channel: cross-platform local socket; hidden-by-default overlay
**Decision:** The resident applet is controlled through a small local-socket
protocol. `layanow` with no arguments runs the applet;
`layanow toggle|show|hide|quit` connect to it and send one newline-terminated
command. The transport is `interprocess`'s `local_socket`: a Unix domain socket
under `$XDG_RUNTIME_DIR` on Unix and a named pipe on Windows. The same socket is
the single-instance lock: the first applet binds it, a later applet
notices a live owner and exits, and a stale Unix socket file left by a crash is
reclaimed. The command type and parsing live in `layanow-platform::control`;
the listener runs on a dedicated thread and forwards commands over an `mpsc`
channel to the app.

The overlay now starts **hidden**: no buffer is attached and the layer
surface uses `KeyboardInteractivity::None` with an empty input region.
`OverlayApp` gains `poll()` (process non-frame events on every host tick, even
while hidden) and `visible()`. On show the host takes
`KeyboardInteractivity::Exclusive`; on hide it blanks the surface to transparent
and returns to `None` while keeping the surface mapped (unmapping would leave it
unconfigured and break the next show), so the keyboard is never grabbed
while the applet is hidden. `Esc`
now **hides** the overlay (revising ADR-33's "quits when nothing is captured");
quitting is `layanow quit` (the tray will add a direct control).

**Why:** D-Bus is Linux-only and would force a second Windows mechanism; TCP has
a firewall/port surface. A user-scoped UDS/named pipe is the OS-native local
control primitive, needs no heavyweight dependency, and collapses `toggle` and
single-instance onto one object. Starting hidden and gating keyboard
interactivity fixes the "can't type while the applet runs" behaviour without
giving up the click-through overlay (ADR-14).

**Consequence / open:** The command path is verified end-to-end (start →
single-instance → `quit`) and the Wayland hide/show mapping is compiled, but the
visual show/hide against a live compositor is not yet exercised. Process exit
does not run the control thread's destructor, so the applet unlinks the socket
on a clean exit (`control::cleanup`) and `bind` reclaims a stale file otherwise.
A decision that is in flight when the overlay is hidden has its late reply
dropped so stale results cannot reappear; each decision now carries a
monotonic id and the overlay accepts only the reply matching the in-flight id
(superseding the earlier phase-only guard). The tray and settings
attach to the same command stream.

## ADR-35 — Tray icon uses `tray-icon`'s `ksni` backend on Linux
**Decision:** The system tray is `tray-icon` (ADR-20). On Linux it is built
with `default-features = false, features = ["ksni"]`, so it speaks
StatusNotifierItem over D-Bus through `ksni`; Windows and macOS use
`tray-icon`'s native backend. The menu is **Toggle overlay** and **Quit**, and a
left click toggles; both post to the same `Command` channel as the CLI
(ADR-34). The `layanow.png` icon (32x32 RGBA) is embedded and decoded with the
`png` crate. Tray creation is best-effort: a failure is logged and the applet
keeps running from the CLI.

**Why:** The overlay host is a raw Wayland layer-shell client with no GTK main
loop; `tray-icon`'s default Linux backend needs `libappindicator` + GTK and a
GTK event loop, which would add a heavy dependency and a second loop. The
`ksni` backend runs on its own thread and needs no GTK (verified: the resolved
tree has no `gtk`/`libappindicator`). A StatusNotifier host provides the watcher.

**Consequence:** `cargo-deny`'s `[graph] all-features` is turned off because it
would enable both mutually-exclusive Linux backends and pull in the unused GTK
tree (`option-ext`/MPL-2.0, the unmaintained `proc-macro-error`).
`PredefinedMenuItem::quit` renders disabled under the `ksni` bridge, so Quit is
an ordinary menu item. The `Settings...` entry is deferred, and the
menu does not yet reflect overlay visibility.

## ADR-36 — English default is fp32; int8 is an opt-in setting
**Decision:** The default English checkpoint is the shipped **fp32** graph
(`receptron/laya-onnx`, `Quant::Fp32`) — which is what `main.rs` already loads.
Dynamic int8 is an **opt-in setting**, not a default; it lands with the settings
menu and is gated on confirming it meets ADR-24's accuracy
bar. This supersedes ADR-16's "English int8" default and amends the int8 default
in ADR-11 and ADR-24 for English (multilingual was already fp32 per ADR-28).
**Why:** fp32 is strictly the more accurate graph — quantization only adds error.
int8 is the resource win, but fp32 already fits ADR-11's ≤ 3 GB soft
budget (≈ 2.1 GB vs ≈ 0.7 GB), so there is no budget pressure to accept an
unverified accuracy loss. The multilingual export-spike result (int8 vs fp32 top-1
agreement 70% overall) shows int8 can change decisions; the English case has not
been measured.
**Consequence:** `AGENTS.md`/`PLAN.md`/`OPEN-QUESTIONS.md`/`LAYA.md` no longer
describe an English int8 default. `Quant` stays metadata-only until it is wired
to select the graph file.

## ADR-37 — Checkpoint registry; multilingual ONNX is a community export
**Decision:** Checkpoints are described by a `CheckpointSpec` registry
(`layanow_model::bundle`), each recording its Hugging Face repo and bundle layout
(graph file, optional external weights, calibration config, tokenizer
 directory). The registry ships two entries: `english` (default,
`receptron/laya-onnx`, fp32) and `multilingual`
(`soyelmismo/laya-multilingual-onnx`, fp32 `model-fp32.onnx`). We do **not**
point `DEFAULT_REPO` at the canonical `convaiinnovations/laya` hub: that repo
publishes the *source* checkpoints as safetensors, not ONNX, so `ort` cannot
load it directly.
**Why:** The hub is canonical but contains no ONNX artifacts for any
checkpoint; switching would require us to export and host ~0.7–1.7 GB graphs
ourselves. The community multilingual export matches our graph contract (same
`input_ids`/`attention_mask`/`marker_pos`/`marker_mask`/`qtype` inputs and an
fp32 `logits` output) and was verified end-to-end: it downloads and verifies
against the Hugging Face manifest, loads through `ort`, and ranks a 3-option
`choice` correctly. The alternatives were rejected: `mizchi/laya-multilingual-onnx`
is fp16 weights (≈3900 ms/decision on CPU software emulation) and
`sevenreasons/laya-onnx-fp16` is an English fp16 export.
**Consequence:** English fp32 stays the default (ADR-36); multilingual is a
settings toggle. A replaced or corrupted community export is caught by
the manifest digest check (ADR-17). The layout is data, so a better export can
replace it without code changes. int8 stays opt-in and gated on an accuracy
check.

## ADR-38 — Settings is a separate `layanow settings` process
**Decision:** The settings UI (checkpoint, confidence threshold, unload policy;
) is a standalone process, opened by `layanow settings` or
spawned by the tray's **Settings…** item. It is a normal `xdg_toplevel`
rendered with `eframe`/`glow`, with its own event loop. It edits
`~/.config/layanow/config.toml` and then sends `Command::Reload` to the resident
applet, which re-reads the file and applies the change; the file is the single
source of truth, so no bidirectional IPC is needed. A named control channel
(`layanow-settings`) keeps it to one window.
**Why:** The overlay is a raw `wlr-layer-shell` client with a hand-rolled event
loop (ADR-31); a second event loop cannot run inside it portably, and winit's
event loop has thread-affinity limits. A separate process sidesteps both, and
because it uses portable eframe it will work on Windows/macOS before their
overlay hosts exist. Keeping the config file authoritative avoids a get/set
protocol.
**Consequence:** `eframe`/`glow` returns to the dependency tree for the settings
window only (the overlay keeps the layer-shell host); no `wgpu`. Settings are
applied live via `Command::Reload`: the confidence threshold changes
immediately, and a checkpoint change rebuilds the model on the worker thread
(never the overlay loop) through the factory, with the
on-demand policy dropping it after each decision. The worker's eager load moved
off the startup path, so the applet now starts before the model is ready (a load
failure surfaces on the first decision / in the log rather than at launch).

## ADR-39 — Per-checkpoint precision variants (fp32 / fp16 / int8)
**Decision:** A checkpoint now offers one or more `GraphVariant`s, each pinning a
Hugging Face repo and graph file; the settings UI picks one and `Quant`
(`Fp32`/`Fp16`/`Int8`) selects which graph is downloaded and loaded. fp32 stays
the default. fp16 is offered for **both** checkpoints; int8 is offered for
**English only** and carries a lower-accuracy warning. This amends ADR-24 (the
quantization method), ADR-36 (int8 is no longer "not yet offered") and ADR-37
(mizchi fp16 is now used).
**Why:** Each candidate graph was evaluated against its fp32
reference on a 20-case self-made MCQ set (`layanow-model --example
precision_parity`, `LAYANOW_ALLOW_MODEL_DOWNLOAD=1`):
- English fp16 (`inferenceprince/laya-onnx`): 20/20 top-1 agreement, max
  `|p-q| = 1.3e-3`.
- English int8 (`inferenceprince/laya-onnx-int8`, weight-only `MatMulNBits`):
  19/20, max `|p-q| = 3.5e-2` — below ADR-24's ≥99% bar. The one flip is a
  near-tie ("capital of France") where fp32 itself picks the wrong answer
  (Berlin) and int8 picks Paris.
- Multilingual fp16 (`mizchi/laya-multilingual-onnx`): 20/20, max
  `|p-q| = 5.5e-4`.
- Multilingual int8 (`soyelmismo/laya-multilingual-onnx` `model-int8.onnx`):
  9/20; the logits collapse to near-uniform (~1/K) for every option and every
  language tried (English, French, Spanish, German, Bulgarian, Japanese), so
  the graph is unusable and is **not registered**.
Weight-only int8 (`MatMulNBits`) is used instead of ADR-24's
`quantize_dynamic`, which both the export spike and the inferenceprince results
show destroys this model. fp16 is kept despite ADR-37's earlier rejection
because it is numerically clean; it is slower on CPUs without native FP16,
which the settings label says.
**Consequence:** `Quant` gains `Fp16`; `CheckpointSpec` holds `variants`;
`Settings` gains `quant`, and the worker reloads when it changes. English int8
is offered behind an explicit accuracy warning in the settings window;
multilingual int8 is not offered. A checkpoint whose requested precision is
absent (e.g. multilingual + int8 in a hand-edited config) falls back to fp32
with a log warning. `layanow-model --example precision_parity` records the
evidence and must be rerun before registering a new precision variant.

## ADR-40 — Optional context (state) capture, file selection and named templates
**Decision:** The overlay gains an optional **Context** box, separate from the
`Tab` question/answer capture (Option A). It is the model's `state`:
- the user types/pastes evidence into it;
- a `file://` URI from a file manager (or an existing absolute path) in the
  selection is read into it — a file selection is never treated as a question or
  answer;
- a **Files…** button opens the native picker (the xdg-desktop-portal
  FileChooser on Linux, via `rfd`; the blocking portal call runs on a worker
  thread so the overlay never freezes);
- a dropdown selects a saved, named **context template**, and **Save…** stores
  the current context under a name;
- templates persist in `~/.config/layanow/config.toml` as
  `contexts = [{ name, text }]` and are also managed in the settings window.
The context is sent as the `state` JSON `{"context": <text>}` (blank → `{}`),
with the question still `ins` and the answers `crit` (ADR-23). Because the
context box, dropdown and Save button need the pointer, the capturing panel is
now pointer-interactive too (the panel already was in the results phase); the
area outside the panel stays click-through (amends ADR-14/25).
**Why:** The app's intended use is deciding from *provided* evidence, and the
model is far stronger with it — the same question went from 37% (wrong) to 92%
(correct) with a passage in `state`, and a bare string `state` measured worse
than the `{"context": …}` object. A separate, optional slot keeps today's fast
question→answers flow unchanged (an empty Context box is simply skipped) while
giving reading-comprehension/ticket use cases a home. Reading a selected file is
the local, network-free way to bring in evidence; named templates make recurring
contexts (a ticket queue, a policy) reusable. This replaces the earlier idea of
retrieval/search (Exa etc.), which would violate the no-runtime-network
invariant (AGENTS.md #4).
**Consequence:** `Settings` gains `contexts`; `Request::Decide` and
`DecisionEngine::decide_choice` carry the context; `render::context_state` wraps
it. `Esc`/hiding clears the context (templates persist). Large files are
capped and the renderer truncates `state` to the token budget, so this suits a
passage/ticket, not a whole document — chunked retrieval remains the
long-term answer for large corpora. The native picker is verified on Linux
(xdg-desktop-portal); on Windows/macOS it will need a parent window handle, and
on macOS the panel must run on the main run loop, once those overlay hosts land.

## ADR-41 — UI is English-only
**Decision:** The user interface (overlay, tray menu, settings window, error and
status strings) is **English-only**. There is no localization, language
switching, or translated message catalogue in v1. This concerns the interface
only: the model checkpoints stay multilingual where offered (ADR-39).
**Why:** The interface is a small set of short labels and status strings, and
there is no localization infrastructure to justify. Adding i18n now would bring
a translation workflow and a message-catalogue dependency for little benefit;
the strings are short and unambiguous.
**Consequence:** All user-facing strings are English literals in the UI crates.
Localization can be added later behind a message catalogue without touching the
model, capture, or resolver layers.

## ADR-42 — No telemetry
**Decision:** The applet collects and sends **no telemetry**: no analytics,
crash reporting, usage tracking, or phone-home of any kind. The only network
activity is the opt-in, first-run model download (ADR-17); captured text never
leaves the machine.
**Why:** The tool reads the user's selected text, which can be sensitive, and
sending anything about it — or even aggregate usage counts — would be a privacy
regression with no product benefit. The project is local-first and the
no-runtime-network invariant already constrains it.
**Consequence:** No telemetry dependency or endpoint exists; adding one would
require a new ADR. Diagnostics stay local (`tracing`, E4).

## ADR-43 — License: `MIT`
**Decision:** The project is licensed under the **MIT License** (SPDX `MIT`),
with the canonical text in `LICENSE-MIT`; `Cargo.toml` and the Nix package
metadata declare `MIT`.
**Why:** MIT is permissive, short, and its only obligation is attribution, so
the applet can be used, modified, and redistributed freely, including
commercially.
**Compatibility:** MIT is compatible with every dependency in the build graph:
they are all permissive (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib,
Unicode-3.0, OFL-1.1, Ubuntu-font-1.0, Unlicense, 0BSD, BSL-1.0,
CDLA-Permissive-2.0) and grant at least the rights MIT needs. `cargo deny check
licenses` enforces this against the allowlist in `deny.toml`. The Hugging Face
checkpoints are compatible too: the Laya weights are Apache-2.0 (© Convai
Innovations) and the ONNX export is MIT (Receptron); the weights are not
redistributed (ADR-17), so only the attribution in
`layanow_model::bundle::ATTRIBUTION` and the README is required.
**Consequence:** `LICENSE-MIT` is the license of record. We respect dependency
licenses by keeping `deny.toml`'s allowlist in sync with the build graph and by
retaining their copyright and attribution notices (including the Apache-2.0
NOTICE files, the Unicode-3.0 license, and the `epaint_default_fonts` OFL-1.1 /
Ubuntu-font-1.0 fonts) when distributing binaries. Contributions are accepted
under MIT unless stated otherwise.

## ADR-44 — Quiz mode: single-selection quiz capture
**Decision:** A `quiz_mode` setting (default off) lets the user capture a whole
quiz — the question/statement and its answer options — in **one** selection.
The captured text is split into blocks: first on blank lines (so a question or
an option may wrap across several lines), then, when there are no blank lines,
on single newlines. The first block is the question and every later block is an
option, shown as `A.`, `B.`, … in the overlay. The model call is unchanged:
`ins` = question, `crit` = the letter-labelled options.
**Why:** Quizzes (Likert polls, MCQ exercises) are usually copied as one text
block; demanding a separate `Tab` capture per option is error-prone. This also
fits the v1 invariants: it is still native text, still a single-answer `choice`
(ADR-8), and the clipboard is untouched (ADR-5).
**Consequence:** Quiz mode is **all-or-nothing**: only the first capture of a
session is accepted, and a selection that does not split into a question plus at
least one option is discarded with a status line. Multi-line blocks are kept
whole, so a wrapped question survives; a newline-separated list (a Likert scale)
is the fallback. Turning the setting off restores the per-item `Tab` capture
(ADR-33). `Settings` gains `quiz_mode`; `layanow_core::parse_quiz` holds the pure
split so the parsing rules are unit-tested without a window.
