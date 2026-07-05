{
  description = "Luxel — FOSS live-codable LED controller (dev environment)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSystem = f:
        nixpkgs.lib.genAttrs systems (system:
          f (import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          }));
    in
    {
      devShells = forEachSystem (pkgs:
        let
          # ---- Xtensa toolchain (classic ESP32, e.g. Pixelblaze v3) ----
          # Espressif's Rust fork + GNU linker, packaged as derivations from
          # the same release artifacts espup would install, patched by
          # autoPatchelfHook at build time. x86_64-linux only for now (the
          # fixed-output hashes below are per-artifact; add other systems'
          # artifacts if ever needed).
          xtensaRustVersion = "1.95.0.0";
          xtensaRust = pkgs.stdenv.mkDerivation {
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
              ./rust-*-x86_64-unknown-linux-gnu/install.sh --destdir=$out --prefix="" --disable-ldconfig
              ./rust-src-*/install.sh --destdir=$out --prefix="" --disable-ldconfig
              runHook postInstall
            '';
            dontStrip = true;
          };
          xtensaGcc = pkgs.stdenv.mkDerivation {
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
          ++ pkgs.lib.optional isX86Linux xtensaGcc;
          shellHook = pkgs.lib.optionalString isX86Linux ''
            export XTENSA_RUST_HOME=${xtensaRust}
          '';
        };
      });

      formatter = forEachSystem (pkgs: pkgs.nixfmt-rfc-style);
    };
}
