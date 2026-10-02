//! 布局 · 各视图绘制
//!
//! 对照 React 范例：
//!   顶栏  →  ~/.tlegado › 面包屑          右侧状态
//!   主体  →  左 Sidebar | 右 Main
//!   底栏  →  快捷键提示 / toast           [alpha]

use crate::app::{App, Focus, Route, ShelfFilter, ToastTone};
use crate::data::{Book, Kind};
use crate::theme::{
    self, bar_spans, center_rect, fill_bg, hints, list_scroll, pad, panel, panel_full, row_line, s,
    sb, sbg, truncate, THEME,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    // 整屏铺底，避免默认色漏边
    fill_bg(area, f.buffer_mut());
    if app.boss_mode {
        draw_boss_terminal(f, app, area);
        return;
    }

    let footer_height = app
        .reader
        .as_ref()
        .filter(|r| r.aloud_active())
        .map_or(1, |reader| {
            aloud_footer_lines(app, reader, area.width.saturating_sub(2)).len() as u16
        })
        .min(area.height.saturating_sub(4).max(1));
    // 听书控制器按实际宽度分行，保证窄屏仍可发现所有播放控制。
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                            // top bar
            Constraint::Length(u16::from(area.height >= 30)), // gap
            Constraint::Min(0),                               // body
            Constraint::Length(footer_height),                // footer
        ])
        .split(inset(area, 1, 0));

    draw_topbar(f, app, chunks[0]);
    draw_body(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);

    if crate::sources::draw_confirmation(f, app, area) {
        return;
    }
    if crate::library::draw_modal(f, app, area) {
        return;
    }
    if app.live.as_ref().is_some_and(|l| l.picker.is_some()) {
        draw_source_picker(f, app, area);
        return;
    }

    if let Some(input) = app.live.as_ref().and_then(|l| l.prompt.as_ref()) {
        let rect = center_rect(area, 76, 8);
        ratatui::widgets::Clear.render(rect, f.buffer_mut());
        let title = match input.kind {
            crate::live::InputKind::Rules => "导入净化规则 JSON（文件或 HTTP(S) URL）",
            crate::live::InputKind::Layout => "导入排版 JSON（文件或 HTTP(S) URL）",
            crate::live::InputKind::Import => "导入 Legado JSON",
            crate::live::InputKind::LocalBook => "导入本地 TXT / EPUB",
            crate::live::InputKind::Export => "导出书源（新文件）",
        };
        let block = panel(vec![Span::styled(title, sb(THEME.mag))], None, true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let text = format!(
            "文件路径：\n{}▋\n\nEnter 确认 · Esc 取消 · 支持含空格路径",
            input.text
        );
        f.render_widget(
            Paragraph::new(text)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .style(s(THEME.fg)),
            inner,
        );
        return;
    }

    if app.help {
        if app.reader.is_none()
            && (matches!(app.route(), Route::ExploreSources)
                || (app.live.is_some() && matches!(app.route(), Route::Sources)))
        {
            crate::sources::draw_help(f, area);
        } else if app.reader.is_some() {
            draw_reader_help(
                f,
                area,
                app.live.is_some(),
                app.reader.as_ref().unwrap().is_horizontal(),
            );
        } else {
            draw_help(f, area, app.live.is_some());
        }
    }
}

fn draw_boss_terminal(f: &mut Frame, app: &App, area: Rect) {
    use ratatui::style::{Color, Style};
    let bg = Color::Rgb(8, 12, 18);
    let fg = Color::Rgb(177, 213, 191);
    let accent = Color::Rgb(105, 190, 155);
    let dim = Color::Rgb(94, 119, 107);
    let pulse = ["·", "··", "···", "··", "·"][app.tick as usize % 5];
    let uptime = app.tick / 10;
    let mut lines = vec![
        (
            "workstation@terminal:~$ htop --user-session".to_string(),
            accent,
        ),
        (
            "Linux workstation 6.8.0-generic   x86_64   pts/2".to_string(),
            dim,
        ),
        (
            format!(
                " uptime {:02}:{:02}:{:02}   load 0.42  0.37  0.31",
                uptime / 3600,
                uptime / 60 % 60,
                uptime % 60
            ),
            dim,
        ),
        ("".into(), fg),
        (
            " PID   USER       CPU   MEM   STATE       COMMAND".into(),
            accent,
        ),
        (
            " 1842  reader     1.2%  0.8%  sleeping    index-worker".into(),
            fg,
        ),
        (
            " 2197  reader     0.4%  0.3%  running     sync-agent".into(),
            fg,
        ),
        (
            " 2310  reader     0.1%  0.2%  waiting     cache-refresh".into(),
            fg,
        ),
        ("".into(), fg),
        (
            format!(
                "[{}] sync workspace metadata",
                if app.tick % 17 < 15 { "ok" } else { ".." }
            ),
            fg,
        ),
        (
            format!(
                "[ok] checkpoint journal {:04} entries",
                1200 + app.tick as usize % 97
            ),
            fg,
        ),
        (format!("[ok] background queue{} 4 tasks", pulse), fg),
        ("[ok] file watcher: 0 pending changes".into(), fg),
        ("[info] no interactive input required".into(), dim),
        ("".into(), fg),
        (
            "workstation@terminal:~$ tail -f /var/log/workstation.log".into(),
            accent,
        ),
        (
            format!(
                "{} INFO worker heartbeat: scheduler cycle {}",
                pulse, app.tick
            ),
            fg,
        ),
        (
            format!(
                "{} INFO cache index stable; next scan in {}s",
                pulse,
                30 - app.tick as usize % 30
            ),
            fg,
        ),
        (
            format!(
                "{} INFO session metrics: {} samples collected",
                pulse,
                80 + app.tick as usize % 20
            ),
            fg,
        ),
        ("".into(), fg),
        ("workstation@terminal:~$ _".into(), accent),
    ];
    lines.truncate(area.height.saturating_sub(1) as usize);
    let body = lines
        .into_iter()
        .map(|(line, color)| Line::from(Span::styled(line, Style::default().fg(color))))
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(body).style(Style::default().bg(bg).fg(fg)),
        area,
    );
}

