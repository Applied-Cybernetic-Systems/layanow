# TODO

Granular, issue-style work log for `layassist`. This is the **living** task list:
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

### M5 — tray, settings, toggle
- **T-111** · 2026-09-22 · M5 · app — Tray icon via `tray-icon`, with show/hide, settings, and quit. (ADR-20)
- **T-113** · 2026-09-22 · M5 · app — Checkpoint selector in settings (manual, no auto-routing). (ADR-7/27-C1)
- **T-114** · 2026-09-22 · M5 · app — int8/fp32 setting; make `Quant` actually select the graph file instead of being metadata. (ADR-7/11/28)
- **T-115** · 2026-09-22 · M5 · app — Confidence threshold setting (default 0.5; warn only, never refuse). (ADR-27-C2)
- **T-116** · 2026-09-22 · M5 · app — Probability colour settings.
- **T-117** · 2026-09-22 · M5 · app — Hot vs on-demand model unload setting. (ADR-11)
- **T-118** · 2026-09-22 · M5 · app — Persist settings to `~/.config/layassist/config.toml` (taplo is in the dev shell). (E5, `OPEN-QUESTIONS.md`)

### Model
- **T-120** · 2026-09-22 · model — Resolve the English default mismatch: ADR-16/`PLAN.md`/`AGENTS.md` say int8, but the shipped `receptron/laya-onnx` bundle is **fp32** (`Quant::Fp32`). Either wire an English int8 artifact and make `Quant` functional, or amend ADR-16. **Needs a decision.**
- **T-121** · 2026-09-22 · model — Confirm int8-vs-fp32 accuracy for the chosen English artifact against ADR-24's bar before defaulting to int8. (ADR-24/28)
- **T-122** · 2026-09-22 · model — Checkpoint registry with lazy load + LRU eviction (one session resident); today only `DEFAULT_REPO` is wired and it loads eagerly. (`LAYA.md`)
- **T-123** · 2026-09-22 · model — Multi-answer (`choice`) support: mark options above a probability threshold. (ADR-8/27-C6)
- **T-124** · 2026-09-22 · model — Optional checkpoint auto-routing (port Laya's `Router`). (ADR-7, v2+)
- **T-125** · 2026-09-22 · model — Map `ModelError` variants to actionable, user-facing messages instead of a raw `to_string()` handed to the UI. (ADR-18)

### Capture & resolvers
- **T-130** · 2026-09-22 · resolve — Move the PRIMARY read off the UI thread (blocking `get_contents` currently runs inside `update`; a stalled owner freezes the overlay). (ADR-33)
- **T-131** · 2026-09-22 · resolve — Evaluate reading PRIMARY through the seat's own `zwp_primary_selection_device_v1` (sctk, already a dependency) instead of `wl-clipboard-rs`/data-control. Robustness/maintenance, not urgent.
- **T-133** · 2026-09-22 · platform — Windows selection backend (simulated copy with clipboard save/restore, opt-in). (M7, ADR-5)
- **T-134** · 2026-09-22 · platform — macOS selection backend (simulated copy, opt-in). (M8, ADR-5)
- **T-135** · 2026-09-22 · platform — Accessibility resolver (AT-SPI / UIA / AX) for clipboard-free reads and region resolution. (M9, ADR-3/13)
- **T-136** · 2026-09-22 · platform — OCR resolver for non-selectable text. (M10, ADR-3)
- **T-137** · 2026-09-22 · resolve — Decide the fallback for apps that never publish PRIMARY (e.g. Zed): accessibility, OCR, or opt-in copy-with-save/restore; document the shortlist. (`README.md` "# Text capture")
- **T-169** · 2026-09-22 · platform — Harden the control socket when `XDG_RUNTIME_DIR` is unset: `socket_path` falls back to `std::env::temp_dir()`, so ensure the socket is mode 0600 or refuse a shared directory. (ADR-34, `crates/layassist-platform/src/control.rs`)

### Core & data model
- **T-159** · 2026-09-22 · core — `Selection.bounds`/`Rect` are currently unused; wire them when the a11y/OCR resolvers land, or drop them if no resolver will populate them. (`RESOLVERS.md`)
- **T-160** · 2026-09-22 · core — Decide the fate of `TextResolver::watch`/`SelectionStream` now that auto-capture was dropped (remove the unused API, or keep it explicitly for the future). (ADR-26/33)

### App & UI
- **T-140** · 2026-09-22 · app — Error popup per ADR-18 (currently errors render inline in the overlay). (ADR-18, M6)
- **T-141** · 2026-09-22 · app — Add a regression test/guarantee that neither PRIMARY reads nor any fallback ever modifies the clipboard. (ADR-5, M6)
- **T-142** · 2026-09-22 · app — Decide and implement history/privacy (local decision log? retention?). (E6)
- **T-143** · 2026-09-22 · app — Logging via `tracing` (targets, verbosity); replace ad-hoc `eprintln!`/`LAYASSIST_DEBUG`. (E4)
- **T-144** · 2026-09-22 · ui — Localization decision (UI English-only in v1?). (E10)
- **T-166** · 2026-09-22 · overlay — Don't dim the whole screen: paint the translucent backdrop only behind the UI panel (question/answer list, field, hints) instead of filling the entire layer surface, and keep every other region fully transparent (and click-through). (`crates/layassist-app/src/overlay.rs`, `theme::OVERLAY_BG`)
- **T-167** · 2026-09-22 · app — Add a per-decision request id/cancellation token so a decision that finishes after a hide/show cannot be consumed as a newer one; the current stale-reply guard only drops replies while the overlay is not in the `Running` phase. (ADR-34, `crates/layassist-app/src/worker.rs`)
- **T-168** · 2026-09-22 · app — Verify the live Wayland show/hide end-to-end (surface maps + keyboard grab on `show`, buffer detached + grab released on `hide`); the command path is covered but the visual mapping is not. (ADR-34, T-112)

### Build, CI & packaging
- **T-150** · 2026-09-22 · ci — Add `cargo audit` to CI (listed in `PLAN.md`, not currently run). (E2)
- **T-151** · 2026-09-22 · ci — Align doc linting with `PLAN.md`: `RUSTDOCFLAGS=-D warnings` and/or set `missing_docs = "deny"` (currently `warn`).
- **T-152** · 2026-09-22 · build — Remove dead dev-shell entries: `vulkan-loader` and the `WGPU_BACKEND` export (wgpu/eframe dropped in ADR-31).
- **T-154** · 2026-09-22 · pkg — Autostart at login (systemd user unit / xdg autostart). (D3)
- **T-155** · 2026-09-22 · pkg — Distribution/packaging (Nix package for Linux; Windows/macOS later). (D2)
- **T-156** · 2026-09-22 · pkg — Update mechanism / model version pinning decision. (D5)
- **T-157** · 2026-09-22 · pkg — Telemetry decision (assumed none). (E9)
- **T-163** · 2026-09-22 · build — Audit dev-shell dependencies (`wtype`, `at-spi2-core`/`dbus`, `vulkan-loader`) against actual use and remove or justify each.
- **T-165** · 2026-09-22 · ci — Keep the `.cargo/config.toml` aliases (`lint`, `gate`) and CI in sync (CI calls the raw commands; the `gate` alias is unused).

### Docs & maintenance
- **T-158** · 2026-09-22 · docs — M6 documentation pass: refresh `README.md`/`PLAN.md`/`RESOLVERS.md` after M5 lands (status, quickstart, settings).
- **T-162** · 2026-09-22 · docs — Decide whether to annotate superseded ADRs (ADR-16 → ADR-28, ADR-30 → ADR-31 → ADR-33) with a "Superseded by" pointer, or document the supersession convention once in `DECISIONS.md`.
- **T-164** · 2026-09-22 · docs — Maintain a short list of apps known not to publish PRIMARY (Zed, Alacritty untested) and the recommended workaround, so users can self-diagnose. (`README.md`)

## In Progress

_(empty — pick a task from **Open** and move its line here when you start it.)_

## Closed

- **T-001** · 2026-09-22 · M4/app — Wayland PRIMARY highlight capture + typed entry, committed with `Tab` (first = question, rest = answers). **Closed:** 2026-09-22 · verified end-to-end (Brave highlight captured while the overlay held the keyboard; clipboard untouched) and shipped in the `feat(platform): capture the Wayland PRIMARY highlight on Tab (M4)` commit (ADR-33).
- **T-002** · 2026-09-22 · docs — Remove stale/superseded docs and dead code; introduce `TODO.md` and document it in `AGENTS.md`/`README.md`. **Closed:** 2026-09-22 · shipped in the same commit.
- **T-010** · 2026-09-22 · M0 · build — Rust workspace, Nix dev shell, CI quality gate, and the `ort` smoke test. **Closed:** done (M0).
- **T-011** · 2026-09-22 · M1 · model — Export spike: multilingual → ONNX + dynamic int8; parity and accuracy measured. **Closed:** done; int8 missed ADR-24 for multilingual → fp32 default (ADR-28).
- **T-012** · 2026-09-22 · M2 · model — `layassist-model`: tokenizer, rendering/calibration port, golden fixtures. **Closed:** done (ADR-29).
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
- **T-028** · 2026-09-22 · docs — No documentation of the highlight-buffer requirement or diagnostics. **Closed:** added a "Text capture" section to `README.md` (`wl-paste -p`, `read_primary`, `LAYASSIST_DEBUG`).
- **T-161** · 2026-09-22 · build — The platform selection module (`src/selection*`) and `examples/read_primary.rs` are untracked; stage/commit them with the ADR-33 change. **Closed:** 2026-09-22 · already tracked and committed in `122612a`; verified with `git ls-files` on a clean tree.
- **T-132** · 2026-09-22 · core — Add `Source::Manual` so typed items carry provenance instead of being recorded as `Source::Selection`. **Closed:** 2026-09-22 · added the `Source::Manual` variant; `Overlay::capture_item` now tags typed items `Manual` while highlights stay `Selection`, with tests for both provenances; shipped in the `feat(core): record typed items as Source::Manual (T-132)` commit.
- **T-110** · 2026-09-22 · M5 · app — Implement `layassist toggle` (IPC/socket or dbus) so the resident applet can be shown/hidden. (ADR-6/19, `README.md`) **Closed:** 2026-09-22 · cross-platform `interprocess` local socket (Unix UDS / Windows named pipe) + `toggle|show|hide|quit` commands, listener thread forwarding over `mpsc`; shipped in the `feat(app): resident control socket and hidden-by-default overlay (T-110/T-112/T-153)` commit (ADR-34).
- **T-112** · 2026-09-22 · M5 · app — Make the overlay **hidden by default** and stop grabbing the keyboard unless it is shown (`KeyboardInteractivity::Exclusive` only while visible); fixes "can't type while the app runs". (ADR-26/31) **Closed:** 2026-09-22 · overlay starts hidden (no buffer, `KeyboardInteractivity::None`); `OverlayApp::visible` maps to Exclusive + a fresh buffer on show and detaches the buffer on hide; `Esc` now hides. Shipped in the same commit.
- **T-153** · 2026-09-22 · pkg — Enforce a single applet instance. (D4) **Closed:** 2026-09-22 · the control socket is the single-instance lock: bind rejects a live owner and reclaims a stale Unix socket; verified end-to-end (second invocation prints "already running" and exits 0). Shipped in the same commit.
