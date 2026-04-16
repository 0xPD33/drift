{ self }:

{ config, lib, pkgs, ... }:

let
  cfg = config.services.drift;

  featuresConfig = ''
    [features]
    dispatch = ${lib.boolToString cfg.features.dispatch}
    commander = ${lib.boolToString cfg.features.commander}
    drivers = [${lib.concatMapStringsSep ", " (d: ''"${d}"'') cfg.features.drivers}]
  '';

  buildFeatures =
    lib.optional cfg.features.dispatch "dispatch"
    ++ lib.optional cfg.features.commander "commander";

  driftPackage = self.packages.${pkgs.system}.drift-cli.override (old: {
    cargoExtraArgs = (old.cargoExtraArgs or "--package drift-cli")
      + lib.optionalString (buildFeatures != [])
          " --features ${lib.concatStringsSep "," ([ "overview" ] ++ buildFeatures)}"
      + lib.optionalString (buildFeatures != []) " --no-default-features";
  });
in
{
  options.services.drift = {
    enable = lib.mkEnableOption "drift workspace manager daemon";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.default;
      description = "The drift CLI package.";
    };

    features = {
      dispatch = lib.mkEnableOption "task dispatch pipeline";

      commander = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Enable voice/LLM commander at runtime.";
      };

      drivers = lib.mkOption {
        type = lib.types.listOf (lib.types.enum [ "claude-code" "codex" ]);
        default = [ "claude-code" ];
        description = "Agent drivers to register at runtime.";
      };
    };

    persistWindows = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Persist window layout across workspace switches by default.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      description = "User account to run drift as.";
    };
  };

  config = lib.mkIf cfg.enable {
    # Write runtime feature config
    environment.etc."drift/config.toml".text = lib.mkAfter featuresConfig;

    systemd.user.services.drift-daemon = {
      description = "Drift workspace daemon";
      after = [ "graphical-session.target" ];
      wantedBy = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/drift daemon";
        Restart = "on-failure";
        RestartSec = 3;
      };
    };
  };
}
