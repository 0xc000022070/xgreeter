use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AppState, Field, Phase};
use crate::logs::LogBuffer;
use crate::theme::Theme;

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const MASK: char = '•';
const FOOTER_H: u16 = 4;
const DISCLAIMER_MAX_H: u16 = 8;

pub struct Chrome<'a> {
    pub theme: &'a Theme,
    pub art: Option<&'a Text<'a>>,
    pub disclaimer: Option<&'a str>,
    pub show_help: bool,
}

pub fn draw(f: &mut Frame, app: &AppState, logs: &LogBuffer, chrome: &Chrome) {
    let area = f.area();
    let theme = chrome.theme;

    f.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let [stage, footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(FOOTER_H)]).areas(area);

    if let Some(art) = chrome.art {
        render_art(f, stage, art, theme);
    }
    render_login(f, stage, app, chrome);
    render_footer(f, footer, logs, theme);
}

/// User-supplied art, centered in the stage as a background layer. The login
/// panel is opaque and punches over its middle, matching the reference look.
fn render_art(f: &mut Frame, stage: Rect, art: &Text, theme: &Theme) {
    let rect = centered(stage, stage.width, stage.height.saturating_sub(2));
    f.render_widget(
        Paragraph::new(art.clone())
            .style(Style::default().fg(theme.art))
            .alignment(Alignment::Center),
        rect,
    );
}

fn render_login(f: &mut Frame, stage: Rect, app: &AppState, chrome: &Chrome) {
    let theme = chrome.theme;
    let width = 72u16.min(stage.width.saturating_sub(6)).max(36);

    // Show the status row for any non-idle phase, or when idle help is on.
    let show_status_row = chrome.show_help || !matches!(app.phase, Phase::Idle);
    let disc_h = chrome
        .disclaimer
        .map(|d| wrapped_height(d, width.saturating_sub(2)).min(DISCLAIMER_MAX_H));

    // Height: fields(3) + gap(1) + bar(1) [+ gap + status] [+ gap + disclaimer].
    let mut height = 5u16;
    if show_status_row {
        height += 2;
    }
    if let Some(h) = disc_h {
        height += 1 + h;
    }
    let area = centered(stage, width, height.min(stage.height));

    // Opaque panel: wipe art beneath, repaint the canvas bg.
    f.render_widget(Clear, area);
    f.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    // Optional rows make Layout awkward, so place rows top-down by hand.
    let mut y = area.y;
    let row = |y: u16, h: u16| Rect {
        x: area.x,
        y,
        width: area.width,
        height: h,
    };

    let fields = row(y, 3);
    y += 3 + 1;
    let bar = row(y, 1);
    y += 1;

    render_fields(f, fields, app, theme);
    render_bar(f, bar, app, theme);

    if show_status_row {
        y += 1;
        let (text, style) = status_line(app, theme, chrome.show_help);
        f.render_widget(
            Paragraph::new(text).alignment(Alignment::Center).style(style),
            row(y, 1),
        );
        y += 1;
    }

    if let (Some(text), Some(h)) = (chrome.disclaimer, disc_h) {
        y += 1;
        f.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme.dim)),
            row(y, h),
        );
    }
}

fn render_fields(f: &mut Frame, fields: Rect, app: &AppState, theme: &Theme) {
    let [user_area, _gutter, pass_area] = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .areas(fields);

    let caret_on = app.editable() && (app.tick / 5).is_multiple_of(2);
    let secret_prompt = matches!(app.phase, Phase::Prompt { secret: true, .. });
    let visible_prompt = matches!(app.phase, Phase::Prompt { secret: false, .. });
    let pass_focus = app.focus == Field::Password || secret_prompt || visible_prompt;

    render_input(
        f,
        user_area,
        "USER",
        &app.user,
        false,
        app.focus == Field::User,
        caret_on && app.focus == Field::User,
        theme,
    );
    render_input(
        f,
        pass_area,
        "PASS",
        &app.password,
        !visible_prompt,
        pass_focus,
        caret_on && pass_focus,
        theme,
    );
}

fn render_bar(f: &mut Frame, bar: Rect, app: &AppState, theme: &Theme) {
    let name = if app.user.trim().is_empty() {
        app.brand.as_str()
    } else {
        app.user.as_str()
    };
    f.render_widget(
        Paragraph::new(format!("LOGGING AS {name}"))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .bg(theme.accent)
                    .fg(theme.on_accent)
                    .add_modifier(Modifier::BOLD),
            ),
        bar,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_input(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    mask: bool,
    focused: bool,
    caret: bool,
    theme: &Theme,
) {
    let accent_or_dim = if focused { theme.accent } else { theme.dim };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent_or_dim))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent_or_dim),
        ))
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(theme.field_bg));

    let mut shown = if mask {
        MASK.to_string().repeat(value.chars().count())
    } else {
        value.to_string()
    };
    if caret {
        shown.push('▏');
    }

    f.render_widget(
        Paragraph::new(shown)
            .style(Style::default().fg(theme.fg))
            .block(block),
        area,
    );
}

