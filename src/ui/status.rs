use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::app::{App, CursorItem};
use crate::git::repo::SectionKind;

const KIND_WIDTH: usize = 10; // "new file  " — widest label + padding

pub fn render(frame: &mut Frame, app: &App) {
    let cursor_items = app.visible_items();
    let mut list_items: Vec<ListItem> = Vec::new();
    let mut cursor_map: Vec<usize> = Vec::new();

    // Header
    list_items.push(ListItem::new(Line::from(vec![
        Span::styled("Head:     ", Style::default().fg(Color::Cyan)),
        Span::styled(app.branch.clone(), Style::default().add_modifier(Modifier::BOLD)),
    ])));
    list_items.push(ListItem::new(""));

    for item in &cursor_items {
        cursor_map.push(list_items.len());

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
                ))));
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
                ])));
            }

            CursorItem::Hunk(si, fi, hi) => {
                let hunk = &app.sections[*si].files[*fi].entry.hunks[*hi];
                list_items.push(ListItem::new(Line::from(Span::styled(
                    hunk.header.clone(),
                    Style::default().fg(Color::Cyan),
                ))));
                for dl in &hunk.lines {
                    let (color, prefix) = match dl.origin {
                        '+' => (Color::Green, "+"),
                        '-' => (Color::Red,   "-"),
                        _   => (Color::DarkGray, " "),
                    };
                    list_items.push(ListItem::new(Line::from(Span::styled(
                        format!("{prefix}{}", dl.content),
                        Style::default().fg(color),
                    ))));
                }
            }

            CursorItem::CommitHeader => {
                list_items.push(ListItem::new(Line::from("")));
                // CommitHeader is the cursor position; blank line above is non-selectable
                cursor_map.last_mut().map(|v| *v += 1); // adjust to point at the header row
                list_items.push(ListItem::new(Line::from(Span::styled(
                    "Recent commits",
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                ))));
            }

            CursorItem::Commit(i) => {
                let c = &app.commits[*i];
                list_items.push(ListItem::new(Line::from(vec![
                    Span::styled(c.sha.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::raw(c.message.clone()),
                ])));
            }
        }
    }

    let mut state = ListState::default();
    if let Some(&idx) = cursor_map.get(app.cursor) {
        state.select(Some(idx));
    }

    let list = List::new(list_items)
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_stateful_widget(list, frame.area(), &mut state);
}
