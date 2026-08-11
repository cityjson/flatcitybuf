//! Interactive terminal UI for inspecting an FCB dataset header.

pub mod app;
pub mod map;
pub mod model;
pub mod source;
pub mod static_report;
pub mod ui;

use std::io::{self, IsTerminal, Stdout, Write};
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

/// Which of the two renderings `inspect` produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectMode {
    /// Full-screen interactive terminal UI.
    Tui,
    /// One-shot plain-text report on stdout.
    Static,
}

/// Pick the mode: `--static` always wins, and without a terminal the TUI is
/// impossible, so a pipe or a redirect gets the static report.
pub fn select_mode(force_static: bool, is_tty: bool) -> InspectMode {
    if force_static || !is_tty {
        InspectMode::Static
    } else {
        InspectMode::Tui
    }
}

/// Public entry: inspect a local path or URL. Renders the TUI on a terminal,
/// and the static report when `force_static` is set or stdout is redirected.
pub fn run_inspect(source: &str, force_static: bool) -> Result<(), CliError> {
    run_inspect_with_tty(source, force_static, io::stdout().is_terminal())
}

/// Testable seam: `is_tty` is injected so tests can drive the mode choice
/// without a real terminal.
pub fn run_inspect_with_tty(
    source: &str,
    force_static: bool,
    is_tty: bool,
) -> Result<(), CliError> {
    // Load the model first: cheap failures (bad path, bad URL) surface as plain
    // stderr errors rather than after switching into the alternate screen.
    let model = source::load_model(source)?;

    if select_mode(force_static, is_tty) == InspectMode::Static {
        let report = static_report::render(&model);
        // Not `print!`: that panics when the write fails, and a reader that
        // closed the pipe early (`| head`) is a normal end, not a failure.
        if let Err(err) = io::stdout().write_all(report.as_bytes()) {
            if err.kind() != io::ErrorKind::BrokenPipe {
                return Err(err.into());
            }
        }
        return Ok(());
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

    #[test]
    fn non_tty_falls_back_to_the_static_report() {
        // A valid local file, but no TTY: prints the report and succeeds,
        // never entering raw mode.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../conformance/inferable_types.fcb"
        );
        assert!(run_inspect_with_tty(path, false, false).is_ok());
    }

    #[test]
    fn static_flag_wins_over_a_terminal() {
        assert_eq!(select_mode(true, true), InspectMode::Static);
        assert_eq!(select_mode(true, false), InspectMode::Static);
    }

    #[test]
    fn a_terminal_without_the_flag_gets_the_tui() {
        assert_eq!(select_mode(false, true), InspectMode::Tui);
        assert_eq!(select_mode(false, false), InspectMode::Static);
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
