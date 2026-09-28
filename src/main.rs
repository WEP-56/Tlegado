//! Tlegado: native terminal reader with a headless Legado core.
mod app;
mod backend;
mod book_logo;
mod data;
mod demo;
mod jobs;
mod live;
mod reader;
mod sources;
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
书源管理：/ 筛选、空格多选、a 全选筛选结果、Enter 启停、+/- 批量启停。
e 探索开关、E 关闭探索、x 确认删除、i 导入、o 导出全部、v 查看 JSON。
探索书源统一在“探索书源”列表中选择，分类页 Backspace 返回列表。
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
    install_panic_hook((!options.demo).then_some(options.data_dir.as_path()))?;
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

fn install_panic_hook(data_dir: Option<&std::path::Path>) -> Result<()> {
    // A caught background panic must never tear down the UI thread's terminal.
    let ui_thread = std::thread::current().id();
    let panic_log = std::sync::Mutex::new(if let Some(data_dir) = data_dir {
        std::fs::create_dir_all(data_dir)?;
        Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(data_dir.join("panic.log"))?,
        )
    } else {
        None
    });
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if std::thread::current().id() == ui_thread {
            restore_terminal();
            previous_hook(info);
        } else if let Ok(mut log) = panic_log.lock() {
            if let Some(log) = log.as_mut() {
                let _ = writeln!(log, "{} {info}", chrono::Local::now());
            }
        }
    }));
    Ok(())
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

#[cfg(test)]
mod terminal_tests {
    #[test]
    fn caught_worker_panic_never_writes_terminal_controls() {
        const CHILD: &str = "TLEGADO_PANIC_HOOK_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "terminal_tests::caught_worker_panic_never_writes_terminal_controls",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stdout}\n{stderr}");
            assert!(stderr.is_empty(), "{stderr}");
            assert!(!stdout.contains('\x1b') && !stdout.contains("worker-panic-marker"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        super::install_panic_hook(Some(temp.path())).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let result = tokio::spawn(async {
                panic!("worker-panic-marker");
            })
            .await;
            assert!(result.unwrap_err().is_panic());
            assert_eq!(tokio::spawn(async { 42 }).await.unwrap(), 42);
        });
        assert!(std::fs::read_to_string(temp.path().join("panic.log"))
            .unwrap()
            .contains("worker-panic-marker"));
    }
}
