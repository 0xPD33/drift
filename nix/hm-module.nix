{ self }:

{ config, lib, pkgs, ... }:

let
  cfg = config.programs.drift;
  qmlDir = "${self}/drift-shell";
in
{
  options.programs.drift = {
    enable = lib.mkEnableOption "drift workspace manager";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.default;
      description = "The drift package to use.";
    };

    shell.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Deploy QuickShell QML components.";
    };

    daemon.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Run the drift daemon as a systemd user service.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    # Deploy QML files into ~/.config/quickshell/
    xdg.configFile = lib.mkIf cfg.shell.enable {
      "quickshell/DriftState.qml".source = "${qmlDir}/DriftState.qml";
      "quickshell/DriftStatus.qml".source = "${qmlDir}/DriftStatus.qml";
      "quickshell/DriftPanel.qml".source = "${qmlDir}/DriftPanel.qml";
      "quickshell/DriftToast.qml".source = "${qmlDir}/DriftToast.qml";
      "quickshell/DriftToastManager.qml".source = "${qmlDir}/DriftToastManager.qml";
    };

    # Systemd user service for the daemon
    systemd.user.services.drift-daemon = lib.mkIf cfg.daemon.enable {
      Unit = {
        Description = "Drift workspace daemon";
        After = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/drift daemon";
        Restart = "on-failure";
        RestartSec = 3;
      };
      Install = {
        WantedBy = [ "graphical-session.target" ];
      };
    };
  };
}
