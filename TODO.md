# TODO

Granular, issue-style work log for `layanow`. This is the **living** task list:
agents must update it whenever work is started, finished, or newly discovered.

## How to use this file

- **One line per task**, grouped by area. Format:
  `- **T-###** · YYYY-MM-DD · <milestone/area> · <title> — <detail> (refs)`
- Ids increase over time but the file is grouped by area, so ids are not
  strictly sequential within a section.
- **Ids are stable and are never reused.** The next new task takes the next
  number, regardless of which section it lands in.
- **The section is the status.** New work goes in *Open*; when you start it,
  move the line to *In Progress* and add the start date; when it is done, move
  it to *Closed* and append `**Closed:** <outcome, date, commit>`.
- **Record discoveries, don't act silently:** anything you notice but don't do
  becomes a new *Open* line.
- `M0`–`M10` refer to the milestones in `PLAN.md`; `ADR-n` refers to
  `DECISIONS.md`; the named doc is the detailed spec.
- Areas: `core`, `model`, `resolve`, `overlay`, `app`, `platform`, `ui`, `docs`,
  `ci`, `build`, `pkg`.

## Open

### M4 — Linux selection backends (deferred)
- **T-100** · 2026-09-22 · M4 · platform — X11 selection backend reading PRIMARY via `x11rb`. Deferred by user decision 2026-09-22; Wayland is the only backend for now. (ADR-21, `RESOLVERS.md`)