fn inset(r: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: r.x + x,
        y: r.y + y,
        width: r.width.saturating_sub(x * 2),
        height: r.height.saturating_sub(y * 2),
    }
}

fn draw_source_picker(f: &mut Frame, app: &App, area: Rect) {
    let live = app.live.as_ref().unwrap();
    let picker = live.picker.as_ref().unwrap();
    let rect = center_rect(area, 88, 22);
    ratatui::widgets::Clear.render(rect, f.buffer_mut());
    let block = panel_full(
        vec![Span::styled(
            format!(
                "书源 · {} · {} 个候选",
                picker.target.name,
                picker.candidates.len()
            ),
            sb(THEME.hi),
        )],
        None,
        Some(Span::styled(
            "j/k 选择 · Enter 试读/换源 · n 续页 · r 重试 · Esc 取消",
            s(THEME.dim),
        )),
        true,
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);
    f.render_widget(
        Paragraph::new(live.status.as_str())
            .style(s(THEME.info))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        rows[0],
    );
    let scroll = list_scroll(
        picker.selected,
        rows[1].height as usize,
        picker.candidates.len(),
    );
    for (i, book) in picker
        .candidates
        .iter()
        .enumerate()
        .skip(scroll)
        .take(rows[1].height as usize)
    {
        let selected = i == picker.selected;
        let current = crate::live::book_id(book) == crate::live::book_id(&picker.target);
        let name = book.origin_name.as_deref().unwrap_or(&book.origin);
        let content = vec![
            Span::styled(if current { "● " } else { "  " }, s(THEME.ok)),
            Span::styled(
                format!(
                    "{}  {}",
                    name,
                    book.latest_chapter_title.as_deref().unwrap_or_default()
                ),
                if selected { sb(THEME.hi) } else { s(THEME.fg) },
            ),
        ];
        f.render_widget(
            Paragraph::new(row_line(selected, true, content)),
            Rect {
                x: rows[1].x,
                y: rows[1].y + (i - scroll) as u16,
                width: rows[1].width,
                height: 1,
            },
        );
    }
}

// ── 顶栏 ────────────────────────────────────────────────────
fn draw_topbar(f: &mut Frame, app: &App, area: Rect) {
    let focus_label = match app.focus {
        Focus::Sidebar if app.reader.is_some() => "目录",
        Focus::Main if app.reader.is_some() => "正文",
        Focus::Sidebar => "侧栏",
        Focus::Main => "主体",
    };
    let left = Line::from(vec![
        Span::styled(
            if app.live.is_some() {
                "Tlegado "
            } else {
                "演示 "
            },
            s(THEME.mute),
        ),
        Span::styled("› ", s(THEME.dim)),
        Span::styled(app.crumb(), s(THEME.fg)),
    ]);
    let right = if area.width >= 70 {
        format!("{focus_label} · Tlegado {}", env!("CARGO_PKG_VERSION"))
    } else {
        String::new()
    };
    draw_status_line(f, area, left, &right);
}

// 为右侧标签预留独立区域，长标题和提示只能在左侧截断。
fn draw_status_line(f: &mut Frame, area: Rect, left: Line<'_>, right: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rw = unicode_width::UnicodeWidthStr::width(right) as u16;
    let reserved = if rw > 0 { (rw + 2).min(area.width) } else { 0 };
    f.render_widget(
        Paragraph::new(left),
        Rect::new(area.x, area.y, area.width - reserved, 1),
    );
    if rw > 0 && rw < area.width {
        f.render_widget(
            Paragraph::new(right).style(s(THEME.dim)),
            Rect::new(area.right() - rw, area.y, rw, 1),
        );
    }
}

