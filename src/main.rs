//! Tlegado —— legado 书源规则的终端阅读器（ratatui 范例）
//!
//! 运行：
//!   cargo run
//!   cargo run --release
//!
//! 对照 React 范例看 theme.rs / ui.rs 的注释，重点是：
//!   1. truecolor 配色（不要用 Color::Yellow）
//!   2. BorderType::Rounded + title 带 bg 嵌在边框上
//!   3. 焦点态边框 / 选中行 bg + › caret
//!   4. unicode-width 算列宽，中英文混排才齐

mod app;
mod data;
mod demo;
mod reader;
mod theme;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{self, stdout},
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    // ── 终端初始化 ──────────────────────────────────────────
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    // 清一次屏并隐藏光标（ratatui 默认会管，但显式更稳）
    terminal.clear()?;

    let res = run(&mut terminal);

    // ── 恢复终端（即使 panic / 出错也要走这里）──────────────
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let tick_rate = Duration::from_millis(100); // 10 fps，够 spinner / toast
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // 事件轮询：有键就处理，否则等 tick
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    // 终端会同时发 Press / Release，只吃 Press
                    if key.kind == KeyEventKind::Press {
                        app.on_key(key);
                    }
                }
                Event::Mouse(m) => {
                    // 最小鼠标支持：滚轮 = j/k
                    // 完整点击选中需要 hit-test，这里留给你扩展
                    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
                    let fake = match m.kind {
                        MouseEventKind::ScrollDown => Some(KeyCode::Char('j')),
                        MouseEventKind::ScrollUp => Some(KeyCode::Char('k')),
                        _ => None,
                    };
                    if let Some(code) = fake {
                        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
                    }
                }
                Event::Resize(_, _) => {
                    // ratatui 下一帧自动用新尺寸，这里不用做事
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
