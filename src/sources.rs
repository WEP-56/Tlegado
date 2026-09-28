//! A bounded navigation entry and a filtered, virtualized source manager.
use crate::{
    app::{App, Focus, Route, ToastTone},
    jobs::Command,
    theme::{center_rect, list_scroll, panel, row_line, s, sb, truncate, THEME},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Span,
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct SourceBrowser {
    pub filter: String,
    pub editing: bool,
    pub cursor: usize,
    pub selected: BTreeSet<String>,
    pub delete: Option<Vec<String>>,
    pub active_explore: Option<String>,
    pub mode: usize,
}
const MODES: [&str; 4] = ["全部", "已启用", "已禁用", "可探索"];

impl App {
    pub(crate) fn source_indices(&self) -> Vec<usize> {
        let query = self.source_browser.filter.trim().to_lowercase();
        let explore = matches!(self.route(), Route::ExploreSources);
        self.sources
            .iter()
            .enumerate()
            .filter(|(_, source)| {
                let available = source.enabled && source.explore && !source.categories.is_empty();
                (!explore || available)
                    && (explore
                        || match self.source_browser.mode {
                            1 => source.enabled,
                            2 => !source.enabled,
                            3 => available,
                            _ => true,
                        })
                    && (query.is_empty()
                        || [&source.name, &source.group, &source.url]
                            .iter()
                            .any(|v| v.to_lowercase().contains(&query)))
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn source_targets(&self, visible: &[usize]) -> Vec<String> {
        if self.source_browser.selected.is_empty() {
            visible
                .get(self.source_browser.cursor)
                .map(|&i| vec![self.sources[i].id.clone()])
                .unwrap_or_default()
        } else {
            self.source_browser.selected.iter().cloned().collect()
        }
    }

    pub(crate) fn source_browser_key(&mut self, key: KeyEvent) -> bool {
        if self.help
            || self.reader.is_some()
            || self.demo.json
            || self
                .live
                .as_ref()
                .is_some_and(|l| !l.ready || l.prompt.is_some())
        {
            return false;
        }
        if self.source_browser.delete.is_some() {
            match key.code {
                KeyCode::Char('y') => {
                    let keys = self.source_browser.delete.take().unwrap();
                    self.cancel_live_reads();
                    self.commands.push(Command::DeleteSources(keys));
                    self.toast("正在删除书源…", ToastTone::Info);
                }
                KeyCode::Esc | KeyCode::Char('n') => self.source_browser.delete = None,
                _ => {}
            }
            return true;
        }
        if self.source_browser.editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Tab => self.source_browser.editing = false,
                KeyCode::Backspace => {
                    self.source_browser.filter.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.source_browser.filter.push(c)
                }
                _ => {}
            }
            self.source_browser.cursor = 0;
            self.source_browser.selected.clear();
            return true;
        }
        if self.focus != Focus::Main {
            return false;
        }
        if matches!(self.route(), Route::Discover { .. }) && key.code == KeyCode::Backspace {
            self.goto_id("discover:sources");
            return true;
        }
        let explore = matches!(self.route(), Route::ExploreSources);
        if !(explore || matches!(self.route(), Route::Sources) && self.live.is_some()) {
            return false;
        }
        let visible = self.source_indices();
        let last = visible.len().saturating_sub(1);
        self.source_browser.cursor = self.source_browser.cursor.min(last);
        match key.code {
            KeyCode::Char('/') => self.source_browser.editing = true,
            KeyCode::Char('j') | KeyCode::Down => {
                self.source_browser.cursor = (self.source_browser.cursor + 1).min(last)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.source_browser.cursor = self.source_browser.cursor.saturating_sub(1)
            }
            KeyCode::PageDown => {
                self.source_browser.cursor = (self.source_browser.cursor + 20).min(last)
            }
            KeyCode::PageUp => {
                self.source_browser.cursor = self.source_browser.cursor.saturating_sub(20)
            }
            KeyCode::Char('g') | KeyCode::Home => self.source_browser.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.source_browser.cursor = last,
            KeyCode::Char('f') if !explore => {
                self.source_browser.mode = (self.source_browser.mode + 1) % MODES.len();
                self.source_browser.cursor = 0;
                self.source_browser.selected.clear();
            }
            KeyCode::Esc if !self.source_browser.selected.is_empty() => {
                self.source_browser.selected.clear()
            }
            KeyCode::Esc if !self.source_browser.filter.is_empty() => {
                self.source_browser.filter.clear();
                self.source_browser.cursor = 0;
            }
            KeyCode::Enter if explore => {
                if let Some(&i) = visible.get(self.source_browser.cursor) {
                    let id = self.sources[i].id.clone();
                    self.source_browser.active_explore = Some(id.clone());
                    self.rebuild_nav();
                    self.goto_id(&format!("discover:{id}"));
                    self.focus = Focus::Main;
                }
            }
            KeyCode::Char(' ') if !explore => {
                if let Some(&i) = visible.get(self.source_browser.cursor) {
                    let id = self.sources[i].id.clone();
                    if !self.source_browser.selected.remove(&id) {
                        self.source_browser.selected.insert(id);
                    }
                }
            }
            KeyCode::Char('a') if !explore => {
                self.source_browser.selected = visible
                    .iter()
                    .map(|&i| self.sources[i].id.clone())
                    .collect();
            }
            KeyCode::Char('A') if !explore => self.source_browser.selected.clear(),
            KeyCode::Char('x') | KeyCode::Delete if !explore => {
                let keys = self.source_targets(&visible);
                if !keys.is_empty() {
                    self.source_browser.delete = Some(keys);
                }
            }
            KeyCode::Enter | KeyCode::Char('e' | 'E' | '+' | '-') if !explore => {
                let keys = self.source_targets(&visible);
                if !keys.is_empty() {
                    let is_explore = matches!(key.code, KeyCode::Char('e' | 'E'));
                    let key_set: BTreeSet<_> = keys.iter().collect();
                    let enabled = match key.code {
                        KeyCode::Char('+') => true,
                        KeyCode::Char('-' | 'E') => false,
                        _ => !self
                            .sources
                            .iter()
                            .filter(|s| key_set.contains(&s.id))
                            .all(|s| if is_explore { s.explore } else { s.enabled }),
                    };
                    self.cancel_live_reads();
                    self.commands.push(Command::SetSources {
                        keys,
                        explore: is_explore,
                        enabled,
                    });
                    self.toast("正在更新书源…", ToastTone::Info);
                }
            }
            KeyCode::Char('v') if !explore => {
                if let Some(&i) = visible.get(self.source_browser.cursor) {
                    self.demo.source_sel = i;
                    self.demo.json = true;
                    self.demo.detail_scroll = 0;
                }
            }
            _ => return false,
        }
        true
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let explore = matches!(app.route(), Route::ExploreSources);
    let visible = app.source_indices();
    let selected = app
        .source_browser
        .cursor
        .min(visible.len().saturating_sub(1));
    let title = if explore {
        "探索书源"
    } else {
        "书源管理"
    };
    let block = panel(
        vec![Span::styled(
            format!(
                "{title} · {} / {} · 已选 {}",
                visible.len(),
                app.sources.len(),
                app.source_browser.selected.len()
            ),
            sb(THEME.hi),
        )],
        None,
        app.focus == Focus::Main,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(inner);
    let mode = if explore {
        "可探索"
    } else {
        MODES[app.source_browser.mode]
    };
    f.render_widget(
        Paragraph::new(format!(
            "[{mode}] 筛选名称 / 分组 / URL：{}{}",
            app.source_browser.filter,
            if app.source_browser.editing {
                "▋"
            } else {
                "  (/ 编辑)"
            }
        ))
        .style(s(THEME.fg))
        .wrap(Wrap { trim: false }),
        regions[0],
    );
    let list = regions[1];
    let start = list_scroll(selected, list.height as usize, visible.len());
    if visible.is_empty() {
        f.render_widget(
            Paragraph::new(if explore {
                "没有匹配的可探索书源。请在书源管理启用书源与探索。"
            } else {
                "没有匹配的书源。i 导入 JSON；Esc 清空筛选；f 切换状态。"
            })
            .style(s(THEME.dim))
            .wrap(Wrap { trim: false }),
            list,
        );
    }
    for (y, (row, &i)) in visible
        .iter()
        .enumerate()
        .skip(start)
        .take(list.height as usize)
        .enumerate()
    {
        let source = &app.sources[i];
        let mark = if explore {
            ""
        } else if app.source_browser.selected.contains(&source.id) {
            "[x] "
        } else {
            "[ ] "
        };
        let text = format!(
            "{mark}{} {}  [{}]  {}",
            if source.enabled { "●" } else { "○" },
            source.name,
            source.group,
            if source.explore {
                "探索开"
            } else {
                "探索关"
            }
        );
        f.render_widget(
            Paragraph::new(row_line(
                row == selected,
                app.focus == Focus::Main,
                vec![Span::styled(
                    truncate(&text, list.width.saturating_sub(2) as usize),
                    s(THEME.fg),
                )],
            )),
            Rect::new(list.x, list.y + y as u16, list.width, 1),
        );
    }
    let hint = if explore {
        "Enter 浏览分类  / 筛选  ? 帮助\nPgUp/PgDn 翻页  g/G 首尾\n分类页 Backspace 返回书源列表"
    } else {
        "空格 多选  a 全选  x 删除  / 筛选\nEnter 启停  e 探索  f 状态  ? 帮助\ni 导入  o 导出  v JSON  PgUp/PgDn 翻页"
    };
    f.render_widget(Paragraph::new(hint).style(s(THEME.dim)), regions[2]);
}

pub fn draw_confirmation(f: &mut Frame, app: &App, area: Rect) -> bool {
    let Some(keys) = &app.source_browser.delete else {
        return false;
    };
    let rect = center_rect(area, 76, 13);
    f.render_widget(Clear, rect);
    let block = panel(
        vec![Span::styled(
            format!("确认删除 {} 个书源", keys.len()),
            sb(THEME.hi),
        )],
        None,
        true,
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut names = keys
        .iter()
        .take(4)
        .map(|id| {
            app.sources
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.name.as_str())
                .unwrap_or(id)
        })
        .collect::<Vec<_>>()
        .join("\n");
    if keys.len() > 4 {
        names.push_str(&format!("\n…共 {} 个", keys.len()));
    }
    let sections = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let text = format!("{names}\n\n删除后须重新导入才能恢复。保留书架与已缓存正文。\n需要此书源的后续联网读取将不可用。");
    f.render_widget(
        Paragraph::new(text)
            .style(s(THEME.fg))
            .wrap(Wrap { trim: false }),
        sections[0],
    );
    f.render_widget(
        Paragraph::new("y 确认删除 · n / Esc 取消").style(sb(THEME.hi)),
        sections[1],
    );
    true
}

pub fn draw_help(f: &mut Frame, area: Rect) {
    let rect = center_rect(area, 68, 23);
    f.render_widget(Clear, rect);
    let block = panel(
        vec![Span::styled("书源列表快捷键", sb(THEME.hi))],
        Some(Span::styled("Esc / ? 关闭", s(THEME.dim))),
        true,
    );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let text = "/           筛选名称、分组、URL；Enter 结束输入\nj/k ↑/↓     移动；PgUp/PgDn 每次 20 项\ng/G         首尾；Tab 切换侧栏\n\n书源管理\n空格        勾选 / 取消当前项\na / A       全选筛选结果 / 清空选择\nEnter       统一启停（有禁用项则全部启用）\n+ / -       全部启用 / 全部禁用\ne / E       统一切换探索 / 全部关闭探索\nx / Delete  删除；弹框 y 确认，n / Esc 取消\nf           全部 → 已启用 → 已禁用 → 可探索\ni / o       导入 JSON / 导出全部到新文件\nv           查看当前书源完整 JSON\nEsc         依次清空多选、筛选、返回侧栏\n未多选时操作当前项；修改筛选会清空多选。\n\n探索书源\nEnter       打开分类；Backspace 返回书源列表";
    f.render_widget(
        Paragraph::new(text)
            .style(s(THEME.fg))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::Snapshot, jobs::Event};
    use ratatui::{backend::TestBackend, Terminal};
    use reader_core::model::book_source::{BookSource, ExploreKind};

    fn fixture(count: usize) -> App {
        let mut app = App::new_live();
        app.apply_live_event(Event::Snapshot(Snapshot {
            books: vec![],
            sources: (0..count)
                .map(|i| BookSource {
                    book_source_name: format!("书源{i:04}"),
                    book_source_url: format!("https://fixture.invalid/{i}"),
                    book_source_group: Some(if i % 2 == 0 { "甲组" } else { "乙组" }.into()),
                    enabled: Some(true),
                    enabled_explore: Some(true),
                    ..Default::default()
                })
                .collect(),
            categories: (0..count)
                .map(|_| {
                    vec![ExploreKind {
                        title: "推荐".into(),
                        url: Some("/list".into()),
                        ..Default::default()
                    }]
                })
                .collect(),
        }));
        app.goto_id("set:sources");
        app
    }
    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn thousands_of_sources_keep_navigation_bounded_and_exploration_reachable() {
        let mut app = fixture(3000);
        assert_eq!(app.nav.len(), 10);
        app.goto_id("discover:sources");
        key(&mut app, KeyCode::End);
        key(&mut app, KeyCode::Enter);
        assert!(matches!(app.route(), Route::Discover { source_idx: 2999 }));
        assert_eq!(app.nav.len(), 11);
        key(&mut app, KeyCode::Backspace);
        assert!(matches!(app.route(), Route::ExploreSources));
        assert_eq!(app.source_browser.cursor, 2999);
        for (w, h) in [(120, 40), (80, 24), (40, 12), (2, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            if w == 80 {
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
                    .replace(' ', "");
                assert!(text.contains("书源2999"));
                assert!(text.contains("书源管理"));
                assert!(text.contains("阅读偏好"));
            }
        }
    }

    #[test]
    fn filter_multiselect_and_delete_use_stable_ids_and_require_confirmation() {
        let mut app = fixture(20);
        key(&mut app, KeyCode::Char('/'));
        for c in "甲组".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.source_indices().len(), 10);
        key(&mut app, KeyCode::Char('a'));
        key(&mut app, KeyCode::Char('-'));
        let Some(Command::SetSources {
            keys,
            enabled,
            explore,
        }) = app.commands.last()
        else {
            panic!("missing command")
        };
        assert!(!enabled && !explore);
        assert_eq!(keys.len(), 10);
        assert!(keys
            .iter()
            .all(|id| id.rsplit('/').next().unwrap().parse::<usize>().unwrap() % 2 == 0));
        app.commands.clear();
        key(&mut app, KeyCode::Char('x'));
        assert_eq!(app.source_browser.delete.as_ref().unwrap().len(), 10);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Char('i'));
        assert!(app.commands.is_empty() && app.live.as_ref().unwrap().prompt.is_none());
        key(&mut app, KeyCode::Esc);
        assert!(app.source_browser.delete.is_none());
        key(&mut app, KeyCode::Delete);
        key(&mut app, KeyCode::Char('y'));
        assert!(
            matches!(app.commands.last(), Some(Command::DeleteSources(keys)) if keys.len() == 10)
        );
    }

    #[test]
    fn snapshots_preserve_focus_by_url_and_remove_stale_selections() {
        let mut app = fixture(4);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char(' '));
        let live = app.live.as_ref().unwrap();
        let snapshot = Snapshot {
            books: vec![],
            sources: live.sources[1..].to_vec(),
            categories: live.categories[1..].to_vec(),
        };
        app.apply_live_event(Event::Snapshot(snapshot));
        assert_eq!(app.source_browser.cursor, 0);
        assert_eq!(app.source_browser.selected.len(), 1);
        app.apply_live_event(Event::Snapshot(Snapshot {
            books: vec![],
            sources: vec![],
            categories: vec![],
        }));
        assert!(app.source_browser.selected.is_empty());
        key(&mut app, KeyCode::Delete);
        assert!(app.source_browser.delete.is_none());
        key(&mut app, KeyCode::Char('/'));
        for c in "ioeq/".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        assert_eq!(app.source_browser.filter, "ioeq/");
        assert!(
            !app.should_quit
                && !app.search_input_mode
                && app.live.as_ref().unwrap().prompt.is_none()
        );
    }
}