// ── 底栏 ────────────────────────────────────────────────────
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(reader) = app.reader.as_ref().filter(|r| r.aloud_active()) {
        ratatui::widgets::Clear.render(area, f.buffer_mut());
        f.render_widget(
            Paragraph::new(aloud_footer_lines(app, reader, area.width))
                .style(s(THEME.fg).bg(THEME.bg)),
            area,
        );
        return;
    }
    let left = if let Some((ref msg, tone, _)) = app.toast {
        let (icon, color) = match tone {
            ToastTone::Ok => ("✓", THEME.ok),
            ToastTone::Err => ("✗", THEME.err),
            ToastTone::Info => ("●", THEME.info),
        };
        Line::from(vec![
            Span::styled(format!("{icon} "), s(color)),
            Span::styled(msg.clone(), s(color)),
        ])
    } else if app.reader.is_none() && matches!(app.route(), Route::Tts) && app.focus == Focus::Main
    {
        hints(&[
            ("↑↓", "选择"),
            ("Enter", "编辑"),
            ("Ctrl+S", "保存"),
            ("Esc", "返回"),
        ])
    } else if let Some(live) = app.live.as_ref().filter(|l| !l.status.is_empty()) {
        let prefix = if live.busy() {
            theme::SPINNER[app.tick as usize % theme::SPINNER.len()]
        } else {
            "●"
        };
        Line::from(Span::styled(
            format!("{prefix} {}", live.status),
            s(THEME.info),
        ))
    } else if area.width < 70 {
        if app.reader.is_some() {
            hints(&[("p", "听书"), ("?", "帮助"), ("q", "返回")])
        } else {
            hints(&[("tab", "焦点"), ("?", "帮助"), ("q", "退出")])
        }
    } else if app.reader.is_some() && app.focus == Focus::Sidebar {
        hints(&[
            ("tab", "正文"),
            ("j/k", "选章"),
            ("enter", "阅读"),
            ("p", "听书"),
            ("q", "返回"),
            ("?", "帮助"),
        ])
    } else if app.reader.is_some() {
        hints(&[
            ("tab", "目录"),
            if app.reader.as_ref().unwrap().is_horizontal() {
                ("h/l", "翻页")
            } else {
                ("j/k", "滚动")
            },
            ("space", "翻页"),
            ("A", "自动阅读"),
            ("p", "听书"),
            ("s", "换源"),
            ("a", "加入书架"),
            ("q", "返回"),
            ("?", "帮助"),
        ])
    } else {
        hints(&[
            ("tab", "焦点"),
            ("j/k", "移动"),
            ("enter", "打开"),
            ("/", "搜索"),
            ("?", "帮助"),
            ("q", "退出"),
        ])
    };
    let right = if left.width() + 9 <= area.width as usize {
        "[alpha]"
    } else {
        ""
    };
    draw_status_line(f, area, left, right);
}

fn aloud_footer_lines(app: &App, reader: &crate::reader::Reader, width: u16) -> Vec<Line<'static>> {
    let status = if reader.aloud_loading() {
        "加载中"
    } else if reader.aloud_error().is_some() {
        "失败"
    } else if reader.aloud_playing() && reader.aloud_preparing() {
        "准备中"
    } else if reader.aloud_playing() {
        "▶"
    } else {
        "Ⅱ"
    };
    let mut line = Line::from(Span::styled(
        format!(
            "{status} {} · {}",
            reader.aloud_speed_label(),
            reader.aloud_timer_label()
        ),
        sb(THEME.accent),
    ));
    let mut lines = Vec::new();
    let hint_items = [
        (
            "p/空格",
            if reader.aloud_error().is_some() {
                "重试"
            } else if reader.aloud_playing() {
                "暂停"
            } else {
                "继续"
            },
        ),
        (
            "[ ]",
            if width >= 140 {
                "上一段/下一段"
            } else {
                "段落"
            },
        ),
        (
            "{ }",
            if width >= 140 {
                "上一章/下一章"
            } else {
                "章节"
            },
        ),
        ("-/+", "倍速"),
        ("t", "定时"),
        ("s", "停止"),
        ("q/Esc", "返回"),
    ];
    for (key, label) in hint_items {
        let item_width = key.width() + 1 + label.width();
        if line.width() + 2 + item_width > usize::from(width) {
            lines.push(line);
            line = Line::default();
        } else {
            line.spans.push(Span::styled("  ", s(THEME.dim)));
        }
        line.spans
            .push(Span::styled(key.to_string(), s(THEME.mute)));
        line.spans
            .push(Span::styled(format!(" {label}"), s(THEME.dim)));
    }
    lines.push(line);
    if let Some(message) = reader.aloud_error() {
        lines.push(Line::styled(
            truncate(message, width as usize),
            s(THEME.err),
        ));
    } else if let Some((message, ToastTone::Err, _)) = &app.toast {
        lines.push(Line::styled(
            truncate(message, width as usize),
            s(THEME.err),
        ));
    }
    lines
}

