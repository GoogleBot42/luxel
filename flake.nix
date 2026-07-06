{
  description = "Luxel — FOSS live-codable LED controller (dev environment + firmware images)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      lib = nixpkgs.lib;
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSystem = f:
        lib.genAttrs systems (system:
          f (import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          }));

      # ---- Xtensa toolchain (classic ESP32, e.g. Pixelblaze v3) ----
      # Espressif's Rust fork + GNU linker, packaged as derivations from the
      # same release artifacts espup would install, patched by
      # autoPatchelfHook at build time. x86_64-linux only for now (the
      # fixed-output hashes are per-artifact; add other systems' artifacts if
      # ever needed).
      xtensaRustVersion = "1.95.0.0";
      mkXtensaRust = pkgs:
        pkgs.stdenv.mkDerivation {
          pname = "xtensa-rust";
          version = xtensaRustVersion;
          srcs = [
            (pkgs.fetchurl {
              url = "https://github.com/esp-rs/rust-build/releases/download/v${xtensaRustVersion}/rust-${xtensaRustVersion}-x86_64-unknown-linux-gnu.tar.xz";
              hash = "sha256-qtL7JLrqtq1hxB8ALxNv4LQW7zmlAdKXUKtmwYpplDM=";
            })
            (pkgs.fetchurl {
              url = "https://github.com/esp-rs/rust-build/releases/download/v${xtensaRustVersion}/rust-src-${xtensaRustVersion}.tar.xz";
              hash = "sha256-cIvuM3rC1BwOhhr5MEe//skaBS+8tJV5JdhApB8ydxc=";
            })
          ];
          sourceRoot = ".";
          nativeBuildInputs = [ pkgs.autoPatchelfHook ];
          buildInputs = [ pkgs.zlib pkgs.stdenv.cc.cc.lib ];
          installPhase = ''
            runHook preInstall
            # explicit bash: the scripts' /usr/bin/env shebang doesn't exist
            # inside the build sandbox
            bash ./rust-*-x86_64-unknown-linux-gnu/install.sh --destdir=$out --prefix="" --disable-ldconfig
            bash ./rust-src-*/install.sh --destdir=$out --prefix="" --disable-ldconfig
            runHook postInstall
          '';
          dontStrip = true;
        };
      mkXtensaGcc = pkgs:
        pkgs.stdenv.mkDerivation {
          pname = "xtensa-esp-elf-gcc";
          version = "15.2.0_20250920";
          src = pkgs.fetchurl {
            url = "https://github.com/espressif/crosstool-NG/releases/download/esp-15.2.0_20250920/xtensa-esp-elf-15.2.0_20250920-x86_64-linux-gnu.tar.xz";
            hash = "sha256-49d60UVEgUUnu+ei0PeexFkqTiM5LFHHOIwOaGtqaXc=";
          };
          nativeBuildInputs = [ pkgs.autoPatchelfHook ];
          buildInputs = [ pkgs.zlib pkgs.stdenv.cc.cc.lib pkgs.gmp pkgs.mpfr pkgs.libmpc pkgs.isl ];
          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -r . $out/
            runHook postInstall
          '';
          dontStrip = true;
        };

      # ---- firmware image builder ----
      # nix build .#luxel-fw-pixelblaze-v3 → result/luxel-fw.elf (flash with
      # espflash) + result/luxel-fw.bin (merged full-flash image: bootloader
      # + partition table + app; `espflash write-bin 0 luxel-fw.bin`).
      # WiFi credentials are compile-time. A pure build bakes none (offline
      # render-only image). To bake credentials, pass them from the
      # environment with an impure eval:
      #   LUXEL_SSID=net LUXEL_PASS=secret nix build .#luxel-fw-pixelblaze-v3 --impure
      # (or use .override { ssid; pass; }). The credentials end up in the
      # image and in the world-readable nix store — don't build cred-baked
      # images on shared machines or share the resulting .bin.
      envOr = name: let v = builtins.getEnv name; in if v == "" then null else v;
      mkFirmware = pkgs:
        lib.makeOverridable
          ({ board
           , chip
           , target
           , buildStd ? false
           , ssid ? envOr "LUXEL_SSID"
           , pass ? envOr "LUXEL_PASS"
           }:
            let
              xtensaRust = mkXtensaRust pkgs;
              xtensaGcc = mkXtensaGcc pkgs;
              riscvRust = pkgs.rust-bin.stable.latest.default.override {
                targets = [ "riscv32imc-unknown-none-elf" ];
              };
              stdFlags = lib.optionalString buildStd " -Zbuild-std=core,alloc";
            in
            pkgs.stdenv.mkDerivation {
              pname = "luxel-fw-${board}";
              version = "0.1.0";
              src = lib.cleanSource ./.;

              cargoRoot = "firmware";
              # -Zbuild-std additionally resolves the std workspace's own
              # crates.io deps; firmware/rust-std.Cargo.lock is a pinned copy
              # of the toolchain's library/Cargo.lock (re-copy from
              # $XTENSA_RUST_HOME/lib/rustlib/src/rust/library/Cargo.lock on
              # toolchain bumps). Our lock must come first: the setup hook
              # validates the vendor dir's Cargo.lock against cargoRoot's.
              # name must stay "cargo-vendor-dir": importCargoLock's generated
              # .cargo/config.toml hardcodes that relative directory.
              cargoDeps = pkgs.symlinkJoin {
                name = "cargo-vendor-dir";
                paths = [
                  (pkgs.rustPlatform.importCargoLock { lockFile = ./firmware/Cargo.lock; })
                ] ++ lib.optional buildStd
                  (pkgs.rustPlatform.importCargoLock { lockFile = ./firmware/rust-std.Cargo.lock; });
              };

              nativeBuildInputs = [
                pkgs.rustPlatform.cargoSetupHook
                pkgs.espflash
              ] ++ (if buildStd then [ xtensaGcc ] else [ riscvRust ]);

              env = {
                LUXEL_SSID = lib.optionalString (ssid != null) ssid;
                LUXEL_PASS = lib.optionalString (pass != null) pass;
              } // lib.optionalAttrs buildStd {
                RUSTC = "${xtensaRust}/bin/rustc";
                RUSTDOC = "${xtensaRust}/bin/rustdoc";
              };

              buildPhase = ''
                runHook preBuild
                cd firmware
                ${if buildStd then "${xtensaRust}/bin/cargo" else "cargo"} build --release --offline \
                  --no-default-features --features ${board} \
                  --target ${target}${stdFlags}
                runHook postBuild
              '';

              installPhase = ''
                runHook preInstall
                mkdir -p $out
                cp target/${target}/release/luxel-fw $out/luxel-fw.elf
                # merged full-flash image (bootloader + OTA partition table +
                # app in the factory slot): espflash write-bin 0 …
                espflash save-image --chip ${chip} --merge \
                  --partition-table partitions.csv \
                  $out/luxel-fw.elf $out/luxel-fw.bin
                # app-only image for OTA: curl --data-binary @… /api/ota
                espflash save-image --chip ${chip} \
                  $out/luxel-fw.elf $out/luxel-fw-ota.bin
                runHook postInstall
              '';

              dontFixup = true;
            });

      firmwareVariants = {
        luxel-fw-c3-devkit = {
          board = "board-c3-devkit";
          chip = "esp32c3";
          target = "riscv32imc-unknown-none-elf";
        };
        luxel-fw-pixelblaze-v3 = {
          board = "board-pixelblaze-v3";
          chip = "esp32";
          target = "xtensa-esp32-none-elf";
          buildStd = true;
        };
        luxel-fw-athom-music = {
          board = "board-athom-music";
          chip = "esp32";
          target = "xtensa-esp32-none-elf";
          buildStd = true;
        };
      };
    in
    {
      devShells = forEachSystem (pkgs:
        let
          isX86Linux = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
        in
        {
          default = pkgs.mkShell {
            packages = [
              # Stable Rust with the wasm target (M1 browser playground) and IDE tooling.
              # ESP32-C3 (riscv32imc-unknown-none-elf) is a plain target here.
              (pkgs.rust-bin.stable.latest.default.override {
                extensions = [ "rust-src" "rust-analyzer" "clippy" ];
                # wasm32: browser playground; riscv32imc: ESP32-C3 firmware
                targets = [ "wasm32-unknown-unknown" "riscv32imc-unknown-none-elf" ];
              })
              # ESP32 flashing/monitoring over USB
              pkgs.espflash
              # Web IDE toolchain (M1)
              pkgs.nodejs_22
              # browser for driving/verifying the web IDE (puppeteer-core over CDP)
              pkgs.chromium
            ]
            # Classic-ESP32 (Xtensa) firmware toolchain; firmware/build-esp32.sh
            # picks it up via XTENSA_RUST_HOME + the gcc on PATH.
            ++ lib.optional isX86Linux (mkXtensaGcc pkgs);
            shellHook = lib.optionalString isX86Linux ''
              export XTENSA_RUST_HOME=${mkXtensaRust pkgs}
            '';
          };
        });

      packages = forEachSystem (pkgs:
        # Firmware images need the Xtensa artifacts (x86_64-linux hashes only).
        lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux")
          (lib.mapAttrs (_: v: mkFirmware pkgs v) firmwareVariants));

      formatter = forEachSystem (pkgs: pkgs.nixfmt-rfc-style);
    };
}
