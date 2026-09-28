mod app;
mod event;
mod ui;

use std::io;
use std::time::Duration;

use app::{Action, App};
use crossterm::event as crossterm_event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut app = App::default();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        if crossterm_event::poll(Duration::from_millis(100))? {
            let terminal_event = crossterm_event::read()?;
            let action = Action::from(crate::event::Event::from(terminal_event));
            app.dispatch(action);
        }
    }
    Ok(())
}
