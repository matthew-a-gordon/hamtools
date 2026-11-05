{
  description = "Rust TUIs for ham rig control and satellite work";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        # All workspace binaries (rig-tui).
        hamtools = pkgs.rustPlatform.buildRustPackage {
          pname = "hamtools";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          # Integration tests drive the dummy rig (`rigctld -m 1`).
          nativeCheckInputs = [ pkgs.hamlib_4 ];
        };
        default = hamtools;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          # Stdlib sources for rust-analyzer and IDEs (see .envrc).
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          packages = with pkgs; [
            cargo
            clippy
            rustc
            rustfmt
            rust-analyzer
            hamlib_4
          ];
        };
      });
    };
}
