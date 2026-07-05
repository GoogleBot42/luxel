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
      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = [
            # Stable Rust with the wasm target (M1 browser playground) and IDE tooling.
            # Xtensa (ESP32/S3) needs Espressif's rustc fork; that toolchain will be
            # added when firmware work starts (M2) — likely via espup or nixpkgs-esp-dev.
            # ESP32-C3 (riscv32imc-unknown-none-elf) can be added here as a plain target.
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" "clippy" ];
              # wasm32: browser playground; riscv32imc: ESP32-C3 firmware
              targets = [ "wasm32-unknown-unknown" "riscv32imc-unknown-none-elf" ];
            })
            # Classic ESP32 (Xtensa — e.g. the Athom music-reactive WLED
            # controller) needs Espressif's rustc fork. One-time setup:
            #   espup install --targets esp32
            # then build with:
            #   cd firmware && rustup run esp cargo build --release \
            #     --no-default-features --features esp32 --target xtensa-esp32-none-elf
            pkgs.espup
            pkgs.rustup
            # ESP32 flashing/monitoring over USB
            pkgs.espflash
            # Web IDE toolchain (M1)
            pkgs.nodejs_22
            # browser for driving/verifying the web IDE (puppeteer-core over CDP)
            pkgs.chromium
          ];
        };
      });

      formatter = forEachSystem (pkgs: pkgs.nixfmt-rfc-style);
    };
}