// ── 主体：侧栏 + 主区 ───────────────────────────────────────
fn draw_body(f: &mut Frame, app: &mut App, area: Rect) {
    if app.sidebar_hidden {
        draw_main(f, app, area);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(area);
    draw_sidebar(f, app, cols[0]);
    draw_main(f, app, cols[1]);
}

// ── 侧栏 ────────────────────────────────────────────────────
fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    if let Some(reader) = &app.reader {
        crate::reader::draw_chapters(f, reader, area, app.focus == Focus::Sidebar);
        return;
    }
    let focused = app.focus == Focus::Sidebar;
    let title = vec![
        Span::styled(
            "● ",
            if focused {
                sb(THEME.accent)
            } else {
                s(THEME.mute)
            },
        ),
        Span::styled("Tlegado", sb(THEME.hi)),
    ];
    let block = panel(title, Some(Span::styled("tab ⇄", s(THEME.dim))), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let vis = app.visible_nav();
    let h = inner.height as usize;

    let mut last_section: Option<&str> = Some("__");
    // 按 visible 顺序生成行；section header 插在该 section 第一项前面
    let mut rows: Vec<SideRow> = Vec::new();
    for &i in &vis {
        let n = &app.nav[i];
        if n.section != last_section {
            if let Some(sec) = n.section {
                let collapsed = app.collapsed.get(sec).copied().unwrap_or(false);
                rows.push(SideRow::Header {
                    name: sec,
                    collapsed,
                });
            }
            last_section = n.section;
        }
        rows.push(SideRow::Item { idx: i });
    }

    let sel_row = rows
        .iter()
        .position(|r| matches!(r, SideRow::Item { idx } if *idx == app.nav_idx))
        .unwrap_or(0);
    let scroll = list_scroll(sel_row, h, rows.len());

    for (y, row) in rows.iter().skip(scroll).enumerate() {
        if y as u16 >= inner.height {
            break;
        }
        let row_area = Rect {
            x: inner.x,
            y: inner.y + y as u16,
            width: inner.width,
            height: 1,
        };
        match row {
            SideRow::Header { name, collapsed } => {
                let arrow = if *collapsed { "▸" } else { "▾" };
                let line = Line::from(vec![
                    Span::styled(format!(" {arrow} "), s(THEME.dim)),
                    Span::styled(*name, s(THEME.dim)),
                    Span::styled(" ", s(THEME.dim)),
                    Span::styled("─".repeat(20), s(THEME.border)),
                ]);
                f.render_widget(Paragraph::new(line), row_area);
            }
            SideRow::Item { idx } => {
                let n = &app.nav[*idx];
                let sel = *idx == app.nav_idx;
                let indent = if n.section.is_some() { "  " } else { "" };
                let mut content = vec![
                    Span::raw(indent),
                    Span::styled(
                        pad(&n.label, 12),
                        if sel { sb(THEME.hi) } else { s(THEME.fg) },
                    ),
                ];
                if !n.right.is_empty() {
                    // caret(2) + indent + label(12) + right；剩余空间用空格填
                    let used = 2
                        + indent.len()
                        + 12
                        + unicode_width::UnicodeWidthStr::width(n.right.as_str());
                    let gap = (inner.width as usize).saturating_sub(used + 1);
                    content.push(Span::raw(" ".repeat(gap)));
                    content.push(Span::styled(n.right.clone(), s(THEME.dim)));
                }
                let line = row_line(sel, focused, content);
                f.render_widget(Paragraph::new(line), row_area);
            }
        }
    }
}

enum SideRow {
    Header { name: &'static str, collapsed: bool },
    Item { idx: usize },
}

// ── 主区路由 ────────────────────────────────────────────────
fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let body = area;

    if let Some(reader) = &mut app.reader {
        crate::reader::draw_page(f, reader, body, app.focus == Focus::Main);
        return;
    }
    match app.route().clone() {
        Route::Home => draw_home(f, app, body),
        Route::Shelf { filter } => draw_shelf(f, app, body, filter),
        Route::Discover { source_idx } => draw_discover(f, app, body, source_idx),
        Route::ExploreSources => crate::sources::draw(f, app, body),
        Route::Search => draw_search(f, app, body),
        Route::Tts => crate::audio::settings::draw(f, app, body),
        Route::Sources if app.live.is_some() && !app.demo.json => {
            crate::sources::draw(f, app, body)
        }
        Route::History | Route::Sources | Route::Purify | Route::Prefs => {
            crate::demo::draw(f, app, body)
        }
    }
}

// ── 首页 ────────────────────────────────────────────────────
fn draw_home(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Main;
    let block = panel(
        vec![
            Span::styled("Tlegado ", sb(THEME.hi)),
            Span::styled(env!("CARGO_PKG_VERSION"), s(THEME.dim)),
        ],
        None,
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let enabled = app.sources.iter().filter(|s| s.enabled).count();
    let updates: u32 = app.books.iter().map(|b| b.new_count).sum();

    // The animated logo gets its own column. On narrow terminals it is hidden
    // completely so its Braille cells cannot collide with the welcome copy.
    let content = if inner.width >= 86 {
        let logo_area = Rect::new(inner.x + 1, inner.y + 1, 34, inner.height.min(12));
        crate::book_logo::render(f.buffer_mut(), logo_area, app.tick as i64 * 100);
        Rect::new(
            inner.x + 38,
            inner.y + 1,
            inner.width.saturating_sub(39),
            inner.height.saturating_sub(2),
        )
    } else {
        inset(inner, 2, 1)
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "基于 legado（阅读 3.0）书源规则的终端阅读器。书源 ",
            s(THEME.mute),
        ),
        Span::styled(format!("{enabled}/{}", app.sources.len()), s(THEME.fg)),
        Span::styled(" 已启用，书架 ", s(THEME.mute)),
        Span::styled(
            if app.live.is_some() {
                app.books.len().to_string()
            } else {
                updates.to_string()
            },
            s(THEME.accent),
        ),
        Span::styled(
            if app.live.is_some() {
                " 本书籍。"
            } else {
                " 章更新待读。"
            },
            s(THEME.mute),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            app.books
                .first()
                .map(|book| format!("[继续阅读 {} · 第{}章]", book.title, book.read + 1))
                .unwrap_or_else(|| "[书架暂无书籍]".into()),
            s(THEME.accent),
        ),
        Span::styled("  or press c", s(THEME.dim)),
    ]));
    lines.push(Line::from(""));

    let menu = [
        ("打开书架", "b"),
        ("发现 · 按书源浏览", "e"),
        ("搜索书籍", "/"),
        (
            if app.live.is_some() {
                "导入本地 TXT / EPUB"
            } else {
                "导入本地书籍"
            },
            "o",
        ),
        ("书源管理", "s"),
        ("快捷键帮助", "?"),
    ];
    for (label, key) in &menu {
        lines.push(Line::from(vec![
            Span::styled("  ", s(THEME.fg)),
            Span::styled(pad(label, 22), sb(THEME.fg)),
            Span::styled(*key, s(THEME.mute)),
        ]));
    }
    lines.push(Line::from(""));
    if app.live.is_some() {
        lines.push(Line::from(Span::styled(
            "阅读进度自动保存 · 阅读时长统计待接入",
            s(THEME.dim),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled("今日阅读  ", s(THEME.dim)),
            Span::styled("2小时05分", s(THEME.hi)),
            Span::styled("    本周章节  ", s(THEME.dim)),
            Span::styled("146 章", s(THEME.hi)),
            Span::styled("    缓存  ", s(THEME.dim)),
            Span::styled("38.2 MB", s(THEME.hi)),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines).style(sbg(THEME.fg, THEME.bg)),
        content,
    );
}

// ── 书架 ────────────────────────────────────────────────────
fn draw_shelf(f: &mut Frame, app: &App, area: Rect, filter: ShelfFilter) {
    let focused = app.focus == Focus::Main;
    let list = app.shelf_list(filter);
    let groups = app.shelf_groups(filter);
    let title_name = match filter {
        ShelfFilter::All => "全部书籍",
        ShelfFilter::Local => "本地图书",
        ShelfFilter::Network => "网络图书",
    };

    // 左列表 + 右详情
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(36)])
        .split(area);

    // ── 左：列表面板
    let block = panel_full(
        vec![
            Span::styled("书架", sb(THEME.hi)),
            Span::styled(format!(" / {title_name}"), s(THEME.dim)),
        ],
        Some(Span::styled(format!("{} 本", list.len()), s(THEME.dim))),
        Some(Span::styled(
            if app.live.is_some() {
                "Enter 阅读  o 导入  x 删除/移除  m 分组  M 移动分组"
            } else {
                "enter 阅读  h/l 分组  s 排序  r 更新  x 移出"
            },
            s(THEME.dim),
        )),
        focused,
    );
    let inner = block.inner(cols[0]);
    f.render_widget(block, cols[0]);

    // 分组 tabs
    let mut tab_spans: Vec<Span> = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        if i == app.shelf_group {
            tab_spans.push(Span::styled(
                format!("[{g}]"),
                if focused {
                    s(THEME.accent)
                } else {
                    s(THEME.hi)
                },
            ));
        } else {
            tab_spans.push(Span::styled(format!(" {g} "), s(THEME.dim)));
        }
    }
    let sort_names = ["最近阅读", "书名", "更新数", "进度"];
    let tabs_line = Line::from({
        let mut spans = tab_spans;
        spans.push(Span::styled(
            format!("    排序: {} ↓", sort_names[app.shelf_sort]),
            s(THEME.dim),
        ));
        spans
    });

    // 表头
    let header = Line::from(vec![
        Span::styled("  ", s(THEME.dim)),
        Span::styled(pad("书名", 14), s(THEME.dim)),
        Span::styled(pad("作者", 10), s(THEME.dim)),
        Span::styled(pad("进度", 14), s(THEME.dim)),
        Span::styled("来源", s(THEME.dim)),
    ]);

    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Length(1), // header
            Constraint::Min(0),    // list
        ])
        .split(inner);

    f.render_widget(Paragraph::new(tabs_line), body_chunks[0]);
    f.render_widget(Paragraph::new(header), body_chunks[1]);

    let list_area = body_chunks[2];
    let h = list_area.height as usize;
    let scroll = list_scroll(app.shelf_sel, h, list.len());

    for (i, b) in list.iter().enumerate().skip(scroll) {
        let y = (i - scroll) as u16;
        if y >= list_area.height {
            break;
        }
        let sel = i == app.shelf_sel;
        let progress = (b.read as f64 + if b.read > 0 { 1.0 } else { 0.0 }) / b.total.max(1) as f64;
        let pct = (progress * 100.0).round() as u32;

        let mut title = b.title.to_string();
        if b.new_count > 0 {
            title = format!("{} +{}", b.title, b.new_count);
        }
        let origin = match b.kind {
            Kind::Local => "本地".to_string(),
            Kind::Network => b.origin.to_string(),
        };

        let mut content = vec![
            Span::styled(
                pad(&title, 14),
                if sel { sb(THEME.hi) } else { s(THEME.fg) },
            ),
            Span::styled(pad(&b.author, 10), s(THEME.mute)),
        ];
        content.extend(bar_spans(progress, 8));
        content.push(Span::styled(format!(" {pct}% "), s(THEME.dim)));
        content.push(Span::styled(
            origin,
            if b.kind == Kind::Local {
                s(THEME.info)
            } else {
                s(THEME.dim)
            },
        ));

        let line = row_line(sel, focused, content);
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x: list_area.x,
                y: list_area.y + y,
                width: list_area.width,
                height: 1,
            },
        );
    }

    // ── 右：详情
    if let Some(b) = list.get(app.shelf_sel) {
        draw_book_detail(f, b, cols[1], false);
    }
}

