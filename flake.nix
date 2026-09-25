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
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.installShellFiles ];
          preCheck =
            let
              integration-test-builds = import ./test/integration/all.nix;
            in
            ''
              # Make sure integration-tests runtime and buildtime paths are available
              # ${toString integration-test-builds}
            '';
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
