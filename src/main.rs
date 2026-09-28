//! Tlegado: native terminal reader with a headless Legado core.
mod app;
mod backend;
mod data;
mod demo;
mod jobs;
mod live;
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

const HELP: &str = "Tlegado — Legado 书源终端阅读器

用法：tlegado [--data-dir 路径] [--import-source 文件.json] [--import-only]
      tlegado --demo

默认启动真实模式，无内置书源。数据保存在 ~/.tlegado，或 TLEGADO_DATA_DIR。
--import-source 可重复指定；--import-only 导入后退出，不打开终端界面。
书源管理：i 导入、o 导出、空格启停、e 探索开关、v 查看完整 JSON。
/ 搜索；Enter 打开并自动加入书架；阅读中 [ / ] 切章，q 返回。
Esc 取消后台读取；Ctrl+C 保存进度并退出。--demo 为原离线演示。
";

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    );
}
struct TerminalSession;
impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn main() -> Result<()> {
    let options = backend::Options::parse(std::env::args_os().skip(1))?;
    if options.help {
        println!("{HELP}");
        return Ok(());
    }
    if options.import_only {
        return tokio::runtime::Runtime::new()?.block_on(async {
            let backend = backend::Backend::open(&options.data_dir).await?;
            for path in options.imports {
                let count = backend.import(&path).await?;
                println!("已导入 {count} 个书源：{}", path.display());
            }
            Ok(())
        });
    }
    let bridge = if options.demo {
        None
    } else {
        Some(jobs::Bridge::start(options.data_dir, options.imports)?)
    };
    let mut app = if options.demo {
        App::new()
    } else {
        App::new_live()
    };
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));
    let result = (|| -> Result<()> {
        enable_raw_mode()?;
        let _session = TerminalSession;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
        terminal.clear()?;
        run(&mut terminal, &mut app, bridge.as_ref())
    })();
    app.save_live_progress();
    let flush = dispatch(&mut app, bridge.as_ref());
    let shutdown = bridge.map(jobs::Bridge::finish).unwrap_or(Ok(()));
    result.and(flush).and(shutdown)
}

fn dispatch(app: &mut App, bridge: Option<&jobs::Bridge>) -> Result<()> {
    if let Some(bridge) = bridge {
        for command in std::mem::take(&mut app.commands) {
            bridge.send(command)?;
        }
    }
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    bridge: Option<&jobs::Bridge>,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    loop {
        if let Some(bridge) = bridge {
            for event in bridge.poll().take(64) {
                app.apply_live_event(event);
            }
        }
        app.sync_live();
        dispatch(app, bridge)?;
        terminal.draw(|f| ui::draw(f, app))?;
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(m) => {
                    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
                    let key = match m.kind {
                        MouseEventKind::ScrollDown => Some(KeyCode::Char('j')),
                        MouseEventKind::ScrollUp => Some(KeyCode::Char('k')),
                        _ => None,
                    };
                    if let Some(code) = key {
                        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
                    }
                }
                _ => {}
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
        app.sync_live();
        dispatch(app, bridge)?;
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
