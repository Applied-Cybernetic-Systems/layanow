# layanow

A cross-platform Rust desktop **applet**: highlight a question and its candidate
answers as native text (no OCR), and it asks a local **Laya** decision model
which answer is correct, then shows a ranked list with probability colours.

General-purpose ("any text"), Linux/Wayland first.

```
hotkey ─▶ egui overlay ─▶ drag over the question, then each answer ─▶ Enter
                                                                    │
                                          Laya (ONNX Runtime, fp32) ◀┘
                                                                    │
                         coloured probability panel + top pick ◀─────┘
```

## Status

M0–M2 complete: the `layanow-model` crate acquires/verifies the ONNX bundle,
loads a checkpoint through `ort`, ports Laya's rendering/calibration, and
replays committed golden fixtures against the `rl_common.py` reference.

M4 in progress: the Wayland **PRIMARY selection** backend is wired (ADR-33) —
highlight text natively and press `Tab` to add it as the question, repeat for
each answer, then `Enter` to decide. Typing an item into the field and pressing
`Tab` still works as a fallback. It reads PRIMARY (the highlight buffer) only
and never touches the clipboard (ADR-5). The X11 and Windows/macOS backends
remain.

M5 in progress: the applet is now **resident** and starts **hidden** (ADR-34).
Run it once, then a compositor bind (or `layanow toggle`) shows the overlay;
`Esc` hides it and `layanow quit` stops the applet. A **tray icon** (ADR-35)
offers Toggle/Quit and a left click toggles. A standalone **settings window**
(`layanow settings`, or the tray's **Settings…**) chooses the checkpoint, its
**precision** (fp32/fp16, plus English int8 behind a lower-accuracy warning —
ADR-39), the low-confidence threshold, hot vs on-demand model residency, and the
**probability-bar colours**; it writes `~/.config/layanow/config.toml` and the
applet applies the change live (ADR-38). The overlay also has an optional
**Context** box (ADR-40) — the model's `state`: type/paste evidence, click
**Files…** for the native picker, select files in a file manager (their
`file://` URIs are read), or pick a saved **context template**; it is what makes
document/ticket questions reliable. The
control channel is a cross-platform local socket (Unix domain socket / Windows
named pipe) that also enforces a single instance.

```sh
nix develop
cargo run -p layanow-app            # resident applet; overlay starts hidden
# in another terminal, or a compositor bind:
cargo run -p layanow-app -- toggle  # show/hide · also: show, hide, quit
cargo run -p layanow-app -- settings  # open the settings window
# highlight text (or type), Tab to add · Enter to decide · Esc: hide
# optional: put evidence in the Context box (type/paste, Files… picker, file-manager selection, or a saved template)
```

## Documentation

| File | Contents |
|---|---|
| `AGENTS.md` | Repo guide for agents: invariants, quality gate, conventions |
| `PLAN.md` | Scope, UX, Rust workspace, quality gate, milestones |
| `DECISIONS.md` | Why Rust, why ONNX, egui, resource policy, etc. |
| `FEASIBILITY.md` | Language analysis, ONNX export feasibility, crates, risks |
| `RESOLVERS.md` | `TextResolver` trait and per-platform backends |
| `LAYA.md` | ONNX contract, checkpoint export, rendering port, calibration |
| `OPEN-QUESTIONS.md` | Every decision still needing clarification |
| `TODO.md` | Living, issue-style task log: Open / In Progress / Closed |

## Development environment (Nix)

`flake.nix` pins the same nixpkgs revision as `~/projects/nix` and provides:

- `cargo`, `rustc`, `clippy`, `rustfmt`, `rust-analyzer`
- `onnxruntime` (used by `ort` via `ORT_DYLIB_PATH`)
- `at-spi2-core` + `dbus` (accessibility), `wl-clipboard`, `wtype`
- `wayland`, `wayland-protocols`, `libxkbcommon`, `mesa`, `libGL`
- `pkg-config`, `cmake`, `openssl`
- the AT-SPI bus started on shell entry + the `layanow-atspi` helper

```sh
cd ~/projects/layanow
nix develop
cargo build
```

`.envrc` is provided for direnv.

## Quickstart (planned)

```sh
nix develop
cargo run -p layanow-app          # starts the tray applet, loads the model
# hotkey opens the overlay
```

Hotkey: on Wayland/MangoWC a compositor bind runs `layanow toggle` (Wayland
has no app-level global hotkeys). On Windows/macOS the applet registers it.

## Text capture (highlight buffer)

The text source is the OS **selection buffer** — on Wayland the **PRIMARY**
highlight, never the clipboard (ADR-5/13). Highlight text with the **mouse** in
the target app and press **`Tab`** in the overlay to add it (first = question,
rest = answers). You can also type an item and press `Tab`.

Not every app publishes a highlight buffer (Zed does not; browsers and most
toolkits do). To check an app before running the applet, highlight text and run:

```sh
nix develop -c wl-paste -p     # prints the highlight, or nothing
```

The standalone probe is
`cargo run -p layanow-platform --example read_primary`. Run the applet with
`LAYANOW_LOG=layanow=debug` (or the `LAYANOW_DEBUG=1` shorthand) to log capture
outcomes (lengths only, never the text).

## Model smoke test (M0)

The `layanow-model` crate loads a Laya ONNX bundle and runs one `choice`
decision. The bundle (~1.7 GB, fp32 English checkpoint) is **not** bundled: on
first run it is downloaded into the XDG cache
(`~/.cache/layanow/models/receptron--laya-onnx/`). Downloading is opt-in and
every file is verified against the repository manifest (SHA-256 / git-object
SHA-1) before it is accepted (ADR-17). The weights are © Convai Innovations
(Apache-2.0); the example prints the attribution.

```sh
nix develop
# first run only: fetch the bundle
LAYANOW_ALLOW_MODEL_DOWNLOAD=1 cargo run -p layanow-model --example smoke
# later runs reuse the cache
cargo run -p layanow-model --example smoke
```

The same end-to-end check is an ignored test:

```sh
cargo test -p layanow-model --test smoke -- --ignored
```

Overrides: `LAYANOW_MODEL_CACHE` (cache root), `LAYANOW_INTRA_OP_THREADS`
(ORT CPU thread cap). The graph requires `ORT_DYLIB_PATH`, which the dev shell
sets.

## Non-goals (v1)

- OCR / vision (reserved as a future resolver)
- Accessibility-tree resolver (future; v1 uses the native selection buffer only)
- Browser/CDP integration (deferred)
- Multi-answer (checkbox) quizzes (deferred)
- Automatic answering of anything (this only *advises*)
