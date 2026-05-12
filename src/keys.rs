use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::transient::Transient;

pub enum KeyAction {
    Continue,
    Quit,
    CommitCreate,
    CommitAmend,
    CommitReword,
    CommitExtend,
    ConfirmYes,
}

pub fn handle(app: &mut App, key: KeyEvent) -> Result<KeyAction> {
    app.message = None;

    if app.confirm.is_some() {
        return Ok(match key.code {
            KeyCode::Char('y') => KeyAction::ConfirmYes,
            _ => { app.confirm = None; KeyAction::Continue }
        });
    }

    if app.show_help {
        app.show_help = false;
        return Ok(KeyAction::Continue);
    }

    if app.transient.is_some() {
        let awaiting = app.transient.as_ref().unwrap().awaiting_flag;

        if awaiting {
            app.transient.as_mut().unwrap().awaiting_flag = false;
            if let KeyCode::Char(c) = key.code {
                app.transient.as_mut().unwrap().toggle_flag(c);
            }
            return Ok(KeyAction::Continue);
        }

        return Ok(match key.code {
            KeyCode::Char('-') => {
                app.transient.as_mut().unwrap().awaiting_flag = true;
                KeyAction::Continue
            }
            KeyCode::Char('c') => KeyAction::CommitCreate,
            KeyCode::Char('a') => KeyAction::CommitAmend,
            KeyCode::Char('w') => KeyAction::CommitReword,
            KeyCode::Char('e') => KeyAction::CommitExtend,
            KeyCode::Char('q') | KeyCode::Esc => {
                app.transient = None;
                KeyAction::Continue
            }
            _ => KeyAction::Continue,
        });
    }

    Ok(match (key.modifiers, key.code) {
        (_, KeyCode::Char('q')) | (_, KeyCode::Esc) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Quit,
        (_, KeyCode::Char('j')) | (_, KeyCode::Down) => { app.move_down(); KeyAction::Continue }
        (_, KeyCode::Char('k')) | (_, KeyCode::Up)   => { app.move_up();   KeyAction::Continue }
        (_, KeyCode::Tab)                             => { app.toggle_collapse(); KeyAction::Continue }
        (_, KeyCode::Char('s'))                       => { app.stage_current()?;   KeyAction::Continue }
        (_, KeyCode::Char('u'))                       => { app.unstage_current()?; KeyAction::Continue }
        (_, KeyCode::Char('g'))                       => { app.refresh()?; KeyAction::Continue }
        (_, KeyCode::Char('x'))                       => { app.discard_current(); KeyAction::Continue }
        (_, KeyCode::Char('c'))                       => { app.transient = Some(Transient::commit()); KeyAction::Continue }
        (_, KeyCode::Char('?'))                       => { app.show_help = true; KeyAction::Continue }
        _                                             => KeyAction::Continue,
    })
}
