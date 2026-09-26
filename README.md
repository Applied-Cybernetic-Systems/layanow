# layanow

A cross-platform Rust desktop **applet**: highlight a question and its candidate
answers as native text (no OCR), and it asks a local **Laya** decision model
which answer is correct, then shows a ranked list with probability colours.

General-purpose ("any text"), Linux/Wayland first.

```
hotkey ─▶ egui overlay ─▶ highlight the question, then each answer, Tab ─▶ Enter
                                                                    │
                                          Laya (ONNX Runtime, fp32) ◀┘
                                                                    │
                         coloured probability panel + top pick ◀─────┘
```

## Status

Linux/Wayland, working end to end:

- **Resident applet** that starts hidden; `layanow toggle|show|hide|quit` and a
  tray icon (Toggle / Settings… / Quit) control it (ADR-34/35).
- **Native PRIMARY capture** — highlight text and press `Tab`; typed text is a
  fallback. The clipboard is never touched (ADR-5/33).
- **Optional Context** (the model's `state`): type/paste, pick files with the
  native picker, select files in a file manager (their `file://` URIs are read),
  or reuse a saved context template (ADR-40).
- **Local Laya** via ONNX Runtime, CPU-only, with a per-checkpoint **precision**
  (fp32/fp16, plus English int8 behind a lower-accuracy warning — ADR-39).
- A standalone **settings window** (`layanow settings`) for checkpoint,
  precision, low-confidence threshold, model residency, and probability-bar
  colours; it writes `~/.config/layanow/config.toml` and applies live (ADR-38).
- A **click-through** overlay that only grabs the keyboard while shown, with
  ranked results and probability colours (ADR-14/25/31).
- An optional **quiz mode** (ADR-44): highlight a whole quiz — question plus
  options — once, and it is split into the question and lettered `A.`/`B.`/…
  options.

Not yet: X11 and Windows/macOS selection backends, accessibility and OCR
resolvers, and multi-answer support.

```sh
nix develop
cargo run -p layanow-app            # resident applet; overlay starts hidden
# in another terminal, or a compositor bind:
cargo run -p layanow-app -- toggle  # show/hide · also: show, hide, quit
cargo run -p layanow-app -- settings  # open the settings window
# highlight text (or type), Tab to add · Enter to decide · Esc: hide
# quiz mode: highlight the whole quiz once, then Enter
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
| GitHub Issues | Living task tracker (`area/*` labels, `M0`–`M10` milestones) |

## Development environment (Nix)

`flake.nix` pins a nixpkgs revision for a reproducible toolchain and provides:

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

## Building without Nix

`rust-toolchain.toml` pins the toolchain for [rustup](https://rustup.rs); install
it, then the native dependencies below. `ort` uses its `load-dynamic` feature, so
the build does not link ONNX Runtime — you supply the shared library at runtime
via `ORT_DYLIB_PATH`.

- **Build:** `pkg-config`, `cmake`, a C toolchain, OpenSSL headers, the
  Wayland/EGL/GL stack (`wayland`, `wayland-protocols`, `libxkbcommon`,
  `libEGL`, `libGL`), the X11 stack (`libX11`, `libXcursor`, `libXi`,
  `libXrandr`, `libXinerama`), and **ONNX Runtime 1.24.x**.
- **Runtime:** `libonnxruntime`, the Wayland/EGL/GL libraries, and `libdbus`
  (for the native file picker). The overlay and PRIMARY capture need a
  wlroots-style compositor with `wlr-layer-shell` and `ext-data-control`.

Debian/Ubuntu (names vary by distro):

```sh
sudo apt install build-essential pkg-config cmake libssl-dev \
  libwayland-dev wayland-protocols libxkbcommon-dev libegl-dev libgl-dev \
  libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxinerama-dev \
  libdbus-1-dev
```

Install ONNX Runtime 1.24.x from
<https://github.com/microsoft/onnxruntime/releases> (or your distro), then:

```sh
export ORT_DYLIB_PATH=/path/to/libonnxruntime.so
cargo build --release -p layanow-app
# first run: allow the one-time model download (~1.7 GB, English fp32)
LAYANOW_ALLOW_MODEL_DOWNLOAD=1 ./target/release/layanow
```

The Wayland/EGL/GL libraries and `libdbus` are opened at runtime, so they must be
on the loader path (set `LD_LIBRARY_PATH` if they are not in the default
locations). The overlay host is Linux/Wayland-only today; other platforms build
but have no overlay.

## Quickstart

```sh
nix develop
cargo run -p layanow-app          # starts the tray applet, loads the model
# hotkey opens the overlay
```

Hotkey: on Wayland a compositor bind runs `layanow toggle` (Wayland has no
app-level global hotkeys). On Windows/macOS the applet registers it.

## Text capture (highlight buffer)

The text source is the OS **selection buffer** — on Wayland the **PRIMARY**
highlight, never the clipboard (ADR-5/13). Highlight text with the **mouse** in
the target app and press **`Tab`** in the overlay to add it (first = question,
rest = answers). You can also type an item and press `Tab`.

Browsers and most toolkits publish a highlight buffer; some apps do not. To
check an app before running the applet, highlight text and run:

```sh
nix develop -c wl-paste -p     # prints the highlight, or nothing
```

The standalone probe is
`cargo run -p layanow-platform --example read_primary`. Run the applet with
`LAYANOW_LOG=layanow=debug` (or the `LAYANOW_DEBUG=1` shorthand) to log capture
outcomes (lengths only, never the text).

## Model smoke test

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

## License

`layanow` is released under the **MIT License** ([`LICENSE-MIT`](LICENSE-MIT)),
matching the SPDX expression in `Cargo.toml` (`MIT`). MIT is permissive, so the
applet can be used, modified, and redistributed freely, including commercially.
Every dependency is permissive and compatible; CI enforces the allowlist in
`deny.toml` with `cargo deny check licenses` (ADR-43).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project shall be licensed under the MIT License, without
any additional terms or conditions.

### Third-party notices

Dependencies keep their own licenses; we do not relicense them. When
distributing a binary, retain their copyright and attribution notices — in
particular the Apache-2.0 `NOTICE` files, the Unicode-3.0 license, and the
`epaint_default_fonts` fonts (OFL-1.1 / Ubuntu-font-1.0).

#### Model weights

The model weights are **not** distributed with this project (ADR-17); they are
downloaded from Hugging Face on first use. They remain under their own terms:

- Laya weights — © Convai Innovations, **Apache-2.0**
  (<https://huggingface.co/convaiinnovations/laya>).
- ONNX export — Receptron, **MIT**
  (<https://github.com/receptron/laya>).

The runtime prints this attribution (`layanow_model::bundle::ATTRIBUTION`).
