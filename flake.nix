{
  description = "Development environment for checkle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        rustStable = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" "rust-analyzer" ];
          targets = [
            "x86_64-unknown-linux-gnu"
            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-gnu"
            "aarch64-unknown-linux-musl"
            "x86_64-apple-darwin"
            "aarch64-apple-darwin"
            "x86_64-pc-windows-msvc"
            "aarch64-pc-windows-msvc"
          ];
        };
        
        rustNightly = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" "rust-analyzer" ];
          targets = [
            "x86_64-unknown-linux-gnu"
            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-gnu"
            "aarch64-unknown-linux-musl"
            "x86_64-apple-darwin"
            "aarch64-apple-darwin"
            "x86_64-pc-windows-msvc"
            "aarch64-pc-windows-msvc"
          ];
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "checkle";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
          ];
        };
        
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustStable
            rustNightly
            pkg-config
            just
            pre-commit
            cargo-watch
            cargo-edit
            cargo-outdated
            cargo-audit
            cargo-binstall
            cargo-zigbuild
            zig
            
            # Benchmarking
            hyperfine
            
            # Python for benchmark visualization
            python313
            uv
            
            # Checksum utilities for benchmarking
            coreutils  # includes md5sum, sha256sum, etc.
            rhash      # includes multiple hash algorithms
            xxHash     # extremely fast hash algorithm
            b3sum      # BLAKE3 hash (very fast)
            
            # Search and navigation tools
            ripgrep
            fzf
            tree
            
            # TOML formatter
            taplo
            
            # Documentation
            mdbook
            
            # For direnv
            direnv
            nix-direnv
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
          ];

          shellHook = ''
            echo "checkle development environment"
            echo "Rust toolchains available:"
            echo "  - Stable: rustc (default)"
            echo "  - Nightly: rustc +nightly"
            echo "Run 'just' to see available commands"
          '';

          RUST_BACKTRACE = 1;
        };
      });
}