use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::app::App;
use crate::transient::{
    ActionDef, FlagDef, TransientKind,
    COMMIT_CREATE, COMMIT_EDIT_HEAD, COMMIT_FLAGS,
    FETCH_ACTIONS, FETCH_FLAGS,
    PULL_ACTIONS, PULL_FLAGS,
    PUSH_ACTIONS, PUSH_FLAGS,
    STASH_FLAGS, STASH_TRANSFORM,
};
use std::collections::HashSet;

pub fn render(frame: &mut Frame, app: &App) {
    let Some(transient) = &app.transient else { return };

    let (lines, height) = match transient.kind {
        TransientKind::Commit => (commit_lines(transient), 13u16),
        TransientKind::Fetch  => (simple_lines("Fetch from", FETCH_FLAGS, FETCH_ACTIONS, transient), 8u16),
        TransientKind::Push   => (simple_lines("Push to", PUSH_FLAGS, PUSH_ACTIONS, transient), 10u16),
        TransientKind::Pull   => (simple_lines("Pull from", PULL_FLAGS, PULL_ACTIONS, transient), 9u16),
        TransientKind::Stash  => (stash_lines(transient), 20u16),
    };

    let area = bottom_rect(frame.area(), height);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines), area);
}

fn section(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    ))
}

fn flag_row(f: &FlagDef, active: bool) -> Line<'static> {
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() });
    let git_flag_style = if active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(f.short, key_style),
        Span::raw("  "),
        Span::raw(f.label),
        Span::raw("  "),
        Span::styled(format!("({})", f.git_flag), git_flag_style),
    ])
}

fn flags_block(defs: &'static [FlagDef], active: &HashSet<&'static str>) -> Vec<Line<'static>> {
    let mut lines = vec![section("Arguments")];
    for f in defs {
        lines.push(flag_row(f, active.contains(f.git_flag)));
    }
    lines
}

fn action_row(a: &ActionDef) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(a.key.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {}", a.label)),
    ])
}

fn simple_lines(
    label: &'static str,
    flag_defs: &'static [FlagDef],
    action_defs: &'static [ActionDef],
    transient: &crate::transient::Transient,
) -> Vec<Line<'static>> {
    let mut lines = flags_block(flag_defs, &transient.active_flags);
    lines.push(Line::from(""));
    lines.push(section(label));
    for a in action_defs {
        lines.push(action_row(a));
    }
    lines
}

fn commit_lines(transient: &crate::transient::Transient) -> Vec<Line<'static>> {
    let mut lines = flags_block(COMMIT_FLAGS, &transient.active_flags);

    lines.push(Line::from(""));
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

    let rows = COMMIT_CREATE.len().max(COMMIT_EDIT_HEAD.len());
    for i in 0..rows {
        let mut spans: Vec<Span> = vec![Span::raw("  ")];
        if let Some(a) = COMMIT_CREATE.get(i) {
            spans.push(Span::styled(a.key.to_string(), Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(format!("  {:<10}", a.label)));
        } else {
            spans.push(Span::raw(" ".repeat(13)));
        }
        if let Some(a) = COMMIT_EDIT_HEAD.get(i) {
            spans.push(Span::styled(a.key.to_string(), Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(format!("  {}", a.label)));
        }
        lines.push(Line::from(spans));
    }

    lines
}

const STASH_COL: usize = 24;

fn stash_col_header(left: &'static str, right: &'static str) -> Line<'static> {
    let style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
    let pad = STASH_COL.saturating_sub(left.len());
    Line::from(vec![
        Span::styled(left, style),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, style),
    ])
}

fn stash_action_pair(
    left: Option<(char, &'static str)>,
    right: Option<(char, &'static str)>,
) -> Line<'static> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
    match left {
        Some((k, l)) => {
            let suffix = format!("  {}", l);
            let used = 2 + 1 + suffix.len();
            let pad = STASH_COL.saturating_sub(used).max(1);
            spans.push(Span::styled(k.to_string(), bold));
            spans.push(Span::raw(format!("{}{}", suffix, " ".repeat(pad))));
        }
        None => spans.push(Span::raw(" ".repeat(STASH_COL - 2))),
    }
    if let Some((k, l)) = right {
        spans.push(Span::styled(k.to_string(), bold));
        spans.push(Span::raw(format!("  {}", l)));
    }
    Line::from(spans)
}

fn stash_lines(transient: &crate::transient::Transient) -> Vec<Line<'static>> {
    let mut lines = flags_block(STASH_FLAGS, &transient.active_flags);
    lines.push(Line::from(""));

    lines.push(stash_col_header("Stash", "Snapshot"));
    let stash_snap: &[(Option<(char, &str)>, Option<(char, &str)>)] = &[
        (Some(('z', "both")),           Some(('Z', "both"))),
        (Some(('i', "index")),          Some(('I', "index"))),
        (Some(('w', "worktree")),       Some(('W', "worktree"))),
        (Some(('x', "keeping index")), Some(('r', "to wip ref"))),
        (Some(('P', "push")),           None),
    ];
    for &(l, r) in stash_snap {
        lines.push(stash_action_pair(l, r));
    }

    lines.push(Line::from(""));
    lines.push(stash_col_header("Use", "Inspect"));
    let use_inspect: &[(Option<(char, &str)>, Option<(char, &str)>)] = &[
        (Some(('a', "Apply")), Some(('l', "List"))),
        (Some(('p', "Pop")),   Some(('v', "Show"))),
        (Some(('k', "Drop")),  None),
    ];
    for &(l, r) in use_inspect {
        lines.push(stash_action_pair(l, r));
    }

    lines.push(Line::from(""));
    lines.push(section("Transform"));
    for a in STASH_TRANSFORM {
        lines.push(action_row(a));
    }

    lines
}

fn bottom_rect(r: Rect, height: u16) -> Rect {
    let y = r.height.saturating_sub(height);
    Rect { x: r.x, y: r.y + y, width: r.width, height: height.min(r.height) }
}
