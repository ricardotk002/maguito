use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::app::App;
use crate::transient::{COMMIT_CREATE, COMMIT_EDIT_HEAD, COMMIT_FLAGS};

pub fn render(frame: &mut Frame, app: &App) {
    let Some(transient) = &app.transient else { return };

    let area = bottom_rect(frame.area());
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();

    // Arguments
    lines.push(Line::from(Span::styled(
        "Arguments",
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    )));
    for f in COMMIT_FLAGS {
        let active = transient.active_flags.contains(f.git_flag);
        let key_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() });
        let flag_style = if active {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(f.short, key_style),
            Span::raw("  "),
            Span::raw(f.label),
            Span::raw("  "),
            Span::styled(format!("({})", f.git_flag), flag_style),
        ]));
    }

    lines.push(Line::from(""));

    // Action column headers
    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<16}", "Create"),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Edit HEAD",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Action rows side by side
    let rows = COMMIT_CREATE.len().max(COMMIT_EDIT_HEAD.len());
    for i in 0..rows {
        let mut spans: Vec<Span> = vec![Span::raw("  ")];

        if let Some(a) = COMMIT_CREATE.get(i) {
            spans.push(Span::styled(
                a.key.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!("  {:<10}", a.label)));
        } else {
            spans.push(Span::raw(" ".repeat(13)));
        }

        if let Some(a) = COMMIT_EDIT_HEAD.get(i) {
            spans.push(Span::styled(
                a.key.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!("  {}", a.label)));
        }

        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn bottom_rect(r: Rect) -> Rect {
    const HEIGHT: u16 = 13;
    let y = r.height.saturating_sub(HEIGHT);
    Rect { x: r.x, y: r.y + y, width: r.width, height: HEIGHT.min(r.height) }
}
