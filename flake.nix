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
          buildInputs = [ pkgs.openssl pkgs.portaudio pkgs.onnxruntime pkgs.alsa-lib ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
          ORT_PREFER_DYNAMIC_LINK = "1";
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        drift-cli = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--package drift-cli";
          doCheck = false;
        });

        drift-commander = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "--package drift-commander";
          doCheck = false;
        });

        overviewCargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          cargoExtraArgs = "--package drift-cli --no-default-features --features overview";
        });

        drift-overview = craneLib.buildPackage (commonArgs // {
          cargoArtifacts = overviewCargoArtifacts;
          cargoExtraArgs = "--package drift-cli --no-default-features --features overview";
          doCheck = false;
        });
      in
      {
        packages = {
          default = drift-cli;
          inherit drift-cli drift-commander drift-overview;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rustc
            cargo
            pkg-config
            rust-analyzer
            portaudio
            onnxruntime
            alsa-lib
            pipewire
            openssl
          ];
          ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
          ORT_PREFER_DYNAMIC_LINK = "1";
          ALSA_PLUGIN_DIR = "${pkgs.pipewire}/lib/alsa-lib";
        };
      }))
    // {
      homeManagerModules.default = import ./nix/hm-module.nix { inherit self; };
      nixosModules.drift = import ./nix/nixos-module.nix { inherit self; };
    };
}