fn draw_book_detail(f: &mut Frame, b: &crate::data::Book, area: Rect, focused: bool) {
    let block = panel(vec![Span::styled("详情", s(THEME.mute))], None, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let progress = (b.read as f64 + if b.read > 0 { 1.0 } else { 0.0 }) / b.total.max(1) as f64;

    let lines: Vec<Line> = vec![
        Line::from(Span::styled(&b.title, sb(THEME.hi))),
        Line::from(Span::styled(&b.author, s(THEME.mute))),
        Line::from(vec![
            Span::styled(format!("#{} ", b.category), s(THEME.info)),
            Span::styled(
                format!("#{} ", b.status),
                if b.status == "完结" {
                    s(THEME.ok)
                } else {
                    s(THEME.accent)
                },
            ),
            Span::styled(format!("#{}字", b.words), s(THEME.dim)),
        ]),
        Line::from(""),
        kv("来源", &b.origin),
        kv("章节", &format!("{} 章", b.total)),
        kv("最新", &b.latest),
        kv("上次", &b.last_read),
        Line::from(""),
        Line::from({
            let mut spans = bar_spans(progress, 18);
            spans.push(Span::styled(
                format!(" {:.1}%", progress * 100.0),
                theme::s(THEME.dim),
            ));
            spans
        }),
        Line::from(""),
        Line::from(Span::styled("简介", s(THEME.dim))),
        Line::from(Span::styled(
            truncate(&b.intro, (inner.width as usize).saturating_sub(2)),
            s(THEME.fg),
        )),
    ];
    // 简单 wrap intro
    f.render_widget(
        Paragraph::new(lines).style(sbg(THEME.fg, THEME.bg)),
        inset(inner, 1, 1),
    );
}

fn kv(k: &str, v: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{k:4}"), s(THEME.dim)),
        Span::styled(v.to_string(), s(THEME.fg)),
    ])
}

