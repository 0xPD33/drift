{ self }:

{ config, lib, pkgs, ... }:

let
  cfg = config.programs.drift;
  qmlDir = "${self}/drift-shell";

  featuresConfig = ''
    [features]
    dispatch = ${lib.boolToString cfg.features.dispatch}
    commander = ${lib.boolToString cfg.features.commander}
    drivers = [${lib.concatMapStringsSep ", " (d: ''"${d}"'') cfg.features.drivers}]
  '';
in
{
  options.programs.drift = {
    enable = lib.mkEnableOption "drift workspace manager";

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

    shell.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Deploy drift QML components to ~/.config/quickshell/drift/.

        These components must be manually imported into your shell.qml.
        Add the following to integrate drift into your QuickShell setup:

        First, add the import at the top of your shell.qml:

           import "drift" as Drift

        1. Instantiate the state object in your ShellRoot. IMPORTANT: use
           "property var" to avoid a binding loop where the Bar's driftState
           property resolves to itself:

           property var driftState: Drift.DriftState {}

           Then pass it to child components as root.driftState.

        2. Add toast notifications (standalone overlay panel):

           Drift.DriftToastManager {
               driftState: root.driftState
               bgColor: root.bgColor
               bgSecondary: root.bgSecondary
               textColor: root.textColor
               textDim: root.textDim
           }

        3. Add the side panel (toggle with driftPanel.toggle()).
           DriftPanel is a QtObject containing both a backdrop (click-outside-
           to-dismiss) and the panel window:

           Drift.DriftPanel {
               id: driftPanel
               driftState: root.driftState
               bgColor: root.bgColor
               bgSecondary: root.bgSecondary
               textColor: root.textColor
               textDim: root.textDim
               accentColor: root.accentColor
           }

        4. Add Drift.DriftStatus to your bar (also import "drift" as Drift
           in Bar.qml). Place it BEFORE workspace indicators in the RowLayout
           so it appears on the left:

           Drift.DriftStatus {
               driftState: bar.driftState
               bgColor: bar.bgColor
               bgSecondary: bar.bgSecondary
               textColor: bar.textColor
               textDim: bar.textDim
               accentColor: bar.accentColor
               onClicked: driftPanel.toggle()
           }

        All components accept standard theme colors (bgColor, bgSecondary,
        textColor, textDim, accentColor) so they match your existing shell.
      '';
    };

    daemon.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Run the drift daemon as a systemd user service.";
    };

    commander = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Run the drift commander (TTS announcements + voice control).";
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = self.packages.${pkgs.system}.drift-commander;
        description = "The drift-commander package.";
      };
    };

    llm = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Run a local llama-cpp server for voice command interpretation.";
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.llama-cpp;
        description = "The llama-cpp package providing llama-server.";
      };

      model = lib.mkOption {
        type = lib.types.str;
        description = "Path to the GGUF model file.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 8080;
        description = "Port for the llama-cpp server.";
      };

      contextSize = lib.mkOption {
        type = lib.types.int;
        default = 2048;
        description = "Context size for the model.";
      };

      gpuLayers = lib.mkOption {
        type = lib.types.int;
        default = 99;
        description = "Number of layers to offload to GPU (-1 for all).";
      };

      extraArgs = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Extra arguments passed to llama-server.";
      };
    };

    tts = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Run a TTS HTTP server for speech synthesis.";
      };

      command = lib.mkOption {
        type = lib.types.str;
        description = ''
          The command to start the TTS server.
          It should listen on the port specified by programs.drift.tts.port.
        '';
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 8880;
        description = "Port the TTS server listens on.";
      };

      environment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = {};
        description = "Extra environment variables for the TTS service.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    # Write [features] section into drift's config.toml for runtime gating
    xdg.configFile."drift/config.toml".text = lib.mkAfter featuresConfig;

    # Deploy QML files into ~/.config/quickshell/drift/
    xdg.configFile = lib.mkIf cfg.shell.enable {
      "quickshell/drift/qmldir".source = "${qmlDir}/qmldir";
      "quickshell/drift/DriftState.qml".source = "${qmlDir}/DriftState.qml";
      "quickshell/drift/DriftStatus.qml".source = "${qmlDir}/DriftStatus.qml";
      "quickshell/drift/DriftPanel.qml".source = "${qmlDir}/DriftPanel.qml";
      "quickshell/drift/DriftToast.qml".source = "${qmlDir}/DriftToast.qml";
      "quickshell/drift/DriftToastManager.qml".source = "${qmlDir}/DriftToastManager.qml";
    };

    # --- Systemd user services ---

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

    systemd.user.services.drift-llm = lib.mkIf cfg.llm.enable {
      Unit = {
        Description = "Drift LLM server (llama-cpp)";
        After = [ "network.target" ];
      };
      Service = {
        ExecStart = lib.concatStringsSep " " ([
          "${cfg.llm.package}/bin/llama-server"
          "--model" cfg.llm.model
          "--port" (toString cfg.llm.port)
          "--ctx-size" (toString cfg.llm.contextSize)
          "--n-gpu-layers" (toString cfg.llm.gpuLayers)
        ] ++ cfg.llm.extraArgs);
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install = {
        WantedBy = [ "default.target" ];
      };
    };

    systemd.user.services.drift-tts = lib.mkIf cfg.tts.enable {
      Unit = {
        Description = "Drift TTS server";
        After = [ "network.target" ];
      };
      Service = {
        ExecStart = cfg.tts.command;
        Restart = "on-failure";
        RestartSec = 5;
        Environment = lib.mapAttrsToList (k: v: "${k}=${v}") cfg.tts.environment;
      };
      Install = {
        WantedBy = [ "default.target" ];
      };
    };

    systemd.user.services.drift-commander = lib.mkIf cfg.commander.enable {
      Unit = {
        Description = "Drift commander (TTS announcements + voice control)";
        After = [ "drift-daemon.service" ]
          ++ lib.optional cfg.llm.enable "drift-llm.service"
          ++ lib.optional cfg.tts.enable "drift-tts.service";
        Requires = [ "drift-daemon.service" ];
        Wants = lib.optional cfg.llm.enable "drift-llm.service"
          ++ lib.optional cfg.tts.enable "drift-tts.service";
      };
      Service = {
        ExecStart = "${cfg.commander.package}/bin/drift-commander";
        Restart = "on-failure";
        RestartSec = 3;
        Environment = [
          "ALSA_PLUGIN_DIR=${pkgs.pipewire}/lib/alsa-lib"
          "ORT_DYLIB_PATH=${pkgs.onnxruntime}/lib/libonnxruntime.so"
        ];
      };
      Install = {
        WantedBy = [ "graphical-session.target" ];
      };
    };
  };
}
