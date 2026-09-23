{
  description = "layanow — local multiple-choice assistant";

  # Pinned to the same nixpkgs revision as the user's system flake
  # (~/projects/nix/flake.lock) so the toolchain matches the host.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/9dd5558b06dbdacbf635a3dd36dce1b1a7ee3a89";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      lib = pkgs.lib;

      # Start the AT-SPI accessibility bus if it is not already reachable.
      # The `accessibility` resolver depends on it (atspi crate / D-Bus).
      layanow-atspi = pkgs.writeShellScriptBin "layanow-atspi" ''
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

      # Libraries the binary links or dlopens at runtime: Wayland + EGL/GLES for
      # the overlay, xkbcommon for key handling, the X11 stack for the eframe
      # settings window, and ONNX Runtime (`ort` uses `load-dynamic`).
      runtimeLibs = with pkgs; [
        wayland
        libxkbcommon
        libGL
        mesa
        onnxruntime
        libX11
        libXcursor
        libXi
        libXrandr
        libXinerama
      ];

      # The workspace itself. Everything outside the crates and config is
      # excluded so the store path stays small.
      src = lib.cleanSourceWith {
        src = ./.;
        filter =
          path: _type:
          let
            base = baseNameOf (toString path);
          in
          !(builtins.elem base [
            "target"
            ".direnv"
            "result"
          ]);
      };

      layanow = pkgs.rustPlatform.buildRustPackage {
        pname = "layanow";
        version = "0.1.0";

        inherit src;

        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [
          pkg-config
          cmake
          makeWrapper
        ];
        buildInputs = with pkgs; [
          openssl
          wayland
          libxkbcommon
          libGL
          mesa
          libX11
          libXcursor
          libXi
          libXrandr
          libXinerama
        ];

        # Install the desktop entries + icon and point the binary at the Nix
        # ONNX Runtime and the runtime libraries above.
        postInstall = ''
          install -Dm644 ${./assets/layanow.desktop} \
            $out/share/applications/layanow.desktop
          install -Dm644 ${./assets/layanow-settings.desktop} \
            $out/share/applications/layanow-settings.desktop
          install -Dm644 ${./crates/layanow-platform/assets/layanow.png} \
            $out/share/icons/hicolor/32x32/apps/layanow.png

          wrapProgram $out/bin/layanow \
            --set ORT_DYLIB_PATH "${pkgs.onnxruntime}/lib/libonnxruntime.so" \
            --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"
        '';

        meta = {
          description = "Ask a local Laya decision model which answer is correct";
          longDescription = ''
            A cross-platform desktop applet: highlight a question and its
            candidate answers as native text (no OCR), and layanow asks a local
            Laya typed-decision model (ONNX Runtime) which answer is correct,
            then shows a ranked list with probability colours.
          '';
          homepage = "https://github.com/Applied-Cybernetic-Systems/layanow";
          license = with lib.licenses; [
            mit
            asl20
          ];
          mainProgram = "layanow";
          platforms = [ "x86_64-linux" ];
        };
      };
    in
    {
      packages.${system} = {
        default = layanow;
        inherit layanow;
      };

      apps.${system}.layanow = {
        type = "app";
        program = lib.getExe layanow;
      };

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

          # Wayland + GL (egui_glow, glutin, smithay-client-toolkit)
          wayland
          wayland-protocols
          libxkbcommon
          mesa
          libGL

          # Misc
          jq
          curl
          git
          layanow-atspi
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
          export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib:${pkgs.zlib}/lib:${pkgs.wayland}/lib:${pkgs.libxkbcommon}/lib:${pkgs.libGL}/lib:${pkgs.mesa}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

          # --- Wayland ------------------------------------------------------
          export WAYLAND_DISPLAY="''${WAYLAND_DISPLAY:-wayland-0}"

          # --- Accessibility -------------------------------------------------
          # Make the AT-SPI bus service discoverable and ensure it is running.
          export XDG_DATA_DIRS="${pkgs.at-spi2-core}/share:''${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
          unset NO_AT_BRIDGE
          layanow-atspi || true

          echo "layanow dev shell (Rust)"
          echo "  rustc : $(rustc --version)"
          echo "  ort   : $ORT_DYLIB_PATH"
          echo "  helper: layanow-atspi   # (re)start the accessibility bus"
        '';
      };
    };
}
