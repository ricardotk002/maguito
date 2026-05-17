use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, CursorItem};
use crate::git::repo::SectionKind;

const KIND_WIDTH: usize = 10; // "new file  " — widest label + padding

pub fn render(frame: &mut Frame, app: &App) {
    let footer_text = app.confirm.as_ref().map(|c| c.prompt.as_str())
        .or(app.message.as_deref());
    let footer_height = if footer_text.is_some() { 1 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_height)])
        .split(frame.area());

    let cursor_items = app.visible_items();
    let mut list_items: Vec<ListItem> = Vec::new();
    let mut cursor_map: Vec<usize> = Vec::new();

    // Header
    list_items.push(ListItem::new(Line::from(vec![
        Span::styled("Head:     ", Style::default().fg(Color::Cyan)),
        Span::styled(app.branch.clone(), Style::default().add_modifier(Modifier::BOLD)),
    ])));
    list_items.push(ListItem::new(""));

    let visual_range = app.visual_range();

    for (ci, item) in cursor_items.iter().enumerate() {
        cursor_map.push(list_items.len());

        let in_visual = visual_range.map_or(false, |(lo, hi)| ci >= lo && ci <= hi);
        let vbg = if in_visual {
            Style::default().bg(Color::Rgb(60, 60, 20))
        } else {
            Style::default()
        };

        match item {
            CursorItem::Section(si) => {
                if *si > 0 {
                    list_items.push(ListItem::new(""));
                    *cursor_map.last_mut().unwrap() += 1;
                }
                let section = &app.sections[*si];
                let label = match section.kind {
                    SectionKind::Untracked => format!("Untracked files ({})", section.files.len()),
                    SectionKind::Staged    => format!("Staged changes ({})", section.files.len()),
                    SectionKind::Unstaged  => format!("Unstaged changes ({})", section.files.len()),
                };
                list_items.push(ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                )).style(vbg)));
            }

            CursorItem::File(si, fi) => {
                let section = &app.sections[*si];
                let file = &section.files[*fi];
                let label = file.entry.kind.label();
                let prefix = if label.is_empty() {
                    String::new()
                } else {
                    format!("{:<width$}", label, width = KIND_WIDTH)
                };
                let color = match section.kind {
                    SectionKind::Staged => Color::Green,
                    _                   => Color::Reset,
                };
                list_items.push(ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Reset)),
                    Span::styled(file.entry.path.clone(), Style::default().fg(color)),
                ]).style(vbg)));
            }

            CursorItem::Hunk(si, fi, hi) => {
                let hunk = &app.sections[*si].files[*fi].entry.hunks[*hi];
                list_items.push(ListItem::new(Line::from(Span::styled(
                    hunk.header.clone(),
                    Style::default().fg(Color::Cyan),
                )).style(vbg)));
            }

            CursorItem::DiffLine(si, fi, hi, li) => {
                let dl = &app.sections[*si].files[*fi].entry.hunks[*hi].lines[*li];
                let (color, prefix) = match dl.origin {
                    '+' => (Color::Green, "+"),
                    '-' => (Color::Red,   "-"),
                    _   => (Color::Reset, " "),
                };
                list_items.push(ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{}", dl.content),
                    Style::default().fg(color),
                )).style(vbg)));
            }

            CursorItem::CommitHeader => {
                list_items.push(ListItem::new(Line::from("")));
                cursor_map.last_mut().map(|v| *v += 1);
                list_items.push(ListItem::new(Line::from(Span::styled(
                    "Recent commits",
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                )).style(vbg)));
            }

            CursorItem::Commit(i) => {
                let c = &app.commits[*i];
                list_items.push(ListItem::new(Line::from(vec![
                    Span::styled(c.sha.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::raw(c.message.clone()),
                ]).style(vbg)));
            }
        }
    }

    let mut state = ListState::default();
    if let Some(&idx) = cursor_map.get(app.cursor) {
        state.select(Some(idx));
    }

    let list = List::new(list_items)
        .highlight_style(Style::default().bg(Color::Rgb(48, 48, 56)));

    frame.render_stateful_widget(list, chunks[0], &mut state);

    if let Some(text) = footer_text {
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::White)),
            chunks[1],
        );
    }

    if app.show_help {
        let area = bottom_rect(frame.area());
        frame.render_widget(Clear, area);

        let section = |s: &'static str| {
            Line::from(Span::styled(s, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)))
        };
        let row = |k: &'static str, desc: &'static str| {
            let pad = " ".repeat(8_usize.saturating_sub(k.len()));
            Line::from(vec![
                Span::raw(" "),
                Span::styled(k, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(pad),
                Span::raw(desc),
            ])
        };

        let lines = vec![
            section("Transient and dwim commands"),
            row("c", "Commit"),
            row("f", "Fetch"),
            row("F", "Pull"),
            row("P", "Push"),
            row("z", "Stash"),
            Line::from(""),
            section("Applying changes"),
            row("s", "Stage"),
            row("u", "Unstage"),
            Line::from(""),
            section("Essential commands"),
            row("g", "Refresh current buffer"),
            row("q", "Bury current buffer"),
            row("<tab>", "Toggle section at point"),
        ];

        frame.render_widget(Paragraph::new(lines), area);
    }
}

fn bottom_rect(r: Rect) -> Rect {
    const HEIGHT: u16 = 14;
    let y = r.height.saturating_sub(HEIGHT);
    Rect { x: r.x, y: r.y + y, width: r.width, height: HEIGHT.min(r.height) }
}
