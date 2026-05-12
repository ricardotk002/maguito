use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App) {
    let mut items: Vec<ListItem> = Vec::new();

    items.push(ListItem::new(Line::from(vec![
        Span::styled("Head:     ", Style::default().fg(Color::Cyan)),
        Span::styled(&*app.status.branch, Style::default().add_modifier(Modifier::BOLD)),
    ])));
    items.push(ListItem::new(""));

    if !app.status.untracked.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("Untracked files ({})", app.status.untracked.len()),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ))));
        for f in &app.status.untracked {
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(f.clone(), Style::default().fg(Color::Red)),
            ])));
        }
        items.push(ListItem::new(""));
    }

    if !app.status.unstaged.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("Unstaged changes ({})", app.status.unstaged.len()),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ))));
        for f in &app.status.unstaged {
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  modified  "),
                Span::styled(f.clone(), Style::default().fg(Color::Yellow)),
            ])));
        }
        items.push(ListItem::new(""));
    }

    if !app.status.staged.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("Staged changes ({})", app.status.staged.len()),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ))));
        for f in &app.status.staged {
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  modified  "),
                Span::styled(f.clone(), Style::default().fg(Color::Green)),
            ])));
        }
        items.push(ListItem::new(""));
    }

    app.visible_count = items.len();

    let mut state = ListState::default();
    state.select(Some(app.cursor));

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_stateful_widget(list, frame.area(), &mut state);
}
