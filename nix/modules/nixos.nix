{ self }:
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
          detector.kind = "nvidia";
          process_discovery = {
            kind = "systemd_unit";
            unit = "ollama.service";
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
    # Run as root to read /proc/<pid>/fd for GPU device open-file detection.
    systemd.services.proc-siding = {
      description = "proc-siding GPU pressure monitor";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network.target"
        "ollama.service"
      ];

      serviceConfig = {
        Type = "simple";
        User = "root";
        ExecStart = "${cfg.package}/bin/proc-siding --config ${configFile}";
        Restart = "on-failure";
        RestartSec = "5s";
      };
    };
  };
}
