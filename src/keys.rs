use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

pub fn handle(app: &mut App, key: KeyEvent) -> Result<bool> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Char('q')) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            return Ok(true);
        }
        (_, KeyCode::Char('j')) | (_, KeyCode::Down) => app.move_down(),
        (_, KeyCode::Char('k')) | (_, KeyCode::Up)   => app.move_up(),
        (_, KeyCode::Tab)                             => app.toggle_collapse(),
        (_, KeyCode::Char('s'))                       => app.stage_current()?,
        (_, KeyCode::Char('u'))                       => app.unstage_current()?,
        (_, KeyCode::Char('g'))                       => app.refresh()?,
        _ => {}
    }
    Ok(false)
}
