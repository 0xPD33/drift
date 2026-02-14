{
  description = "Project-oriented workspace isolation for the Niri Wayland compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, flake-utils }:
    (flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          buildInputs = [ ];
          nativeBuildInputs = [ pkgs.pkg-config ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        drift-cli = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--package drift-cli";
        });
      in
      {
        packages = {
          default = drift-cli;
          inherit drift-cli;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rustc
            cargo
            pkg-config
            rust-analyzer
          ];
        };
      }))
    // {
      homeManagerModules.default = import ./nix/hm-module.nix { inherit self; };
    };
}
