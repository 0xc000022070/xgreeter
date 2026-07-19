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

Everything is config-driven; `xgreeter.example.toml` documents every key. The
knobs: `session_cmd`, `default_user`, `idle_status`, `log_cmd`, `accent`
(amber/blue/green/mono), per-color hex `[colors]`, `show_help`, and
`disclaimer`/`disclaimer_path`.

With no `--config`, the greeter auto-detects its config from the first of
`$XDG_CONFIG_HOME/xgreeter/config.toml` (or `~/.config/xgreeter/config.toml`)
then `/etc/xgreeter/config.toml`. Pass `--config PATH` to override.

Fonts aren't an app setting — a TUI draws with the console/VT font. On NixOS set
`console.font` (pick one with box-drawing + block coverage, e.g. Terminus).

## ASCII animation

The background is always a procedural ASCII animation — currently a demoscene-style
flight down a spiraling ring-and-spoke tunnel that zooms and rotates in real time.
Every cell is a pure function of position and the frame clock — no frames to author,
motion continuous by construction. It shades with the active theme (dim → accent)
and fills the whole background; the login panel punches over the vanishing point.
It is permanent, background-only, and never touches the auth path.

Run it full-screen, hands-off and looping, with no login:

```sh
xgreeter --ascii-demo     # ESC/q to quit
```

## Nix

The flake exposes `packages.<system>.xgreeter`, an `overlays.default`, and a
**NixOS module** (`nixosModules.default`). greetd runs the greeter as a system
user before login, so it is system-scope, not home-manager: the module generates
a world-readable config store path, installs the binary, and can grant the
greetd user journal access for the footer.

```nix
# flake input: xgreeter.url = "github:0xc000022070/xgreeter";
imports = [ inputs.xgreeter.nixosModules.default ];

programs.xgreeter = {
  enable = true;
  sessionCmd = [ "${pkgs.hyprland}/bin/start-hyprland" ];
  accent = "amber";
  idleStatus = "AWAITING IDENTIFICATION";
  journalUser = "greeter";            # greetd's user; joins systemd-journal for logCmd
  colors.accent = "#ffb000";
};
```

The module installs the generated config to `/etc/xgreeter/config.toml`, which the
greeter auto-detects — so greetd execs the **bare binary**, no `--config` flag. The
module does **not** touch greetd itself; wiring the login path is your explicit,
boot-critical step. Do it only after `--demo` looks right, and keep the previous
NixOS generation for one-click rollback:

```nix
services.greetd.settings.default_session.command =
  "${lib.getExe config.programs.xgreeter.package}";
```

Before switching the display manager, dry-run on a spare VT (Ctrl-Alt-F3):

```sh
GREETD_SOCK=/run/greetd.sock xgreeter --config <path>
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
