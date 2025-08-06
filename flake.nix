{
  description = "Development environment for checkle";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "checkle";
          version = "0.2.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs =
            with pkgs;
            [ ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
            ];

          # Add compression tools for integration tests that create/extract archives
          nativeCheckInputs = with pkgs; [
            zip
            unzip # ZIP archive tools
            gzip # For .gz compression
            bzip2 # For .bz2 compression
            xz # For .xz compression
            zstd # For .zst compression (future-proofing)
            # tar is already in coreutils
          ];

          # Skip performance tests that are unreliable in CI/container environments
          checkPhase = ''
            cargo test --release -- --skip test_performance_characteristics --skip test_parallel_performance_improvement
          '';
        };

        devShells.default = pkgs.mkShell {
          buildInputs =
            with pkgs;
            [
              # Install rustup to manage toolchains properly
              rustup
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
              coreutils # includes md5sum, sha256sum, etc.
              rhash # includes multiple hash algorithms
              xxHash # extremely fast hash algorithm
              b3sum # BLAKE3 hash (very fast)

              # Archive and compression tools for development and testing
              zip
              unzip # ZIP archive tools
              gzip # For .gz compression
              bzip2 # For .bz2 compression
              xz # For .xz compression
              zstd # For .zst compression (future-proofing)
              # tar is already in coreutils

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
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
            ];

          shellHook = ''
            echo "checkle development environment"

            # Set up rustup with stable and nightly toolchains
            export RUSTUP_HOME=$PWD/.rustup
            export CARGO_HOME=$PWD/.cargo
            export PATH=$CARGO_HOME/bin:$PATH

            if [ ! -d "$RUSTUP_HOME" ]; then
              echo "Setting up Rust toolchains..."
              rustup default stable
              rustup toolchain install nightly
              rustup component add rust-src rustfmt clippy rust-analyzer --toolchain stable
              rustup component add rust-src rustfmt clippy rust-analyzer --toolchain nightly
            fi

            echo "Rust toolchains available:"
            echo "  - Stable: cargo (default)"
            echo "  - Nightly: cargo +nightly"
            echo "Run 'just' to see available commands"
          '';

          RUST_BACKTRACE = 0;
        };
      }
    );
}