// ── 发现 ────────────────────────────────────────────────────
fn draw_discover(f: &mut Frame, app: &App, area: Rect, source_idx: usize) {
    let focused = app.focus == Focus::Main;
    let source = match app.sources.get(source_idx) {
        Some(s) => s,
        None => return,
    };
    let list = app.discover_list(source_idx);
    let cats = &source.categories;

    let lat = if source.respond_ms < 0 {
        "超时".to_string()
    } else {
        format!("{}ms", source.respond_ms)
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(36)])
        .split(area);

    let block = panel_full(
        vec![
            Span::styled("发现", sb(THEME.hi)),
            Span::styled(format!(" / {}", source.name), s(THEME.dim)),
        ],
        Some(Span::styled(
            format!("{} · {lat}", source.url),
            s(THEME.dim),
        )),
        Some(Span::styled(
            if app.live.is_some() {
                "enter 试读  h/l 分类  n 续页  r 重试  R 刷新分类"
            } else {
                "enter 试读  a 加入书架  h/l 分类  j/k 移动"
            },
            s(THEME.dim),
        )),
        focused,
    );
    let inner = block.inner(cols[0]);
    f.render_widget(block, cols[0]);

    // tabs
    // Keep the selected category in view when a source exposes many tabs.
    let tab_width = inner.width as usize;
    let mut tab_start = 0usize;
    let mut used = 0usize;
    for i in (0..cats.len().min(app.discover_cat + 1)).rev() {
        let w = UnicodeWidthStr::width(format!(" {} ", cats[i]).as_str());
        if used + w > tab_width.saturating_sub(2) && i < app.discover_cat {
            break;
        }
        used += w;
        tab_start = i;
    }
    let mut tab_spans = Vec::new();
    if tab_start > 0 {
        tab_spans.push(Span::styled("… ", s(THEME.dim)));
    }
    used = if tab_start > 0 { 2 } else { 0 };
    for (i, c) in cats.iter().enumerate().skip(tab_start) {
        let label = format!(" {} ", c);
        if i != app.discover_cat && UnicodeWidthStr::width(label.as_str()) + used > tab_width {
            break;
        }
        if i == app.discover_cat {
            tab_spans.push(Span::styled(
                format!("[{}]", truncate(c, tab_width.saturating_sub(used + 2))),
                if focused {
                    s(THEME.accent)
                } else {
                    s(THEME.hi)
                },
            ));
        } else {
            tab_spans.push(Span::styled(format!(" {c} "), s(THEME.dim)));
        }
        used += UnicodeWidthStr::width(label.as_str());
    }
    let header = Line::from(vec![
        Span::styled("  ", s(THEME.dim)),
        Span::styled(pad("书名", 14), s(THEME.dim)),
        Span::styled(pad("作者", 10), s(THEME.dim)),
        Span::styled(pad("分类", 6), s(THEME.dim)),
        Span::styled("状态", s(THEME.dim)),
    ]);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(Line::from(tab_spans)), body[0]);
    f.render_widget(Paragraph::new(header), body[1]);

    let list_area = body[2];
    if list.is_empty() {
        if let Some(live) = &app.live {
            f.render_widget(
                Paragraph::new(live.status.as_str())
                    .style(s(THEME.dim))
                    .wrap(Wrap { trim: false }),
                list_area,
            );
        }
    }
    let h = list_area.height as usize;
    let scroll = list_scroll(app.discover_sel, h, list.len());
    for (i, b) in list.iter().enumerate().skip(scroll) {
        let y = (i - scroll) as u16;
        if y >= list_area.height {
            break;
        }
        let sel = i == app.discover_sel;
        let content = vec![
            Span::styled(
                pad(&b.title, 14),
                if sel { sb(THEME.hi) } else { s(THEME.fg) },
            ),
            Span::styled(pad(&b.author, 10), s(THEME.mute)),
            Span::styled(pad(&b.category, 6), s(THEME.info)),
            Span::styled(
                &b.status,
                if b.status == "完结" {
                    s(THEME.ok)
                } else {
                    s(THEME.accent)
                },
            ),
        ];
        let line = row_line(sel, focused, content);
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x: list_area.x,
                y: list_area.y + y,
                width: list_area.width,
                height: 1,
            },
        );
    }

    if let Some(b) = list.get(app.discover_sel) {
        draw_book_detail(f, b, cols[1], false);
    }
}

