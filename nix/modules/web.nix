# NixOS module for the proc-siding-web service.
# Exported from the flake as nixosModules.web.
#
# Minimal usage in a NixOS configuration:
#
#   inputs.proc-siding.nixosModules.web
#
#   services.proc-siding-web = {
#     enable = true;
#     port   = 8080;
#   };
{ self }:
{ config, lib, pkgs, ... }:
let
  cfg = config.services.proc-siding-web;
in
{
  options.services.proc-siding-web = {
    enable = lib.mkEnableOption "proc-siding-web web service";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.web;
      defaultText = lib.literalExpression "self.packages.\${system}.web";
      description = "Package providing the service binary.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "IP address to bind to.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 3000;
      description = "TCP port to listen on.";
    };

    logLevel = lib.mkOption {
      type = lib.types.enum [ "trace" "debug" "info" "warn" "error" ];
      default = "info";
      description = "Tracing log verbosity level.";
    };

    logFormat = lib.mkOption {
      type = lib.types.enum [ "text" "json" ];
      default = "json";
      description = ''
        Log output format.  Use "text" for human-readable local logs and
        "json" for structured logs consumed by a log aggregator.
      '';
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "proc-siding-web";
      description = "System user account the service runs as.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "proc-siding-web";
      description = "System group the service runs as.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      description = "proc-siding-web service user";
    };

    users.groups.${cfg.group} = { };

    systemd.services.proc-siding-web = {
      description = "proc-siding-web web service";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      environment = {
        LOG_LEVEL = cfg.logLevel;
        LOG_FORMAT = cfg.logFormat;
      };

      serviceConfig = {
        # Type = notify causes systemd to wait for the binary to call
        # sd_notify(READY=1) before marking the unit active.  The binary
        # does this via the sd-notify crate immediately after the TCP
        # listener is bound.  NotifyAccess = main restricts who may send
        # notifications to the main process only.
        Type = "notify";
        NotifyAccess = "main";

        # Restart if no WATCHDOG=1 heartbeat arrives within 30 s.  The
        # binary reads WATCHDOG_USEC and pings at half this interval (15 s).
        # Override via systemd.services.proc-siding-web.serviceConfig.WatchdogSec.
        WatchdogSec = lib.mkDefault "30s";

        ExecStart = "${cfg.package}/bin/proc-siding-web"
          + " --host ${cfg.host}"
          + " --port ${toString cfg.port}";

        User = cfg.user;
        Group = cfg.group;
        Restart = "on-failure";
        RestartSec = "5s";

        # Harden the service environment.
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
      };
    };
  };
}
