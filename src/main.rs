mod app;
mod config;
mod greetd;
mod logs;
mod theme;
mod ui;

use std::time::Duration;

use anyhow::Result;
use ansi_to_tui::IntoText;
use clap::Parser;
use futures::StreamExt;
use ratatui::crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::text::Text;

use crate::app::{Action, AppState, Effect};
use crate::config::{Cli, Config};
use crate::greetd::{spawn_mock, spawn_real, Channels};
use crate::logs::{spawn_logs, LogBuffer};
use crate::theme::Theme;
use crate::ui::Chrome;

enum Outcome {
    Launch,
    Quit,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load(&cli)?;
    run(&cli, cfg).await
}

async fn run(cli: &Cli, cfg: Config) -> Result<()> {
    let sock = std::env::var_os("GREETD_SOCK");

    // Real socket only when present AND not forced into demo; anything else runs
    // the mock, so launching from a plain terminal can never touch PAM.
    let (channels, demo) = match (&sock, cli.demo) {
        (Some(path), false) => (spawn_real(std::path::Path::new(path)).await?, false),
        (Some(_), true) => (spawn_mock(), true),
        (None, forced) => {
            if !forced {
                eprintln!("greeter: GREETD_SOCK unset — running in --demo mode (no real login).");
            }
            (spawn_mock(), true)
        }
    };

    let theme = Theme::resolve(cfg.accent, &cfg.overrides);
    let art = cfg.art.as_deref().map(parse_art);
    let app = AppState::new(
        cfg.idle_status.clone(),
        cfg.default_user.clone(),
        cfg.session_cmd.clone(),
        demo,
    );
    let log_rx = spawn_logs(cfg.log_cmd.clone());

    let chrome = Chrome {
        theme: &theme,
        art: art.as_ref(),
        disclaimer: cfg.disclaimer.as_deref(),
        show_help: cfg.show_help,
    };

    let mut terminal = ratatui::init();
    let outcome = event_loop(&mut terminal, app, channels, log_rx, &chrome).await;
    ratatui::restore();

    match outcome? {
        Outcome::Launch => {
            if demo {
                println!(
                    "[demo] authentication OK — would exec: {}",
                    cfg.session_cmd.join(" ")
                );
            }
            // Real mode: exiting hands the VT to greetd, which starts the session.
        }
        Outcome::Quit => {}
    }
    Ok(())
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    mut app: AppState,
    greetd: Channels,
    mut log_rx: tokio::sync::mpsc::Receiver<String>,
    chrome: &Chrome<'_>,
) -> Result<Outcome> {
    let mut greetd = greetd;
    let mut logs = LogBuffer::new(500);
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(120));

    loop {
        terminal.draw(|f| ui::draw(f, &app, &logs, chrome))?;

        tokio::select! {
            _ = ticker.tick() => {
                app.update(Action::Tick);
            }
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.kind == KeyEventKind::Release {
                            continue;
                        }
                        // Ctrl+C: hard quit in demo, ignored in real mode.
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && matches!(key.code, KeyCode::Char('c'))
                        {
                            if app.demo {
                                return Ok(Outcome::Quit);
                            }
                            continue;
                        }
                        if let Some(action) = map_key(key.code) {
                            let effects = app.update(action);
                            if let Some(outcome) = apply(effects, &greetd).await {
                                return Ok(outcome);
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return Ok(Outcome::Quit),
                }
            }
            maybe_resp = greetd.resp_rx.recv() => {
                if let Some(resp) = maybe_resp {
                    let effects = app.update(Action::Greetd(resp));
                    if let Some(outcome) = apply(effects, &greetd).await {
                        return Ok(outcome);
                    }
                }
            }
            maybe_line = log_rx.recv() => {
                if let Some(line) = maybe_line {
                    logs.push(line);
                }
            }
        }
    }
}

// ANSI art keeps its own colors; plain art falls back to an untinted block the
// UI colors with the theme's art color.
fn parse_art(s: &str) -> Text<'static> {
    s.into_text().unwrap_or_else(|_| Text::from(s.to_string()))
}

fn map_key(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Enter => Some(Action::Submit),
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Down | KeyCode::Up => Some(Action::FocusToggle),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Char(c) => Some(Action::Char(c)),
        _ => None,
    }
}

async fn apply(effects: Vec<Effect>, greetd: &Channels) -> Option<Outcome> {
    for effect in effects {
        match effect {
            Effect::Send(req) => {
                // greetd task gone => session unrecoverable; bail.
                if greetd.req_tx.send(req).await.is_err() {
                    return Some(Outcome::Quit);
                }
            }
            Effect::LaunchAndExit => return Some(Outcome::Launch),
            Effect::Quit => return Some(Outcome::Quit),
        }
    }
    None
}
