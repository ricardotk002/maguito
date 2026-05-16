use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::transient::{Transient, TransientKind};

fn attempt(app: &mut App, f: impl FnOnce(&mut App) -> Result<()>) {
    if let Err(e) = f(app) {
        app.message = Some(format!("{:#}", e));
    }
}

pub enum KeyAction {
    Continue,
    Quit,
    CommitCreate,
    CommitAmend,
    CommitReword,
    CommitExtend,
    ConfirmYes,
    FetchFromPushRemote,
    FetchFromUpstream,
    FetchAll,
    PushToPushRemote,
    PushToUpstream,
    PullFromPushRemote,
    PullFromUpstream,
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

        let kind = app.transient.as_ref().unwrap().kind; // Copy — releases borrow

        return Ok(match (kind, key.code) {
            (_, KeyCode::Char('-')) => {
                app.transient.as_mut().unwrap().awaiting_flag = true;
                KeyAction::Continue
            }
            (TransientKind::Commit, KeyCode::Char('c')) => KeyAction::CommitCreate,
            (TransientKind::Commit, KeyCode::Char('a')) => KeyAction::CommitAmend,
            (TransientKind::Commit, KeyCode::Char('w')) => KeyAction::CommitReword,
            (TransientKind::Commit, KeyCode::Char('e')) => KeyAction::CommitExtend,
            (TransientKind::Fetch,  KeyCode::Char('p')) => KeyAction::FetchFromPushRemote,
            (TransientKind::Fetch,  KeyCode::Char('u')) => KeyAction::FetchFromUpstream,
            (TransientKind::Fetch,  KeyCode::Char('a')) => KeyAction::FetchAll,
            (TransientKind::Push,   KeyCode::Char('p')) => KeyAction::PushToPushRemote,
            (TransientKind::Push,   KeyCode::Char('u')) => KeyAction::PushToUpstream,
            (TransientKind::Pull,   KeyCode::Char('p')) => KeyAction::PullFromPushRemote,
            (TransientKind::Pull,   KeyCode::Char('u')) => KeyAction::PullFromUpstream,
            (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => {
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
        (_, KeyCode::Char('s')) => { attempt(app, |a| a.stage_current());   KeyAction::Continue }
        (_, KeyCode::Char('u')) => { attempt(app, |a| a.unstage_current()); KeyAction::Continue }
        (_, KeyCode::Char('g')) => { attempt(app, |a| a.refresh());         KeyAction::Continue }
        (_, KeyCode::Char('x')) => { app.discard_current(); KeyAction::Continue }
        (_, KeyCode::Char('c')) => { app.transient = Some(Transient::commit()); KeyAction::Continue }
        (_, KeyCode::Char('f')) => { app.transient = Some(Transient::fetch()); KeyAction::Continue }
        (_, KeyCode::Char('p')) | (_, KeyCode::Char('P')) => { app.transient = Some(Transient::push());  KeyAction::Continue }
        (_, KeyCode::Char('F')) => { app.transient = Some(Transient::pull());  KeyAction::Continue }
        (_, KeyCode::Char('?')) => { app.show_help = true; KeyAction::Continue }
        _                       => KeyAction::Continue,
    })
}
