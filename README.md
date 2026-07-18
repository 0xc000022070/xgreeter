# 0xc000022070's greeter

A ctOS-flavored [ratatui](https://ratatui.rs) login frontend for
[greetd](https://git.sr.ht/~kennylevinsen/greetd): background art, centered
`USER`/`PASS` fields, a `LOGGING AS <you>` bar, an optional disclaimer, and a
live system-init log ticker in the footer.

It is *only* a frontend — auth, PAM, and session launch stay inside greetd. This
binary just speaks greetd's IPC over `$GREETD_SOCK`.

## Try it safely

```sh
cargo run -- --demo
```

`--demo` (auto-selected when `$GREETD_SOCK` is unset) runs an in-process mock
greetd: it never touches PAM and never launches a session. Password `demo` (or
empty) succeeds; anything else shows the failure state. `Tab`/`↑↓` switch field,
`Enter` authenticates, `Esc`/`Ctrl-C` quits (demo only — the real greeter never
self-exits).

## Configure

Everything is config-driven; `greeter.example.toml` documents every key. The
knobs: `session_cmd`, `default_user`, `idle_status`, `log_cmd`, `accent`
(amber/blue/green/mono), per-color hex `[colors]`, `show_help`, `art`/`art_path`
(ASCII or ANSI — a fastfetch/neofetch dump works, omit for none), and
`disclaimer`/`disclaimer_path`.

Fonts aren't an app setting — a TUI draws with the console/VT font. On NixOS set
`console.font` (pick one with box-drawing + block coverage, e.g. Terminus).

## Nix

The flake exposes `packages.<system>.greeter`, an `overlays.default`, and a
home-manager module that generates the config and puts the binary on `PATH`:

```nix
# flake inputs: greeter.url = "path:./greeter";  (or your fork's URL)
imports = [ inputs.greeter.homeModules.default ];

programs.greeter = {
  enable = true;
  accent = "amber";
  sessionCmd = [ "start-hyprland" ];
  disclaimer = "authorized access only";
  colors.accent = "#ffb000";
};
```

Wiring into greetd is a *system* change — do it only after `--demo` looks right,
and keep the previous NixOS generation for one-click rollback. The module's
`configFile` output is a world-readable store path, so the greetd user (not your
login user) can read it. Reference it from system config and grant journal
access for the footer:

```nix
# greeter pkg via overlays.default; cfg = the HM user's programs.greeter config,
# e.g. config.home-manager.users.you.programs.greeter
users.users.greeter.extraGroups = [ "systemd-journal" ];
services.greetd.settings.default_session.command =
  "${pkgs.greeter}/bin/greeter --config ${cfg.configFile}";
```

Before switching the display manager, dry-run on a spare VT (Ctrl-Alt-F3):

```sh
GREETD_SOCK=/run/greetd.sock greeter --config <path>
```

## Test

```sh
cargo test
```

Covers the pure auth reducer (happy path, wrong password, retry, cancel, stray
responses), the mock PAM mapping, the log ring buffer, and `TestBackend` render
snapshots — including that the password is never rendered in plaintext.

The reducer (`app.rs`) is pure `AppState + Action -> Effect` with no IO; the real
socket task and the mock share one channel interface, so demo, tests, and
production run the same code path.
