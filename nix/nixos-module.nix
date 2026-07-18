self: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.xgreeter;
  toml = pkgs.formats.toml {};

  colorOpt = name:
    lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Hex `#rrggbb` override for the ${name} color.";
    };

  # Drop null keys so the greeter falls back to its built-in defaults.
  prune = lib.filterAttrs (_: v: v != null);

  colors = prune {
    accent = cfg.colors.accent;
    on_accent = cfg.colors.onAccent;
    foreground = cfg.colors.foreground;
    dim = cfg.colors.dim;
    art = cfg.colors.art;
    error = cfg.colors.error;
    background = cfg.colors.background;
    field_background = cfg.colors.fieldBackground;
  };

  settings =
    prune {
      session_cmd = cfg.sessionCmd;
      default_user = cfg.defaultUser;
      idle_status = cfg.idleStatus;
      log_cmd = cfg.logCmd;
      accent = cfg.accent;
      show_help = cfg.showHelp;
      art = cfg.art;
      art_path = cfg.artPath;
      disclaimer = cfg.disclaimer;
      disclaimer_path = cfg.disclaimerPath;
    }
    // lib.optionalAttrs (colors != {}) {inherit colors;};
in {
  options.programs.xgreeter = {
    enable = lib.mkEnableOption "0xc000022070's greeter";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.xgreeter;
      defaultText = lib.literalExpression "xgreeter.packages.\${system}.xgreeter";
      description = "The greeter package to install.";
    };

    journalUser = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "greeter";
      description = ''
        User greetd runs the greeter as. When set, it is added to the
        `systemd-journal` group so the default `logCmd` (journalctl) can read the
        journal. The user must already exist (greetd creates `greeter`). Null
        leaves group membership untouched.
      '';
    };

    sessionCmd = lib.mkOption {
      type = lib.types.nullOr (lib.types.listOf lib.types.str);
      default = null;
      example = ["start-hyprland"];
      description = "argv greetd execs as the session after auth. Not a shell string.";
    };

    defaultUser = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Prefilled username. Empty string starts the caret on the USER field.";
    };

    idleStatus = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "AWAITING IDENTIFICATION";
      description = "Standalone status-bar prompt shown before a username is typed. Once one is typed the bar switches to `LOGGING AS <user>`.";
    };

    logCmd = lib.mkOption {
      type = lib.types.nullOr (lib.types.listOf lib.types.str);
      default = null;
      example = ["journalctl" "-b" "-n" "40" "-f" "-o" "cat"];
      description = "argv whose stdout streams into the SYSTEM INIT footer.";
    };

    accent = lib.mkOption {
      type = lib.types.nullOr (lib.types.enum ["amber" "blue" "green" "mono"]);
      default = null;
      description = "Named base palette.";
    };

    showHelp = lib.mkOption {
      type = lib.types.nullOr lib.types.bool;
      default = null;
      description = "Show the idle key-hint line.";
    };

    art = lib.mkOption {
      type = lib.types.nullOr lib.types.lines;
      default = null;
      description = "Inline background art (ASCII or ANSI). `artPath` wins over this.";
    };

    artPath = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Path to a file whose contents become the art.";
    };

    disclaimer = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Inline blurb under the status bar.";
    };

    disclaimerPath = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Path to a file whose contents become the disclaimer.";
    };

    colors = {
      accent = colorOpt "status bar + focused borders";
      onAccent = colorOpt "text on the status bar";
      foreground = colorOpt "typed text";
      dim = colorOpt "hints, unfocused borders, footer";
      art = colorOpt "plain (non-ANSI) art tint";
      error = colorOpt "auth-failure text";
      background = colorOpt "canvas";
      fieldBackground = colorOpt "input interiors";
    };

    configFile = lib.mkOption {
      type = lib.types.path;
      readOnly = true;
      description = ''
        The generated config, as a world-readable /nix/store path. Point greetd
        at it, e.g. `services.greetd.settings.default_session.command =
          "''${lib.getExe config.programs.xgreeter.package} --config ''${config.programs.xgreeter.configFile}"`.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [cfg.package];
    programs.xgreeter.configFile = toml.generate "greeter.toml" settings;

    users.users = lib.optionalAttrs (cfg.journalUser != null) {
      ${cfg.journalUser}.extraGroups = ["systemd-journal"];
    };
  };
}