fn status_line(app: &AppState, theme: &Theme, show_help: bool) -> (String, Style) {
    let dim = Style::default().fg(theme.dim);
    let err = Style::default().fg(theme.error).add_modifier(Modifier::BOLD);
    let spin = SPINNER[(app.tick as usize / 2) % SPINNER.len()];

    match &app.phase {
        Phase::Idle => {
            // Only reached with show_help on; otherwise the row is omitted.
            let _ = show_help;
            let base = "TAB switch field   ·   ENTER authenticate";
            if app.demo {
                (format!("{base}   ·   ESC quit  [demo]"), dim)
            } else {
                (base.to_string(), dim)
            }
        }
        Phase::Creating | Phase::Authenticating => {
            let msg = app.info.as_deref().unwrap_or("authenticating");
            (format!("{spin} {msg}…"), dim)
        }
        Phase::Prompt { message, .. } => (message.trim().to_string(), dim),
        Phase::Starting => (format!("{spin} starting session…"), dim),
        Phase::Done => (
            "session started".into(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Phase::Failed(msg) => (format!("✕ {}", msg.to_uppercase()), err),
    }
}

fn render_footer(f: &mut Frame, footer: Rect, logs: &LogBuffer, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(
            " SYSTEM INIT ",
            Style::default().fg(theme.dim).add_modifier(Modifier::DIM),
        ))
        .padding(Padding::horizontal(1));
    let inner = block.inner(footer);
    f.render_widget(block, footer);

    let rows = inner.height as usize;
    let lines: Vec<Line> = if logs.is_empty() {
        vec![Line::styled(
            "// awaiting system logs…",
            Style::default().fg(theme.dim),
        )]
    } else {
        logs.tail(rows)
            .map(|l| Line::styled(l.to_string(), Style::default().fg(theme.dim)))
            .collect()
    };

    f.render_widget(Paragraph::new(lines), inner);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Upper bound on the rows `text` needs when wrapped to `width` columns. Counts
/// explicit newlines plus a char-count ceiling per line (>= real word-wrap).
fn wrapped_height(text: &str, width: u16) -> u16 {
    let w = (width.max(1)) as usize;
    let mut lines = 0u16;
    for raw in text.split('\n') {
        let chars = raw.chars().count();
        lines += (chars.div_ceil(w).max(1)) as u16;
    }
    lines.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Accent;

    fn chrome<'a>(theme: &'a Theme, disclaimer: Option<&'a str>, show_help: bool) -> Chrome<'a> {
        Chrome {
            theme,
            art: None,
            disclaimer,
            show_help,
        }
    }

    fn render_to_string(app: &AppState, chrome: &Chrome, w: u16, h: u16) -> String {
        use ratatui::{backend::TestBackend, Terminal};
        let logs = LogBuffer::new(10);
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app, &logs, chrome)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn app() -> AppState {
        AppState::new(
            "0xc000022070".into(),
            "0xc000022070".into(),
            vec!["start-hyprland".into()],
            true,
        )
    }

    #[test]
    fn login_screen_shows_brand_and_fields() {
        let theme = Theme::preset(Accent::Amber);
        let screen = render_to_string(&app(), &chrome(&theme, None, true), 100, 30);
        assert!(screen.contains("LOGGING AS 0xc000022070"));
        assert!(screen.contains("USER"));
        assert!(screen.contains("PASS"));
        assert!(screen.contains("SYSTEM INIT"));
    }

    #[test]
    fn help_can_be_hidden_while_idle_but_status_survives() {
        let theme = Theme::preset(Accent::Amber);
        let hidden = render_to_string(&app(), &chrome(&theme, None, false), 100, 30);
        assert!(!hidden.contains("ENTER authenticate"), "help leaked when hidden");

        let mut failing = app();
        failing.phase = Phase::Failed("access denied".into());
        let screen = render_to_string(&failing, &chrome(&theme, None, false), 100, 30);
        assert!(screen.contains("ACCESS DENIED"), "status must show even with help off");
    }

    #[test]
    fn disclaimer_renders_when_present() {
        let theme = Theme::preset(Accent::Amber);
        let screen = render_to_string(
            &app(),
            &chrome(&theme, Some("work of fiction, purely coincidental"), true),
            100,
            30,
        );
        assert!(screen.contains("coincidental"));
    }

    #[test]
    fn password_is_masked_never_plaintext() {
        let theme = Theme::preset(Accent::Amber);
        let mut a = app();
        a.password = "supersecret".into();
        let screen = render_to_string(&a, &chrome(&theme, None, true), 100, 30);
        assert!(!screen.contains("supersecret"));
        assert!(screen.contains(MASK));
    }

    #[test]
    fn wrapped_height_counts_newlines_and_overflow() {
        assert_eq!(wrapped_height("one line", 40), 1);
        assert_eq!(wrapped_height("a\nb\nc", 40), 3);
        assert_eq!(wrapped_height(&"x".repeat(85), 40), 3);
    }
}