// ── 搜索 ────────────────────────────────────────────────────
fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Main;
    let results = app.search_results();

    let block = panel_full(
        vec![
            Span::styled("发现", sb(THEME.hi)),
            Span::styled(" / 搜索", s(THEME.dim)),
        ],
        Some(Span::styled(
            if app.search_query.is_empty() {
                format!(
                    "{} 个书源",
                    app.sources.iter().filter(|s| s.enabled).count()
                )
            } else {
                format!("{} 条结果", results.len())
            },
            s(THEME.dim),
        )),
        Some(Span::styled(
            if app.live.is_some() {
                "enter 试读  a 加入  i 输入  n 续页  r 重试  s 书源"
            } else {
                "enter 试读  a 加入  i 输入  j/k 移动"
            },
            s(THEME.dim),
        )),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // input
            Constraint::Length(1), // sep
            Constraint::Min(0),    // results
        ])
        .split(inner);

    // 输入行
    let cursor = if app.search_input_mode { "▋" } else { " " };
    let input_line = Line::from(vec![
        Span::styled(
            "❯ ",
            if app.search_input_mode {
                s(THEME.hi)
            } else {
                s(THEME.mute)
            },
        ),
        Span::styled(app.search_query.clone(), s(THEME.hi)),
        Span::styled(cursor, s(THEME.accent)),
        if app.search_query.is_empty() && !app.search_input_mode {
            Span::styled(" 输入书名或作者，按 i 开始输入", s(THEME.dim))
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(input_line), body[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(body[1].width as usize),
            s(THEME.border),
        ))),
        body[1],
    );

    let list_area = body[2];
    if app.search_query.trim().is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if app.live.is_some() {
                    "  导入书源后，输入书名或作者并按 Enter 搜索。"
                } else {
                    "  输入关键词后按 Enter。试试「诡秘」「三体」「远瞳」。"
                },
                s(THEME.dim),
            ))),
            list_area,
        );
        return;
    }
    if results.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  没有找到「{}」相关书籍。", app.search_query.trim()),
                s(THEME.dim),
            ))),
            list_area,
        );
        return;
    }

    let selected_origin = results
        .get(app.search_sel.min(results.len().saturating_sub(1)))
        .map(|b| b.origin.as_str());
    let mut groups: Vec<(&str, Vec<(usize, &Book)>)> = Vec::new();
    for (index, book) in results.iter().enumerate() {
        let origin = book.origin.as_str();
        if let Some((_, items)) = groups.iter_mut().find(|(name, _)| *name == origin) {
            items.push((index, *book));
        } else {
            groups.push((origin, vec![(index, *book)]));
        }
    }
    let mut rows: Vec<(bool, usize, &Book)> = Vec::new();
    for (origin, items) in groups {
        rows.push((
            true,
            items.first().map(|(i, _)| *i).unwrap_or(0),
            items[0].1,
        ));
        if app.search_expanded && Some(origin) == selected_origin {
            rows.extend(items.into_iter().map(|(i, b)| (false, i, b)));
        }
    }
    let selected_row = rows
        .iter()
        .position(|(header, i, _)| {
            if app.search_expanded {
                !*header && *i == app.search_sel
            } else {
                *header && *i == app.search_sel
            }
        })
        .unwrap_or(0);
    let h = list_area.height as usize;
    let scroll = list_scroll(selected_row, h, rows.len());
    for (row, (header, i, b)) in rows.iter().enumerate().skip(scroll) {
        let y = (row - scroll) as u16;
        if y >= list_area.height {
            break;
        }
        if *header {
            let selected = Some(b.origin.as_str()) == selected_origin;
            let expanded = selected && app.search_expanded;
            let line = Line::from(vec![
                Span::styled(
                    if expanded {
                        "▾ "
                    } else if selected {
                        "› "
                    } else {
                        "▸ "
                    },
                    s(THEME.accent),
                ),
                Span::styled(
                    format!(
                        "{}  ({} 个结果)",
                        b.origin,
                        results.iter().filter(|x| x.origin == b.origin).count()
                    ),
                    sb(THEME.hi),
                ),
            ]);
            f.render_widget(
                Paragraph::new(line),
                Rect {
                    x: list_area.x,
                    y: list_area.y + y,
                    width: list_area.width,
                    height: 1,
                },
            );
            continue;
        }
        let sel = *i == app.search_sel;
        let content = vec![
            Span::styled(
                pad(&b.title, 16),
                if sel { sb(THEME.hi) } else { s(THEME.fg) },
            ),
            Span::styled(pad(&b.author, 12), s(THEME.mute)),
            Span::styled(
                if b.kind == Kind::Local {
                    "本地"
                } else {
                    b.origin.as_str()
                },
                s(THEME.info),
            ),
        ];
        let line = row_line(sel, focused && !app.search_input_mode, content);
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x: list_area.x,
                y: list_area.y + y,
                width: list_area.width,
                height: 1,
            },
        );
    }
}

