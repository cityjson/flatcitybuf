//! Interactive terminal UI for inspecting an FCB dataset header.

pub mod app;
pub mod map;
pub mod model;
pub mod source;
pub mod ui;

use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::inspect::app::App;
use crate::CliError;

/// Restores the terminal (leave alt-screen, disable raw mode) on drop, so an
/// error or panic never leaves the user's terminal broken.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, CliError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(TerminalGuard { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Apply a single key press to the app state.
fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true
        }
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
        KeyCode::Char('g') => app.to_top(),
        KeyCode::Char('G') => app.to_bottom(),
        _ => {}
    }
}

/// Public entry: inspect a local path or URL, driving a full-screen TUI.
pub fn run_inspect(source: &str) -> Result<(), CliError> {
    run_inspect_with_tty(source, io::stdout().is_terminal())
}

/// Testable seam: `is_tty` is injected so tests can assert the non-TTY guard
/// without a real terminal.
pub fn run_inspect_with_tty(source: &str, is_tty: bool) -> Result<(), CliError> {
    // Load the model first: cheap failures (bad path, bad URL) surface as plain
    // stderr errors rather than after switching into the alternate screen.
    let model = source::load_model(source)?;

    if !is_tty {
        return Err(CliError::NotATerminal);
    }

    let mut app = App::new(model.columns.len());
    let mut guard = TerminalGuard::enter()?;

    while !app.should_quit {
        guard.terminal.draw(|f| ui::draw(f, &model, &app))?;
        // Poll so a resize/redraw stays responsive without busy-spinning.
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key);
                }
            }
        }
    }
    Ok(()) // guard's Drop restores the terminal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CliError;

    #[test]
    fn non_tty_is_rejected_before_touching_the_terminal() {
        // A valid local file, but no TTY: must fail fast with NotATerminal,
        // never entering raw mode.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../conformance/inferable_types.fcb"
        );
        let err = run_inspect_with_tty(path, false);
        assert!(matches!(err, Err(CliError::NotATerminal)));
    }

    #[test]
    fn quit_keys_set_should_quit() {
        use crate::inspect::app::App;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(3);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );
        assert!(app.should_quit);

        let mut app = App::new(3);
        handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_sets_should_quit_but_plain_c_does_not() {
        use crate::inspect::app::App;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(3);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.should_quit);

        let mut app = App::new(3);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        );
        assert!(!app.should_quit);
    }

    #[test]
    fn arrow_and_vim_keys_drive_navigation() {
        use crate::inspect::app::{App, Tab};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(3);
        handle_key(&mut app, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.tab, Tab::Columns);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert_eq!(app.column_offset, 1);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
        );
        assert_eq!(app.column_offset, 0);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        );
        assert_eq!(app.column_offset, 2);
    }
}
