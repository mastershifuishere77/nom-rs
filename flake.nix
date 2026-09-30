{
  description = "nom-rs: nix-output-monitor rewritten in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      rec {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "nom-rs";
          version = "0.0.1";
          src = pkgs.lib.cleanSource ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.installShellFiles ];
          doCheck = false;
          postInstall = ''
            ln -sf nom "$out/bin/nom-build"
            ln -sf nom "$out/bin/nom-shell"
          '';
          meta = with pkgs.lib; {
            description = "nom-rs: nix-output-monitor rewritten in Rust";
            homepage = "https://github.com/mastershifuishere77/nom-rs";
            license = licenses.mit;
            mainProgram = "nom-rs";
          };
        };

        packages.nom-rs = packages.default;
        packages.nom = packages.default;

        packages.bench-stream-gen = pkgs.runCommand "bench-stream-gen" {
          nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
        } ''
          mkdir -p "$out/bin"
          rustc -O ${./benches/generate_stream.rs} -o "$out/bin/bench-stream-gen"
        '';

        packages.bench = pkgs.writeShellApplication {
          name = "nom-bench";
          runtimeInputs = [
            pkgs.hyperfine
            pkgs.time
            pkgs.gawk
            pkgs.coreutils
            pkgs.bash
          ];
          text = ''
            export NOM_RUST_BIN="${packages.default}/bin/nom"
            export NOM_HASKELL_BIN="${pkgs.nix-output-monitor}/bin/nom"
            export NOM_GEN_BIN="${packages.bench-stream-gen}/bin/bench-stream-gen"
            export NOM_TEST_DIR="${./test}"
            export REPO_ROOT="''${REPO_ROOT:-$PWD}"
            exec bash "${./benches/bench.sh}" "$@"
          '';
        };

        apps.default = flake-utils.lib.mkApp {
          drv = packages.default;
          name = "nom";
        };

        apps.bench = flake-utils.lib.mkApp {
          drv = packages.bench;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            rust-analyzer
            clippy
            rustfmt
            pkg-config
            nix
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          RUST_BACKTRACE = "1";
        };
      }
    ) // {
      overlays.default = final: prev: {
        nom-rs = self.packages.${final.system}.default;
        nom = self.packages.${final.system}.default;
        nix-output-monitor = self.packages.${final.system}.default;
      };
    };
}
