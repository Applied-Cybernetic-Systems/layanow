{
  description = "layassist — Rust dev environment";

  # Pinned to the same nixpkgs revision as the user's system flake
  # (~/projects/nix/flake.lock) so the toolchain matches the host.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/9dd5558b06dbdacbf635a3dd36dce1b1a7ee3a89";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };

      # Start the AT-SPI accessibility bus if it is not already reachable.
      # The `accessibility` resolver depends on it (atspi crate / D-Bus).
      layassist-atspi = pkgs.writeShellScriptBin "layassist-atspi" ''
        set -euo pipefail
        probe() {
          ${pkgs.dbus}/bin/dbus-send --session --print-reply \
            --dest=org.a11y.Bus /org/a11y/bus org.a11y.Bus.GetAddress \
            >/dev/null 2>&1
        }
        if probe; then
          echo "AT-SPI bus already available"
          exit 0
        fi
        ${pkgs.at-spi2-core}/libexec/at-spi-bus-launcher \
          --launch-immediately --a11y=1 >/dev/null 2>&1 &
        for _ in 1 2 3 4 5 6 7 8 9 10; do
          sleep 0.3
          probe && { echo "AT-SPI bus started"; exit 0; }
        done
        echo "warning: could not start the AT-SPI bus" >&2
        exit 1
      '';
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          # Rust toolchain
          cargo
          rustc
          clippy
          rustfmt
          rust-analyzer
          # Quality tooling
          cargo-deny
          cargo-audit
          cargo-nextest
          taplo

          # Native build helpers
          pkg-config
          cmake
          openssl

          # Build-time only: ONNX export + dynamic int8 quantization (ADR-9).
          # Python never runs in the app; see tools/export/.
          python312
          uv

          # Model runtime (ort crate loads this; see ORT_DYLIB_PATH below)
          onnxruntime

          # Accessibility (atspi crate speaks D-Bus directly)
          at-spi2-core
          dbus

          # Selection / clipboard
          wl-clipboard
          wtype

          # Wayland + GPU (egui/eframe, winit, smithay-client-toolkit)
          wayland
          wayland-protocols
          libxkbcommon
          vulkan-loader
          mesa
          libGL

          # Misc
          jq
          curl
          git
          layassist-atspi
        ];

        shellHook = ''
          # --- ONNX Runtime (ort crate) -------------------------------------
          # Prefer `ort`'s `load-dynamic` feature; point it at the nix lib.
          export ORT_DYLIB_PATH="${pkgs.onnxruntime}/lib/libonnxruntime.so"
          # Fallback for link-time builds (ORT_STRATEGY=system).
          export ORT_STRATEGY="system"
          export ORT_LIB_LOCATION="${pkgs.onnxruntime}"

          # --- Native runtime libraries -------------------------------------
          # `wayland-sys`/`glutin` dlopen libwayland/libEGL at runtime, and
          # pip-installed torch/onnxruntime wheels need the C++ runtime and
          # zlib; Nix does not put any of these on the loader path by default.
          export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.zlib}/lib:${pkgs.wayland}/lib:${pkgs.libxkbcommon}/lib:${pkgs.libGL}/lib:${pkgs.mesa}/lib:${pkgs.vulkan-loader}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

          # --- Wayland / GPU ------------------------------------------------
          export WAYLAND_DISPLAY="''${WAYLAND_DISPLAY:-wayland-0}"
          export WGPU_BACKEND="''${WGPU_BACKEND:-vulkan}"

          # --- Accessibility -------------------------------------------------
          # Make the AT-SPI bus service discoverable and ensure it is running.
          export XDG_DATA_DIRS="${pkgs.at-spi2-core}/share:''${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
          unset NO_AT_BRIDGE
          layassist-atspi || true

          echo "layassist dev shell (Rust)"
          echo "  rustc : $(rustc --version)"
          echo "  ort   : $ORT_DYLIB_PATH"
          echo "  helper: layassist-atspi   # (re)start the accessibility bus"
        '';
      };
    };
}