### Model
- **T-122** · 2026-09-22 · model — Checkpoint registry with lazy load + LRU eviction (one session resident). A `CheckpointSpec` registry and settings-driven selection landed (T-173); the session is still loaded once at startup and is not yet cached/reloaded on a settings change. (`LAYA.md`)
- **T-123** · 2026-09-22 · model — Multi-answer (`choice`) support: mark options above a probability threshold. (ADR-8/27-C6)
- **T-124** · 2026-09-22 · model — Optional checkpoint auto-routing (port Laya's `Router`). (ADR-7, v2+)
- **T-125** · 2026-09-22 · model — Map `ModelError` variants to actionable, user-facing messages instead of a raw `to_string()` handed to the UI. (ADR-18)
- **T-173** · 2026-09-22 · model — Align `DEFAULT_REPO` with the consolidated hub. **Closed:** 2026-09-22 · decision (ADR-37): keep `receptron/laya-onnx` as the default English fp32 ONNX and add the community `soyelmismo/laya-multilingual-onnx` fp32 export for multilingual, behind a new `CheckpointSpec` registry. The `convaiinnovations/laya` hub publishes safetensors, not ONNX, so it cannot be loaded directly. Verified the multilingual export end-to-end: it downloads/verifies through the Hugging Face manifest, loads in `ort`, and ranks a 3-option `choice` correctly. Shipped in the `feat(model): checkpoint registry and multilingual ONNX source (T-173)` commit (`bundle.rs`, `LAYA.md`).

- **T-178** · 2026-09-23 · model — Accuracy of the checkpoints themselves: English fp32 picks Berlin over Paris for "capital of France" (0.33), and multilingual fp32 misses some simple French/Italian capitals. Evaluate and document model limits; `layanow-model --example ask` reproduces a single question (and `LAYANOW_STATE` sets the context) and `--example precision_parity` compares precisions. (ADR-12)
- **T-179** · 2026-09-23 · app — Let the user supply context (Laya's `state`): the app always sends `{}` (ADR-23), but with the relevant passage in `state` the same question goes from 37% (wrong) to 92% (correct) — context-bearing decisions are far more accurate (M1: 8/8 vs 5/12). Decide how to capture/enter context. (ADR-23/28)

### Capture & resolvers
- **T-130** · 2026-09-22 · resolve — Move the PRIMARY read off the UI thread (blocking `get_contents` currently runs inside `update`; a stalled owner freezes the overlay). (ADR-33)
- **T-131** · 2026-09-22 · resolve — Evaluate reading PRIMARY through the seat's own `zwp_primary_selection_device_v1` (sctk, already a dependency) instead of `wl-clipboard-rs`/data-control. Robustness/maintenance, not urgent.
- **T-133** · 2026-09-22 · platform — Windows selection backend (simulated copy with clipboard save/restore, opt-in). (M7, ADR-5)
- **T-134** · 2026-09-22 · platform — macOS selection backend (simulated copy, opt-in). (M8, ADR-5)
- **T-135** · 2026-09-22 · platform — Accessibility resolver (AT-SPI / UIA / AX) for clipboard-free reads and region resolution. (M9, ADR-3/13)
- **T-136** · 2026-09-22 · platform — OCR resolver for non-selectable text. (M10, ADR-3)
- **T-137** · 2026-09-22 · resolve — Decide the fallback for apps that never publish PRIMARY (e.g. Zed): accessibility, OCR, or opt-in copy-with-save/restore; document the shortlist. (`README.md` "# Text capture")

### App & UI
- **T-140** · 2026-09-22 · app — Error popup per ADR-18 (currently errors render inline in the overlay). (ADR-18, M6)
- **T-141** · 2026-09-22 · app — Add a regression test/guarantee that neither PRIMARY reads nor any fallback ever modifies the clipboard. (ADR-5, M6)
- **T-142** · 2026-09-22 · app — Decide and implement history/privacy (local decision log? retention?). (E6)
- **T-144** · 2026-09-22 · ui — Localization decision (UI English-only in v1?). (E10)
- **T-168** · 2026-09-22 · app — Verify the live Wayland show/hide end-to-end (surface maps + keyboard grab on `show`, buffer detached + grab released on `hide`); the command path is covered but the visual mapping is not. (ADR-34, T-112)
- **T-172** · 2026-09-22 · app — Reflect overlay visibility in the tray menu (checked toggle / dynamic label); needs a visibility→tray channel. (ADR-35)

### Build, CI & packaging
- **T-150** · 2026-09-22 · ci — Add `cargo audit` to CI (listed in `PLAN.md`, not currently run). (E2)
- **T-154** · 2026-09-22 · pkg — Autostart at login (systemd user unit / xdg autostart). (D3)
- **T-156** · 2026-09-22 · pkg — Update mechanism / model version pinning decision. (D5)
- **T-157** · 2026-09-22 · pkg — Telemetry decision (assumed none). (E9)
- **T-163** · 2026-09-22 · build — Audit dev-shell dependencies (`wtype`, `at-spi2-core`/`dbus`) against actual use and remove or justify each.

### Docs & maintenance
- **T-162** · 2026-09-22 · docs — Decide whether to annotate superseded ADRs (ADR-16 → ADR-28, ADR-30 → ADR-31 → ADR-33) with a "Superseded by" pointer, or document the supersession convention once in `DECISIONS.md`.
- **T-164** · 2026-09-22 · docs — Maintain a short list of apps known not to publish PRIMARY (Zed, Alacritty untested) and the recommended workaround, so users can self-diagnose. (`README.md`)

## In Progress

_(empty — pick a task from **Open** and move its line here when you start it.)_

## Closed

- **T-114** · 2026-09-22 · M5 · app — int8/fp32 setting; make `Quant` actually select the graph file instead of being metadata. Default stays **fp32**; int8 is opt-in (ADR-36). Blocked on T-121; the settings window (T-113) is ready to host it. (ADR-7/11/28/36) **Closed:** 2026-09-23 · `Quant` now selects a `GraphVariant` (fp32/fp16/int8) per checkpoint; the settings window gained a **Precision** dropdown and `Settings.quant` persists in `config.toml`; the worker rebuilds the engine when checkpoint or precision changes. English offers fp32/fp16/int8 (int8 behind a lower-accuracy warning); multilingual offers fp32/fp16 only (int8 rejected). Shipped in the `feat: precision variants and probability colours (T-114/T-116/T-121)` commit (ADR-39).
- **T-116** · 2026-09-22 · M5 · app — Probability colour settings. **Closed:** 2026-09-23 · `Settings.palette` stores the three probability-bar anchor colours (low/mid/high, defaulting to the gruvbox red/orange/green); the settings window has three colour pickers and `results::probability_colour` lerps through them, applied live on `Reload`. Shipped in the same commit (ADR-32/39).
- **T-121** · 2026-09-22 · model — Confirm int8-vs-fp32 accuracy for the chosen English artifact against ADR-24's bar before offering int8 as an opt-in setting. (ADR-24/28/36) **Closed:** 2026-09-23 · evaluated the shipped graphs against fp32 on a 20-case self-made MCQ set via `layanow-model --example precision_parity`: fp16 is effectively exact for both checkpoints (20/20, max `|p-q| ≤ 1.3e-3`); English int8 is 19/20 (below the ≥99% bar → offered only with a warning); multilingual int8 is 9/20 and near-uniform in every language → not registered. Shipped in the same commit (ADR-24/39).

- **T-001** · 2026-09-22 · M4/app — Wayland PRIMARY highlight capture + typed entry, committed with `Tab` (first = question, rest = answers). **Closed:** 2026-09-22 · verified end-to-end (Brave highlight captured while the overlay held the keyboard; clipboard untouched) and shipped in the `feat(platform): capture the Wayland PRIMARY highlight on Tab (M4)` commit (ADR-33).
- **T-002** · 2026-09-22 · docs — Remove stale/superseded docs and dead code; introduce `TODO.md` and document it in `AGENTS.md`/`README.md`. **Closed:** 2026-09-22 · shipped in the same commit.
- **T-010** · 2026-09-22 · M0 · build — Rust workspace, Nix dev shell, CI quality gate, and the `ort` smoke test. **Closed:** done (M0).
- **T-011** · 2026-09-22 · M1 · model — Export spike: multilingual → ONNX + dynamic int8; parity and accuracy measured. **Closed:** done; int8 missed ADR-24 for multilingual → fp32 default (ADR-28).
- **T-012** · 2026-09-22 · M2 · model — `layanow-model`: tokenizer, rendering/calibration port, golden fixtures. **Closed:** done (ADR-29).
- **T-013** · 2026-09-22 · M3 · app — Click-through Wayland layer-shell overlay, `Session` model, inference worker, results panel. **Closed:** done (ADR-30/31).
- **T-014** · 2026-09-22 · M3 · docs — Auto-capture on selection change (ADR-26). **Closed:** not needed for v1 (user decision 2026-09-22); manual `Tab` capture (ADR-33) stands.
- **T-020** · 2026-09-22 · platform — Dead, misleading `accessibility_available()` stub claiming M4. **Closed:** removed.
- **T-021** · 2026-09-22 · model — `Quant` docs implied it was a default and affected loading; it is metadata only. **Closed:** documented on `Quant`, `checkpoint_from_dir`, and `DEFAULT_REPO`.
- **T-022** · 2026-09-22 · docs — `FEASIBILITY.md` said "egui/eframe chosen". **Closed:** updated to egui + custom `wlr-layer-shell` host (ADR-31).
- **T-023** · 2026-09-22 · docs — `LAYA.md` claimed "default: int8 / English int8 published". **Closed:** corrected to shipped fp32 + ADR-28 pointer.
- **T-024** · 2026-09-22 · docs — `PLAN.md` M3 row still said "resolver stub". **Closed:** marked as replaced by the M4 backend.
- **T-025** · 2026-09-22 · docs — `RESOLVERS.md` interface omitted the `ResolveError` type. **Closed:** fixed.
- **T-026** · 2026-09-22 · docs — `OPEN-QUESTIONS.md` E8 (MSRV/edition) listed as deferred though decided. **Closed:** moved to resolved.
- **T-027** · 2026-09-22 · build — `deny.toml` allowed `MPL-2.0`, which no dependency uses (warning). **Closed:** allowance removed.
- **T-028** · 2026-09-22 · docs — No documentation of the highlight-buffer requirement or diagnostics. **Closed:** added a "Text capture" section to `README.md` (`wl-paste -p`, `read_primary`, `LAYANOW_DEBUG`).
- **T-161** · 2026-09-22 · build — The platform selection module (`src/selection*`) and `examples/read_primary.rs` are untracked; stage/commit them with the ADR-33 change. **Closed:** 2026-09-22 · already tracked and committed in `122612a`; verified with `git ls-files` on a clean tree.
- **T-132** · 2026-09-22 · core — Add `Source::Manual` so typed items carry provenance instead of being recorded as `Source::Selection`. **Closed:** 2026-09-22 · added the `Source::Manual` variant; `Overlay::capture_item` now tags typed items `Manual` while highlights stay `Selection`, with tests for both provenances; shipped in the `feat(core): record typed items as Source::Manual (T-132)` commit.
- **T-110** · 2026-09-22 · M5 · app — Implement `layanow toggle` (IPC/socket or dbus) so the resident applet can be shown/hidden. (ADR-6/19, `README.md`) **Closed:** 2026-09-22 · cross-platform `interprocess` local socket (Unix UDS / Windows named pipe) + `toggle|show|hide|quit` commands, listener thread forwarding over `mpsc`; shipped in the `feat(app): resident control socket and hidden-by-default overlay (T-110/T-112/T-153)` commit (ADR-34).
- **T-112** · 2026-09-22 · M5 · app — Make the overlay **hidden by default** and stop grabbing the keyboard unless it is shown (`KeyboardInteractivity::Exclusive` only while visible); fixes "can't type while the app runs". (ADR-26/31) **Closed:** 2026-09-22 · overlay starts hidden (no buffer, `KeyboardInteractivity::None`); `OverlayApp::visible` maps to Exclusive + a fresh buffer on show and detaches the buffer on hide; `Esc` now hides. Shipped in the same commit.
- **T-153** · 2026-09-22 · pkg — Enforce a single applet instance. (D4) **Closed:** 2026-09-22 · the control socket is the single-instance lock: bind rejects a live owner and reclaims a stale Unix socket; verified end-to-end (second invocation prints "already running" and exits 0). Shipped in the same commit.
- **T-170** · 2026-09-22 · docs — Rename the project from `layassist` to `layanow` (crate/package/binary names, `LAYASSIST_*` env vars, `~/.cache/layanow`, socket id, all docs, GitHub repo). **Closed:** 2026-09-22 · renamed the five crates, the binary, the env vars, the cache/socket paths and every doc reference; moved the user's `layanow.png` into `crates/layanow-platform/assets/`; migrated `~/.cache/layassist` → `~/.cache/layanow`; GitHub repo renamed to `Uiyx/layanow`.
- **T-111** · 2026-09-22 · M5 · app — Tray icon via `tray-icon`, with show/hide, settings, and quit. (ADR-20) **Closed:** 2026-09-22 · `layanow-platform::tray` uses `tray-icon` with the Linux `ksni` (StatusNotifierItem, no GTK) backend; menu `Toggle overlay`/`Quit` plus left-click toggle, posting to the shared `Command` channel (now `crossbeam-channel`); embedded 32x32 `layanow.png`; best-effort startup. Verified the SNI registration over D-Bus (`org.kde.StatusNotifierItem-*`) and clean quit. `Settings…` deferred to T-171. Shipped in the `feat(app): system tray with toggle and quit (T-111)` commit (ADR-35).
- **T-120** · 2026-09-22 · model — Resolve the English default mismatch: ADR-16/`PLAN.md`/`AGENTS.md` say int8, but the shipped `receptron/laya-onnx` bundle is **fp32** (`Quant::Fp32`). Either wire an English int8 artifact and make `Quant` functional, or amend ADR-16. **Needs a decision.** **Closed:** 2026-09-22 · decision: English default stays **fp32** (the shipped artifact); dynamic int8 becomes an **opt-in setting** when the settings menu lands (T-113/T-114), gated on T-121. Amended ADR-11/16/24 and added ADR-36; updated `AGENTS.md`/`PLAN.md`/`OPEN-QUESTIONS.md`/`README.md`/`LAYA.md`.
- **T-174** · 2026-09-22 · platform — Layer-surface hide/show bug: `attach(None)` unmapped the surface, and the next `set_keyboard_interactivity` failed with `zwlr_layer_surface_v1 error: layer_surface has never been configured`. Fix: keep it mapped and blank it to transparent on hide. (ADR-34) **Closed:** 2026-09-22 · reproduced (show→hide→show errored); fixed by presenting a transparent frame on hide instead of detaching the buffer, and gating visibility changes on the surface being configured; verified with 3 hide/show cycles with no protocol error. Shipped in the `fix(platform): keep the layer surface mapped when hiding (T-174)` commit.
- **T-175** · 2026-09-22 · ui — Results view: show the question above the ranked results, and inset the results block so it does not sit flush against the left screen edge. **Closed:** 2026-09-22 · `Phase::Results` now draws `Question: …` above the rows and wraps the block in a `RESULTS_LEFT_MARGIN` (160 px) inset. Shipped in the `feat(ui): show the question above results and inset the panel (T-175)` commit.
- **T-176** · 2026-09-22 · ui — Render the `Question: ` prefix and the question text in different colours. **Closed:** 2026-09-22 · added `theme::shadowed_labelled_text` (a two-colour `LayoutJob` painted with the drop shadow) and used it for the question in both the capturing and results views (prefix `FG4`, question `BLUE`). Shipped in the `feat(ui): two-tone question label (T-176)` commit.
- **T-167** · 2026-09-22 · app — Add a per-decision request id/cancellation token so a decision that finishes after a hide/show cannot be consumed as a newer one. **Closed:** 2026-09-22 · `Request::Decide` and `Response` now carry a `u64` id; `Overlay` tracks the id of the in-flight decision and drops any reply whose id does not match (the old guard only checked the phase). Added a gated-engine regression test that abandons decision 0, runs decision 1, and asserts the stale reply is ignored. Shipped in the `fix(app): drop stale decision replies by request id (T-167)` commit (ADR-34).
- **T-151** · 2026-09-22 · ci — Align doc linting with `PLAN.md`. **Closed:** 2026-09-22 · set `missing_docs = "deny"` in the workspace lints (matching `PLAN.md`/`AGENTS.md`) and added `RUSTDOCFLAGS=-D warnings` to the CI docs step; the whole workspace is clean under both. Shipped in the `chore(ci): deny missing docs and rustdoc warnings (T-151)` commit.
- **T-152** · 2026-09-22 · build — Remove dead dev-shell entries. **Closed:** 2026-09-22 · dropped `vulkan-loader` (unused since ADR-31 replaced eframe/wgpu with the `egui_glow`/glutin host) and the `WGPU_BACKEND` export from `flake.nix`, and updated the README dev-environment list. Shipped in the `chore(build): drop dead vulkan/wgpu dev-shell entries (T-152)` commit.
- **T-160** · 2026-09-22 · core — Decide the fate of `TextResolver::watch`/`SelectionStream`. **Closed:** 2026-09-22 · decision: **delete** both — data-control exposes no change notification, ADR-33 replaced auto-capture with manual `Tab`-commit, and no backend implemented it. Removed the trait method, the `SelectionStream` type, and every impl; documented that auto-capture would observe the overlay's own `wl_data_device` and need a new interface. Shipped in the `refactor: prune resolver API and cargo aliases (T-159/T-160/T-165)` commit (ADR-26/33).
- **T-165** · 2026-09-22 · ci — Keep the `.cargo/config.toml` aliases and CI in sync. **Closed:** 2026-09-22 · a cargo alias can run only one command, so the misnamed `gate = fmt` could never express the full gate; removed it and added `fmt-check`, leaving `lint`/`fmt-check` matching the CI steps. Shipped in the same commit.
- **T-159** · 2026-09-22 · core — `Selection.bounds`/`Rect` are currently unused. **Closed:** 2026-09-22 · decision: **keep and document** them as reserved for the a11y/OCR resolvers to report screen regions (ADR-3); added doc comments to `Rect`/`Selection::bounds` and a `RESOLVERS.md` note. Shipped in the same commit.
- **T-169** · 2026-09-22 · platform — Harden the control socket when `XDG_RUNTIME_DIR` is unset. **Closed:** 2026-09-22 · the fallback now lives in a per-uid subdirectory of the temp dir created mode 0700 (refusing a squatting file/symlink), and `bind` restricts the socket itself to 0600. Added a unit test for the directory hardening. Shipped in the `fix(platform): keep the control socket user-private (T-169)` commit (ADR-34).
- **T-143** · 2026-09-22 · app — Logging via `tracing` (targets, verbosity). **Closed:** 2026-09-22 · added `tracing` to the libraries and a `tracing-subscriber` `EnvFilter` in the binary: `LAYANOW_LOG` (or `RUST_LOG`), default `warn`, with `LAYANOW_DEBUG` as a `layanow=debug` shorthand; replaced every `eprintln!` and the `LAYANOW_DEBUG` gate with `info!`/`warn!`/`error!`/`debug!` events and updated the README. Captured text is never logged. Shipped in the `feat(app): structured logging via tracing (T-143)` commit (E4).
- **T-166** · 2026-09-22 · overlay — Don't dim the whole screen: paint the translucent backdrop only behind the UI panel and keep every other region transparent and click-through. **Closed:** 2026-09-22 · the overlay is now an auto-sized egui `Area` whose `Frame` carries the translucent backdrop, so only the panel is dimmed; `OverlayApp::wants_pointer` was replaced by `interactive_rect`, and the Wayland host sets the input region to exactly that rectangle (empty otherwise), keeping everything outside the panel click-through. Shipped in the `feat(ui): paint the backdrop only behind the panel (T-166)` commit (ADR-14/31).
- **T-113** · 2026-09-22 · M5 · app — Checkpoint selector in settings. **Closed:** 2026-09-22 · the `CheckpointSpec` registry (T-173) is offered in the settings window (`layanow settings`); selecting one writes `checkpoint` to `config.toml`, sends `Command::Reload`, and the worker rebuilds the model on its thread. Manual only, no auto-routing. Shipped in the `feat(app): standalone settings window (T-113–T-118/T-171)` commit (ADR-7/38).
- **T-115** · 2026-09-22 · M5 · app — Confidence threshold setting. **Closed:** 2026-09-22 · `confidence_threshold` (default 0.5) is a slider in the settings window and is applied live to the results panel; it only flags low confidence, never refuses. Shipped in the same commit (ADR-27 C2).
- **T-117** · 2026-09-22 · M5 · app — Hot vs on-demand model unload setting. **Closed:** 2026-09-22 · `unload = "hot" | "on-demand"` in `config.toml`; the factory-backed worker keeps the engine (hot) or drops it after each decision and reloads on the next (on-demand). Shipped in the same commit (ADR-11).
- **T-118** · 2026-09-22 · M5 · app — Persist settings to `~/.config/layanow/config.toml`. **Closed:** 2026-09-22 · `Settings` (checkpoint, confidence threshold, unload policy) loads at startup and saves from the settings window; an unknown or invalid file falls back to defaults. Shipped in the same commit (E5).
- **T-171** · 2026-09-22 · app — Add a `Settings…` entry to the tray menu. **Closed:** 2026-09-22 · the tray menu is now `Toggle overlay`/`Settings…`/`Quit`; `Settings…` spawns `layanow settings`. Shipped in the same commit (ADR-35/38).
- **T-158** · 2026-09-22 · docs — M6 documentation pass after M5. **Closed:** 2026-09-22 · refreshed `README.md` (status + `layanow settings` quickstart), `PLAN.md` (M5 row), `RESOLVERS.md` (dropped the removed `watch` API), and `OPEN-QUESTIONS.md` (E4/E5 resolved) as the settings work landed. Shipped across the M5 commits.
- **T-155** · 2026-09-22 · pkg — Distribution/packaging (Nix package for Linux). **Closed:** 2026-09-22 · added `packages.x86_64-linux.{default,layanow}` and an `apps.layanow` entry to `flake.nix`, built with `rustPlatform.buildRustPackage` (the sandbox test phase passes) and wrapped with `ORT_DYLIB_PATH` and the Wayland/GL/X11 runtime libs. Installs `layanow.desktop`, `layanow-settings.desktop`, and the hicolor icon so rofi and other launchers find it. Windows/macOS packaging remains (D2). Shipped in the `feat(pkg): Nix package with desktop entries (T-155)` commit.
- **T-177** · 2026-09-22 · docs — Transfer the repository to the `Applied-Cybernetic-Systems` GitHub organization, still private. **Closed:** 2026-09-22 · transferred via the GitHub transfer API; updated the local `origin` remote and ADR-22/E1. Committed in the `feat(pkg): Nix package with desktop entries (T-155)` commit.
