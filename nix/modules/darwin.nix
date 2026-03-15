{ self, metalps }:
{
  config,
  lib,
  pkgs,
  system,
  ...
}:
let
  cfg = config.services.proc-siding;
  settingsFormat = pkgs.formats.toml { };
  configFile = settingsFormat.generate "proc-siding.toml" cfg.settings;
in
{
  options.services.proc-siding = {
    enable = lib.mkEnableOption "proc-siding GPU pressure monitor";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${system}.default;
      defaultText = lib.literalExpression
        "proc-siding.packages.\${system}.default";
      description = "The proc-siding package to use.";
    };

    logFile = lib.mkOption {
      type = lib.types.str;
      default = "/tmp/proc-siding.log";
      description = "Path for combined stdout/stderr log output.";
    };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      description = ''
        Configuration written verbatim to the proc-siding TOML config file.
        Mirrors the config.toml structure.  All of pressure, detector,
        process_discovery, and action sections are required.
      '';
      example = lib.literalExpression ''
        {
          pressure = {
            threshold = 25;
            hysteresis = 3;
            poll_interval_ms = 2000;
          };
          detector.kind = "metal";
          process_discovery = {
            kind = "process_name";
            pattern = "ollama";
          };
          action = {
            kind = "http_post";
            pressure_url = "http://127.0.0.1:9091/control/pause";
            clear_url = "http://127.0.0.1:9091/control/resume";
          };
        }
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # User-level agent: the Metal detector calls `metalps`, which requires
    # access to the logged-in user's GPU context.
    launchd.user.agents.proc-siding = {
      # metalps must be in PATH so the Metal detector can invoke it.
      path = [
        cfg.package
        metalps.packages.${system}.default
      ];

      serviceConfig = {
        ProgramArguments = [
          "${cfg.package}/bin/proc-siding"
          "--config"
          "${configFile}"
        ];
        RunAtLoad = true;
        KeepAlive = true;
        StandardOutPath = cfg.logFile;
        StandardErrorPath = cfg.logFile;
      };
    };
  };
}