// ── 帮助浮层 ────────────────────────────────────────────────
fn draw_help(f: &mut Frame, area: Rect, live: bool) {
    let r = center_rect(area, 76, 24);
    // 清出一块 + 铺底
    ratatui::widgets::Clear.render(r, f.buffer_mut());
    fill_bg(r, f.buffer_mut());

    let block = panel(
        vec![Span::styled("快捷键", sb(THEME.hi))],
        Some(Span::styled("esc / ? 关闭", s(THEME.dim))),
        true,
    );
    let inner = block.inner(r);
    f.render_widget(block, r);

    let groups: &[(&str, &[(&str, &str)])] = &[
        (
            "全局",
            &[
                ("tab", "侧栏 ⇄ 主体"),
                ("/", "搜索"),
                ("?", "帮助"),
                ("ctrl+b", "显隐侧栏"),
                ("q", "退出"),
            ],
        ),
        (
            "列表",
            &[
                ("j k", "上下移动"),
                ("h l", "切换分组/分类"),
                ("g G", "首/尾"),
                ("enter", "打开"),
                ("space", "折叠 section"),
            ],
        ),
        (
            "书架",
            if live {
                &[
                    ("s", "切换排序"),
                    ("x", "移出书架"),
                    ("o", "导入本地书"),
                    ("m/M", "管理/移动分组"),
                ]
            } else {
                &[("s", "切换排序"), ("r", "检查更新"), ("x", "移出书架")]
            },
        ),
        (
            "发现/搜索",
            if live {
                &[
                    ("a", "加入书架"),
                    ("i", "聚焦输入"),
                    ("h l", "切换分类"),
                    ("n", "加载后续页"),
                    ("r", "重试失败页"),
                    ("s", "选择书源"),
                ]
            } else {
                &[("a", "加入书架"), ("i", "聚焦输入"), ("h l", "切换分类")]
            },
        ),
    ];

    let mut lines = Vec::new();
    for (g, items) in groups {
        lines.push(Line::from(Span::styled(format!("── {g}"), s(THEME.dim))));
        for pair in items.chunks(2) {
            let mut spans = Vec::new();
            for (k, d) in pair {
                spans.push(Span::styled(format!("  {k} "), s(THEME.accent)));
                spans.push(Span::styled(format!("{d}  "), s(THEME.fg)));
            }
            lines.push(Line::from(spans));
        }
    }
    f.render_widget(Paragraph::new(lines), inset(inner, 2, 1));
}

fn draw_reader_help(f: &mut Frame, area: Rect, live: bool, horizontal: bool) {
    let r = center_rect(area, 68, 20);
    ratatui::widgets::Clear.render(r, f.buffer_mut());
    let block = panel(
        vec![Span::styled("阅读快捷键", sb(THEME.hi))],
        Some(Span::styled("esc / ? 关闭", s(THEME.dim))),
        true,
    );
    let inner = block.inner(r);
    f.render_widget(block, r);
    let lines = [
        "Tab       章节目录 ⇄ 正文",
        "目录 j/k  上下选择；Enter 打开章节",
        if horizontal {
            "正文 ←/→  或 h/l 整页翻页（按横向方向设置）"
        } else {
            "正文 j/k  逐行滚动"
        },
        "Space     下一页；PgUp/PgDn 翻页（到边界自动切章）",
        "A         自动滚动/翻页启停；底部进度条显示下一次动作",
        "p         启动听书；播放控制见底栏，s 停止",
        "[ / ]     上一章 / 下一章",
        "g / G     目录首尾 / 正文首尾",
        "t         聚焦当前章节目录",
        "s         选择其他书源（真实模式）",
        "a         将当前网络书加入书架并保存当前位置",
        "Ctrl+B    显示 / 隐藏章节目录",
        "q / Esc   返回进入阅读前的页面",
        "",
        if live {
            "书架内图书自动保存进度；试读须按 a 加入。换源从匹配章节章首阅读。"
        } else {
            "预览使用演示正文；章节进度仅保留在本次运行中。"
        },
    ];
    f.render_widget(
        Paragraph::new(
            lines
                .iter()
                .map(|line| Line::from(*line))
                .collect::<Vec<_>>(),
        )
        .style(s(THEME.fg)),
        inset(inner, 1, 1),
    );
}

#[cfg(test)]
mod home_layout_tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn has_braille(terminal: &Terminal<TestBackend>) -> bool {
        terminal.backend().buffer().content.iter().any(|cell| {
            cell.symbol()
                .chars()
                .next()
                .is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
        })
    }

    #[test]
    fn home_logo_has_a_separate_wide_column_and_is_hidden_when_narrow() {
        for (width, expected_logo) in [(80, false), (140, true)] {
            let mut app = App::new();
            app.focus = Focus::Main;
            let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
            terminal
                .draw(|frame| draw_home(frame, &app, frame.area()))
                .unwrap();
            assert_eq!(has_braille(&terminal), expected_logo, "width={width}");
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            // Wide characters have blank continuation cells in TestBackend.
            let text = text.replace(' ', "");
            assert!(text.contains("基于legado"));
            assert!(text.contains("快捷键帮助"));
        }
    }
}
