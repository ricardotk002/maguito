use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

pub enum KeyAction {
    Continue,
    Quit,
    OpenCommitEditor,
}

pub fn handle(app: &mut App, key: KeyEvent) -> Result<KeyAction> {
    // Resolve pending prefix first
    if let Some(prefix) = app.pending_prefix.take() {
        return Ok(match (prefix, key.code) {
            ('c', KeyCode::Char('c')) => KeyAction::OpenCommitEditor,
            _ => KeyAction::Continue,
        });
    }

    Ok(match (key.modifiers, key.code) {
        (_, KeyCode::Char('q')) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Quit,
        (_, KeyCode::Char('j')) | (_, KeyCode::Down) => { app.move_down(); KeyAction::Continue }
        (_, KeyCode::Char('k')) | (_, KeyCode::Up)   => { app.move_up();   KeyAction::Continue }
        (_, KeyCode::Tab)                             => { app.toggle_collapse(); KeyAction::Continue }
        (_, KeyCode::Char('s'))                       => { app.stage_current()?;   KeyAction::Continue }
        (_, KeyCode::Char('u'))                       => { app.unstage_current()?; KeyAction::Continue }
        (_, KeyCode::Char('g'))                       => { app.refresh()?; KeyAction::Continue }
        (_, KeyCode::Char('c'))                       => { app.pending_prefix = Some('c'); KeyAction::Continue }
        _                                             => KeyAction::Continue,
    })
}
