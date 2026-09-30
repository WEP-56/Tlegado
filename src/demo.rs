//! 阶段 2：离线演示状态，不执行网络请求或持久化。
use crate::{
    app::{App, Focus, Route, ToastTone},
    data::{Book, Source},
    reader::{chapter_title, Reader},
    theme::*,
};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::Paragraph,
    Frame,
};

#[derive(Clone)]
pub struct History {
    pub book: Book,
    pub time: String,
}
#[derive(Clone)]
pub struct Rule {
    pub name: &'static str,
    pub pattern: &'static str,
    pub replacement: &'static str,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Idle,
    Running(u8),
    Ok(i32),
    Timeout,
    Cancelled,
}
impl Check {
    fn label(self) -> String {
        match self {
            Self::Idle => "未测试".into(),
            Self::Running(_) => "测试中…".into(),
            Self::Ok(ms) => format!("{ms} ms"),
            Self::Timeout => "超时".into(),
            Self::Cancelled => "已取消".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs(pub [usize; 7]);
impl Default for Prefs {
    fn default() -> Self {
        Self([0, 2, 0, 1, 0, 0, 0])
    }
}
pub const PREFS: [(&str, &[&str]); 7] = [
    ("配色主题", &["终端默认", "护眼绿", "羊皮纸", "高对比"]),
    ("正文行宽", &["自适应", "28 字", "36 字", "44 字"]),
    ("行间空行", &["0", "1"]),
    ("段首缩进", &["0 字", "2 字"]),
    ("翻页方式", &["整页", "逐行"]),
    ("进度显示", &["开", "关"]),
    ("正文净化", &["开", "关"]),
];
pub struct Demo {
    pub history: Vec<History>,
    pub rules: Vec<Rule>,
    pub checks: Vec<Check>,
    pub prefs: Prefs,
    pub history_sel: usize,
    pub source_sel: usize,
    pub rule_sel: usize,
    pub pref_sel: usize,
    pub json: bool,
    pub detail_scroll: usize,
}
impl Demo {
    pub fn new(books: &[Book], source_count: usize) -> Self {
        Self {
            history: books
                .iter()
                .filter(|b| b.read > 0)
                .take(8)
                .cloned()
                .map(|mut book| {
                    book.read = book.read.min(book.total.saturating_sub(1));
                    History {
                        time: book.last_read.clone(),
                        book,
                    }
                })
                .collect(),
            rules: vec![
                Rule {
                    name: "去除站点水印",
                    pattern: "（演示站点水印）",
                    replacement: "",
                    enabled: true,
                },
                Rule {
                    name: "去除翻页提示",
                    pattern: "本章未完，请点击下一页继续阅读",
                    replacement: "",
                    enabled: true,
                },
                Rule {
                    name: "去除求票尾巴",
                    pattern: "求月票！",
                    replacement: "",
                    enabled: true,
                },
                Rule {
                    name: "统一省略号",
                    pattern: "...",
                    replacement: "……",
                    enabled: false,
                },
            ],
            checks: vec![Check::Idle; source_count],
            prefs: Prefs::default(),
            history_sel: 0,
            source_sel: 0,
            rule_sel: 0,
            pref_sel: 0,
            json: false,
            detail_scroll: 0,
        }
    }
    pub fn record(&mut self, mut book: Book, chapter: u32) {
        book.read = chapter;
        self.history.retain(|h| h.book.id != book.id);
        self.history.insert(
            0,
            History {
                book,
                time: "本次阅读".into(),
            },
        );
        self.history_sel = 0;
    }
    pub fn tick(&mut self, sources: &[Source]) {
        for (state, source) in self.checks.iter_mut().zip(sources) {
            if let Check::Running(ticks) = *state {
                *state = if ticks > 1 {
                    Check::Running(ticks - 1)
                } else if source.respond_ms < 0 {
                    Check::Timeout
                } else {
                    Check::Ok(source.respond_ms)
                };
            }
        }
    }
}
pub fn purify(text: &str, rules: &[Rule]) -> String {
    rules
        .iter()
        .filter(|r| r.enabled)
        .fold(text.to_owned(), |text, r| {
            text.replace(r.pattern, r.replacement)
        })
}
fn move_selection(sel: &mut usize, len: usize, code: KeyCode) -> bool {
    match code {
        KeyCode::Down | KeyCode::Char('j') => {
            *sel = sel.saturating_add(1).min(len.saturating_sub(1))
        }
        KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
        KeyCode::Home | KeyCode::Char('g') => *sel = 0,
        KeyCode::End | KeyCode::Char('G') => *sel = len.saturating_sub(1),
        _ => return false,
    }
    true
}
impl App {
    pub(crate) fn demo_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::PageDown => {
                self.demo.detail_scroll = self.demo.detail_scroll.saturating_add(5);
                return;
            }
            KeyCode::PageUp => {
                self.demo.detail_scroll = self.demo.detail_scroll.saturating_sub(5);
                return;
            }
            KeyCode::Down
            | KeyCode::Up
            | KeyCode::Char('j' | 'k' | 'g' | 'G')
            | KeyCode::Home
            | KeyCode::End => self.demo.detail_scroll = 0,
            _ => {}
        }
        if code == KeyCode::Char('q') {
            self.focus = Focus::Sidebar;
            return;
        }
        match self.route().clone() {
            Route::History => {
                if move_selection(&mut self.demo.history_sel, self.demo.history.len(), code) {
                    return;
                }
                if code == KeyCode::Enter {
                    if let Some(h) = self.demo.history.get(self.demo.history_sel) {
                        self.open_reader(h.book.clone());
                    }
                } else if code == KeyCode::Char('x') && !self.demo.history.is_empty() {
                    self.demo.history.remove(self.demo.history_sel);
                    self.demo.history_sel = self
                        .demo
                        .history_sel
                        .min(self.demo.history.len().saturating_sub(1));
                    self.rebuild_nav();
                }
            }
            Route::Sources => {
                if move_selection(&mut self.demo.source_sel, self.sources.len(), code) {
                    return;
                }
                let i = self.demo.source_sel;
                if let Some(source) = self.sources.get_mut(i) {
                    match code {
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            source.enabled = !source.enabled;
                            self.rebuild_nav();
                        }
                        KeyCode::Char('e') => {
                            source.explore = !source.explore;
                            if source.explore {
                                self.source_browser.active_explore = Some(source.id.clone());
                            } else if self.source_browser.active_explore.as_deref()
                                == Some(source.id.as_str())
                            {
                                self.source_browser.active_explore = None;
                            }
                            self.rebuild_nav();
                        }
                        KeyCode::Char('t') => self.demo.checks[i] = Check::Running(8),
                        KeyCode::Char('T') => self.demo.checks.fill(Check::Running(8)),
                        KeyCode::Char('c') => {
                            for check in &mut self.demo.checks {
                                if matches!(check, Check::Running(_)) {
                                    *check = Check::Cancelled;
                                }
                            }
                        }
                        KeyCode::Char('v') => {
                            self.demo.json = !self.demo.json;
                            self.demo.detail_scroll = 0;
                        }
                        _ => {}
                    }
                }
            }
            Route::Purify => {
                if move_selection(&mut self.demo.rule_sel, self.demo.rules.len(), code) {
                    return;
                }
                if matches!(code, KeyCode::Enter | KeyCode::Char(' ')) {
                    if let Some(rule) = self.demo.rules.get_mut(self.demo.rule_sel) {
                        rule.enabled = !rule.enabled;
                        self.rebuild_nav();
                    }
                }
            }
            Route::Prefs => {
                if move_selection(&mut self.demo.pref_sel, PREFS.len(), code) {
                    return;
                }
                let i = self.demo.pref_sel;
                let n = PREFS[i].1.len();
                match code {
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.demo.prefs.0[i] = (self.demo.prefs.0[i] + n - 1) % n
                    }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                        self.demo.prefs.0[i] = (self.demo.prefs.0[i] + 1) % n
                    }
                    KeyCode::Char('R') => {
                        self.demo.prefs = Prefs::default();
                        self.toast("已恢复默认阅读偏好（本次运行）", ToastTone::Info);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    pub(crate) fn add_demo_book(&mut self, book: Book) {
        if let Some(live) = &self.live {
            if let Some(domain) = live.domain.get(&book.id) {
                self.commands
                    .push(crate::jobs::Command::Add(domain.clone()));
            }
            return;
        }
        if self
            .books
            .iter()
            .any(|b| b.id == book.id || (b.title == book.title && b.author == book.author))
        {
            self.toast("这本书已在书架中", ToastTone::Info);
        } else {
            let title = book.title.clone();
            self.books.push(book);
            self.rebuild_nav();
            self.toast(format!("已将《{title}》加入书架"), ToastTone::Ok);
        }
    }
}

fn panes(area: Rect) -> (Rect, Rect) {
    let wide = area.width >= 90;
    let chunks = Layout::default()
        .direction(if wide {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);
    (chunks[0], chunks[1])
}
fn details(f: &mut Frame, area: Rect, title: &str, text: String, offset: &mut usize) {
    let block = panel_full(
        vec![Span::styled(title, s(THEME.mute))],
        None,
        Some(Span::styled("PgUp/Dn 滚动详情", s(THEME.dim))),
        false,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<String> = text
        .lines()
        .flat_map(|line| {
            if line.is_empty() {
                vec![String::new()]
            } else {
                crate::reader::wrap(line, inner.width.max(1) as usize)
            }
        })
        .collect();
    *offset = (*offset).min(lines.len().saturating_sub(inner.height as usize));
    let visible = lines
        .into_iter()
        .skip(*offset)
        .take(inner.height as usize)
        .map(ratatui::text::Line::from)
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(visible).style(s(THEME.fg)), inner);
}
fn list(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    hint: &str,
    selected: usize,
    rows: Vec<String>,
) {
    let block = panel_full(
        vec![Span::styled(title, sb(THEME.hi))],
        Some(Span::styled(format!("{} 项", rows.len()), s(THEME.dim))),
        Some(Span::styled(hint, s(THEME.dim))),
        app.focus == Focus::Main,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(
                if matches!(app.route(), Route::Sources) && app.live.is_some() {
                    "暂无书源 · 按 i 导入 Legado JSON"
                } else {
                    "暂无记录"
                },
            )
            .style(s(THEME.dim)),
            inner,
        );
        return;
    }
    let start = list_scroll(selected, inner.height as usize, rows.len());
    for (y, (i, text)) in rows
        .iter()
        .enumerate()
        .skip(start)
        .take(inner.height as usize)
        .enumerate()
    {
        f.render_widget(
            Paragraph::new(row_line(
                i == selected,
                app.focus == Focus::Main,
                vec![Span::styled(
                    truncate(text, inner.width.saturating_sub(2) as usize),
                    s(THEME.fg),
                )],
            )),
            Rect::new(inner.x, inner.y + y as u16, inner.width, 1),
        );
    }
}
fn json_string(value: &str) -> String {
    let quote = char::from(34);
    let slash = char::from(92);
    let mut out = String::from(quote);
    for ch in value.chars() {
        if ch == quote || ch == slash {
            out.push(slash);
            out.push(ch);
        } else if ch.is_control() {
            out.push_str(&format!("{slash}u{:04x}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out.push(quote);
    out
}
pub fn source_json(source: &Source) -> String {
    [
        "{".into(),
        format!(
            "  {}: {},",
            json_string("bookSourceName"),
            json_string(&source.name)
        ),
        format!(
            "  {}: {},",
            json_string("bookSourceUrl"),
            json_string(&source.url)
        ),
        format!(
            "  {}: {},",
            json_string("bookSourceGroup"),
            json_string(&source.group)
        ),
        format!("  {}: {},", json_string("enabled"), source.enabled),
        format!("  {}: {}", json_string("enabledExplore"), source.explore),
        "}".into(),
    ]
    .join(&char::from(10).to_string())
}
pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.demo.json && matches!(app.route(), Route::Sources) {
        if let Some(source) = app.sources.get(app.demo.source_sel) {
            details(
                f,
                area,
                if app.live.is_some() {
                    "书源 JSON · v/Esc 返回"
                } else {
                    "JSON 预览 · v/Esc 返回 · 非完整解析规则"
                },
                app.live
                    .as_ref()
                    .and_then(|l| l.sources.iter().find(|s| s.book_source_url == source.id))
                    .and_then(|s| serde_json::to_string_pretty(s).ok())
                    .unwrap_or_else(|| source_json(source)),
                &mut app.demo.detail_scroll,
            );
        }
        return;
    }
    let (left, right) = panes(area);
    match app.route() {
        Route::History => {
            let rows = app
                .demo
                .history
                .iter()
                .map(|h| {
                    format!(
                        "{}  {} · 第{}章",
                        pad(&h.book.title, 20),
                        h.time,
                        h.book.read + 1
                    )
                })
                .collect();
            list(
                f,
                app,
                left,
                "阅读历史",
                "enter 恢复  x 删除",
                app.demo.history_sel,
                rows,
            );
            let text = app
                .demo
                .history
                .get(app.demo.history_sel)
                .map(|h| {
                    [
                        h.book.title.to_string(),
                        h.book.author.to_string(),
                        app.live
                            .as_ref()
                            .and_then(|l| l.domain.get(&h.book.id))
                            .and_then(|b| b.dur_chapter_title.clone())
                            .unwrap_or_else(|| chapter_title(h.book.read)),
                        h.book.origin.to_string(),
                        String::new(),
                        "Enter 从此章节继续阅读".into(),
                        if app.live.is_some() {
                            "进度已保存到本地数据目录".into()
                        } else {
                            "记录仅保留在本次运行".into()
                        },
                    ]
                    .join(&char::from(10).to_string())
                })
                .unwrap_or_else(|| "从书架或发现进入阅读，返回后自动记录进度。".into());
            details(f, right, "阅读记录", text, &mut app.demo.detail_scroll);
        }
        Route::Sources => {
            let rows = app
                .sources
                .iter()
                .enumerate()
                .map(|(i, source)| {
                    let status = app.demo.checks[i].label();
                    format!(
                        "{} {} {} {}",
                        if source.enabled { "●" } else { "○" },
                        pad(&source.name, 16),
                        if source.explore {
                            "发现开"
                        } else {
                            "发现关"
                        },
                        status
                    )
                })
                .collect();
            list(
                f,
                app,
                left,
                if app.live.is_some() {
                    "书源管理 · Legado"
                } else {
                    "书源管理 · 离线演示"
                },
                if app.live.is_some() {
                    "i 导入  o 导出  空格 启停  e 发现  v JSON"
                } else {
                    "空格 启停  e 发现  t 测试  v JSON"
                },
                app.demo.source_sel,
                rows,
            );
            if let Some(source) = app.sources.get(app.demo.source_sel) {
                let text = [
                    source.name.to_string(),
                    format!("分组：{}", source.group),
                    source.url.to_string(),
                    format!("分类：{}", source.categories.join(" / ")),
                    if app.live.is_some() {
                        "导入与启停状态已持久化".into()
                    } else {
                        format!(
                            "测试：{}（模拟）",
                            app.demo.checks[app.demo.source_sel].label()
                        )
                    },
                    if app.live.is_some() {
                        "i 导入 JSON · o 导出到新文件".into()
                    } else {
                        "T 测试全部 · c 取消测试".into()
                    },
                    if app.live.is_some() {
                        "/ 搜索 · Enter 打开书籍".into()
                    } else {
                        "仅模拟延时与超时，无网络请求。".into()
                    },
                    "v 查看 JSON".into(),
                ]
                .join(&char::from(10).to_string());
                details(f, right, "书源详情", text, &mut app.demo.detail_scroll);
            }
        }
        Route::Purify => {
            if app.live.is_some() {
                crate::library::draw_rules(f, app, area);
                return;
            }
            let rows = app
                .demo
                .rules
                .iter()
                .map(|r| format!("{} {}", if r.enabled { "●" } else { "○" }, r.name))
                .collect();
            list(
                f,
                app,
                left,
                "净化规则",
                "j/k 选择  空格/enter 启停",
                app.demo.rule_sel,
                rows,
            );
            if let Some(rule) = app.demo.rules.get(app.demo.rule_sel) {
                let sample = format!("夜色渐深...{}故事仍在继续。", rule.pattern);
                let text = [
                    format!(
                        "{} · 命中 {} 处",
                        rule.name,
                        sample.matches(rule.pattern).count()
                    ),
                    format!("匹配：{}", rule.pattern),
                    format!(
                        "替换：{}",
                        if rule.replacement.is_empty() {
                            "（删除）"
                        } else {
                            rule.replacement
                        }
                    ),
                    format!("原文：{sample}"),
                    format!("处理后：{}", purify(&sample, &app.demo.rules)),
                ]
                .join(&char::from(10).to_string());
                details(
                    f,
                    right,
                    "匹配预览 · 字面替换",
                    text,
                    &mut app.demo.detail_scroll,
                );
            }
        }
        Route::Prefs => {
            let imported = app.live.as_ref().and_then(|l| l.library.layout.as_ref());
            let rows = PREFS
                .iter()
                .enumerate()
                .map(|(i, (name, options))| {
                    let active = imported.is_some_and(|l| match i {
                        0 => l.foreground.is_some() || l.background.is_some(),
                        2 => l.line_gap.is_some(),
                        3 => l.indent.is_some(),
                        _ => false,
                    });
                    format!(
                        "{} ‹{}›",
                        pad(name, 14),
                        if active {
                            "导入配置"
                        } else {
                            options[app.demo.prefs.0[i]]
                        }
                    )
                })
                .collect();
            list(
                f,
                app,
                left,
                if app.live.is_some() {
                    "阅读偏好 · 自动保存 · i 导入"
                } else {
                    "阅读偏好 · 本次运行"
                },
                "j/k 选择  h/l 修改  i 导入 JSON  R 重置",
                app.demo.pref_sel,
                rows,
            );
            let mut preview = Reader::new(
                app.books
                    .first()
                    .cloned()
                    .unwrap_or_else(|| crate::data::shelf().remove(0)),
                false,
            );
            preview.options = app.demo.prefs.clone();
            if let Some(live) = &app.live {
                preview.layout = live.library.layout.clone();
                if preview.layout.is_some() {
                    preview.set_real(vec!["排版预览".into()], "这是导入排版的实际预览。颜色、粗体、缩进与间距已应用。\n终端间距以行为单位，字体与字号由终端控制。".repeat(12), 0);
                }
            }
            preview.rules = app.demo.rules.clone();
            crate::reader::draw_page(f, &mut preview, right, false);
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::KeyCode;

    #[test]
    fn history_records_reader_progress_and_can_restore() {
        let mut app = App::new();
        let book = app.books[0].clone();
        app.demo.record(book.clone(), 7);
        assert_eq!(app.demo.history[0].book.id, book.id);
        assert_eq!(app.demo.history[0].book.read, 7);
        app.goto_id_for_test("set:history");
        app.demo_key(KeyCode::Enter);
        assert_eq!(app.reader.as_ref().unwrap().book.id, book.id);
    }

    #[test]
    fn source_controls_toggle_test_and_json() {
        let mut app = App::new();
        app.goto_id_for_test("set:sources");
        let enabled = app.sources[0].enabled;
        app.demo_key(KeyCode::Char(' '));
        assert_eq!(app.sources[0].enabled, !enabled);
        app.demo_key(KeyCode::Char('t'));
        assert!(matches!(app.demo.checks[0], Check::Running(_)));
        app.demo_key(KeyCode::Char('v'));
        assert!(app.demo.json);
        assert!(source_json(&app.sources[0]).contains("bookSourceName"));
    }

    #[test]
    fn purify_and_preferences_change_preview_state() {
        let mut app = App::new();
        assert_eq!(
            purify("内容...（演示站点水印）", &app.demo.rules),
            "内容..."
        );
        app.goto_id_for_test("set:purify");
        app.demo_key(KeyCode::Char(' '));
        assert!(!app.demo.rules[0].enabled);
        app.goto_id_for_test("set:prefs");
        app.demo_key(KeyCode::Char('l'));
        assert_eq!(app.demo.prefs.0[0], 1);
        app.demo_key(KeyCode::Char('R'));
        assert_eq!(app.demo.prefs, Prefs::default());
    }
}
