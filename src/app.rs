use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use crate::git::repo::{self, RepoStatus};

pub struct App {
    pub status: RepoStatus,
    pub cursor: usize,
    pub visible_count: usize,
}

impl App {
    pub fn new() -> Result<Self> {
        let status = repo::load()?;
        Ok(Self { status, cursor: 0, visible_count: 0 })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.status = repo::load()?;
        self.cursor = self.cursor.min(self.visible_count.saturating_sub(1));
        Ok(())
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.visible_count {
            self.cursor += 1;
        }
    }
}

pub fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;
    loop {
        terminal.draw(|f| crate::ui::status::render(f, &mut app))?;
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if crate::keys::handle(&mut app, key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}
