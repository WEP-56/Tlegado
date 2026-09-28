//! TUI adaptation of domain data; all effects are queued as jobs.
use crate::{
    app::{App, Focus, Route, ToastTone},
    backend::Snapshot,
    data,
    jobs::{Command, Event},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use reader_core::model::{
    book::Book,
    book_chapter::BookChapter,
    book_source::{BookSource, ExploreKind},
};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy)]
pub enum InputKind {
    Import,
    Export,
}
pub struct Input {
    pub kind: InputKind,
    pub text: String,
}
pub struct Live {
    pub ready: bool,
    pub status: String,
    pub prompt: Option<Input>,
    pub search: Vec<data::Book>,
    pub discover: Vec<data::Book>,
    pub sources: Vec<BookSource>,
    pub categories: Vec<Vec<ExploreKind>>,
    pub domain: HashMap<String, Book>,
    pub current: Option<Book>,
    pub chapters: Vec<BookChapter>,
    sequence: u64,
    query_id: u64,
    read_id: u64,
    querying: bool,
    opening: bool,
    query_discover: bool,
    failures: usize,
    last_error: String,
    discover_key: Option<(String, usize)>,
    last_route: String,
    last_saved: Option<(String, u32, usize)>,
}
impl Default for Live {
    fn default() -> Self {
        Self {
            ready: false,
            status: "正在初始化本地存储…".into(),
            prompt: None,
            search: vec![],
            discover: vec![],
            sources: vec![],
            categories: vec![],
            domain: HashMap::new(),
            current: None,
            chapters: vec![],
            sequence: 0,
            query_id: 0,
            read_id: 0,
            querying: false,
            opening: false,
            query_discover: false,
            failures: 0,
            last_error: String::new(),
            discover_key: None,
            last_route: "home".into(),
            last_saved: None,
        }
    }
}
impl Live {
    pub fn busy(&self) -> bool {
        !self.ready || self.querying || self.opening
    }

    fn next_id(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
}

pub fn book_id(book: &Book) -> String {
    serde_json::to_string(&(&book.origin, &book.book_url)).expect("string pair")
}
pub fn book_view(book: &Book) -> data::Book {
    let total = book.total_chapter_num.unwrap_or(0).max(0) as u32;
    data::Book {
        id: book_id(book),
        title: book.name.clone(),
        author: book.author.clone(),
        kind: data::Kind::Network,
        origin: book
            .origin_name
            .clone()
            .unwrap_or_else(|| book.origin.clone()),
        group: "未分组".into(),
        category: book.kind.clone().unwrap_or_default(),
        status: "—".into(),
        words: book.word_count.clone().unwrap_or_else(|| "—".into()),
        total,
        read: (book.dur_chapter_index.unwrap_or(0).max(0) as u32).min(total.saturating_sub(1)),
        latest: book.latest_chapter_title.clone().unwrap_or_default(),
        last_read: book
            .dur_chapter_time
            .filter(|t| *t > 0)
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "未读".into()),
        new_count: 0,
        intro: book.intro.clone().unwrap_or_default(),
    }
}

impl App {
    pub fn new_live() -> Self {
        let mut app = Self::new();
        app.books.clear();
        app.sources.clear();
        app.demo.history.clear();
        app.demo.rules.clear();
        app.demo.checks.clear();
        app.live = Some(Live::default());
        app.rebuild_nav();
        app
    }

    pub fn live_key(&mut self, key: KeyEvent) -> bool {
        if self.live.is_none() {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.save_live_progress();
            self.should_quit = true;
            return true;
        }
        if self.source_browser.editing || self.source_browser.delete.is_some() {
            return false;
        }
        if self.live.as_ref().unwrap().prompt.is_some() {
            match key.code {
                KeyCode::Esc => self.live.as_mut().unwrap().prompt = None,
                KeyCode::Enter => {
                    let input = self.live.as_mut().unwrap().prompt.take().unwrap();
                    let path = input.text.trim().trim_matches('"');
                    if path.is_empty() {
                        self.toast("请输入文件路径", ToastTone::Err);
                    } else {
                        self.commands.push(match input.kind {
                            InputKind::Import => Command::Import(PathBuf::from(path)),
                            InputKind::Export => Command::Export(PathBuf::from(path)),
                        });
                        self.toast("正在处理文件…", ToastTone::Info);
                    }
                }
                KeyCode::Backspace => {
                    self.live
                        .as_mut()
                        .unwrap()
                        .prompt
                        .as_mut()
                        .unwrap()
                        .text
                        .pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => self
                    .live
                    .as_mut()
                    .unwrap()
                    .prompt
                    .as_mut()
                    .unwrap()
                    .text
                    .push(c),
                _ => {}
            }
            return true;
        }
        if self.help {
            return false;
        }
        if !self.live.as_ref().unwrap().ready {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return true;
        }
        if self.reader.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                self.save_live_progress();
                self.cancel_live_reads();
                self.live.as_mut().unwrap().current = None;
            }
            return false;
        }
        if self.search_input_mode && key.code == KeyCode::Enter {
            self.search_input_mode = false;
            if self.search_query.trim().is_empty() {
                self.toast("请输入关键词", ToastTone::Err);
            } else {
                self.start_live_query(false);
            }
            return true;
        }
        if self.search_input_mode {
            return false;
        }
        if key.code == KeyCode::Esc
            && (self.live.as_ref().unwrap().opening || self.live.as_ref().unwrap().querying)
        {
            self.cancel_live_reads();
            self.toast("已取消读取；正在发送的请求会正常结束", ToastTone::Info);
            return true;
        }
        if self.focus != Focus::Main {
            return false;
        }
        match (self.route().clone(), key.code) {
            (Route::Sources, KeyCode::Char('i' | 'o')) if !self.demo.json => {
                self.live.as_mut().unwrap().prompt = Some(Input {
                    kind: if key.code == KeyCode::Char('i') {
                        InputKind::Import
                    } else {
                        InputKind::Export
                    },
                    text: String::new(),
                });
                true
            }
            (Route::Sources, KeyCode::Enter | KeyCode::Char(' ' | 'e')) if self.demo.json => true,
            (Route::Sources, KeyCode::Char('t' | 'T' | 'c')) => {
                self.toast(
                    "书源批量校验将在下一阶段接入；可先通过搜索检查规则",
                    ToastTone::Info,
                );
                true
            }
            (Route::Home, KeyCode::Char('o')) => {
                self.toast("本地书籍导入将在下一阶段接入", ToastTone::Info);
                true
            }
            (Route::Shelf { .. }, KeyCode::Char('r')) => {
                self.toast("批量更新检查将在下一阶段接入", ToastTone::Info);
                true
            }
            (Route::Shelf { filter }, KeyCode::Char('x')) => {
                if let Some(view) = self.shelf_list(filter).get(self.shelf_sel) {
                    if let Some(book) = self.live.as_ref().unwrap().domain.get(&view.id) {
                        self.commands.push(Command::Remove(book.clone()));
                    }
                }
                true
            }
            (Route::History, KeyCode::Char('x')) => {
                self.toast(
                    "历史由持久化阅读进度生成，暂不支持单独删除",
                    ToastTone::Info,
                );
                true
            }
            _ => false,
        }
    }

    pub fn cancel_live_reads(&mut self) {
        if let Some(live) = &mut self.live {
            live.query_id = 0;
            live.read_id = 0;
            live.querying = false;
            live.opening = false;
            live.status = "已取消".into();
            self.commands.push(Command::Cancel);
        }
    }

    pub fn open_live(&mut self, view: &data::Book) {
        let live = self.live.as_mut().unwrap();
        if let Some(book) = live.domain.get(&view.id).cloned() {
            let id = live.next_id();
            live.read_id = id;
            live.opening = true;
            live.status = format!("正在加载《{}》目录和正文… · Esc 取消", book.name);
            self.commands.push(Command::Open { id, book });
            self.toast = None;
        }
    }

    pub fn start_live_query(&mut self, discover: bool) {
        let route = self.route().clone();
        let live = self.live.as_mut().unwrap();
        let (sources, explore) = if discover {
            let Route::Discover { source_idx } = route else {
                return;
            };
            let Some(source) = live.sources.get(source_idx) else {
                return;
            };
            let url = live
                .categories
                .get(source_idx)
                .and_then(|c| c.get(self.discover_cat))
                .and_then(|c| c.url.clone());
            if url.is_none() {
                return;
            }
            live.discover_key = Some((source.book_source_url.clone(), self.discover_cat));
            (vec![source.clone()], url)
        } else {
            (
                live.sources
                    .iter()
                    .filter(|s| {
                        s.is_enabled()
                            && s.search_url
                                .as_deref()
                                .is_some_and(|u| !u.trim().is_empty())
                    })
                    .cloned()
                    .collect(),
                None,
            )
        };
        let id = live.next_id();
        live.query_id = id;
        live.query_discover = discover;
        live.querying = !sources.is_empty();
        live.failures = 0;
        live.last_error.clear();
        if discover {
            live.discover.clear();
            self.discover_sel = 0;
        } else {
            live.search.clear();
            self.search_sel = 0;
        }
        let keep: std::collections::HashSet<String> = self
            .books
            .iter()
            .chain(live.search.iter())
            .chain(live.discover.iter())
            .map(|b| b.id.clone())
            .chain(live.current.iter().map(book_id))
            .collect();
        live.domain.retain(|id, _| keep.contains(id));
        live.status = if sources.is_empty() {
            "没有可搜索的已启用书源，请到书源管理按 i 导入".into()
        } else {
            format!("正在查询 {} 个书源… · Esc 取消", sources.len())
        };
        if !sources.is_empty() {
            self.commands.push(Command::Query {
                id,
                sources,
                keyword: self.search_query.trim().to_string(),
                explore,
            });
        }
        self.toast = None;
    }

    pub fn save_live_progress(&mut self) {
        let (Some(live), Some(reader)) = (&mut self.live, &self.reader) else {
            return;
        };
        let Some(book) = live.current.clone() else {
            return;
        };
        let position = reader.position();
        let progress = (book_id(&book), reader.chapter, position);
        if live.last_saved.as_ref() == Some(&progress) {
            return;
        }
        live.last_saved = Some(progress);
        self.commands.push(Command::Progress {
            book,
            index: reader.chapter as usize,
            position,
            title: reader.title(reader.chapter),
        });
    }

    pub fn sync_live(&mut self) {
        let Some(live) = &self.live else { return };
        if !live.ready {
            return;
        }
        let route_id = self.nav[self.nav_idx].id.clone();
        if live.last_route != route_id {
            self.cancel_live_reads();
            let live = self.live.as_mut().unwrap();
            live.last_route = route_id;
            live.status.clear();
            live.discover_key = None;
        }
        if self.reader.is_none() {
            if let Route::Discover { source_idx } = self.route() {
                let live = self.live.as_ref().unwrap();
                if let Some(source) = self.sources.get(*source_idx) {
                    if live.discover_key != Some((source.id.clone(), self.discover_cat)) {
                        self.start_live_query(true);
                    }
                }
            }
        }
        if let Some((index, end)) = self.reader.as_mut().and_then(|r| r.request.take()) {
            self.save_live_progress();
            let live = self.live.as_mut().unwrap();
            if let (Some(book), Some(chapter)) = (
                live.current.clone(),
                live.chapters.get(index as usize).cloned(),
            ) {
                let id = live.next_id();
                live.read_id = id;
                live.opening = true;
                live.status = format!("正在加载 {}…", chapter.title);
                self.commands.push(Command::Chapter {
                    id,
                    book,
                    chapter,
                    index: index as usize,
                    end,
                });
            }
        }
        if self.tick.is_multiple_of(20) {
            self.save_live_progress();
        }
    }

    fn load_shelf(&mut self, mut books: Vec<Book>) {
        books.sort_by_key(|b| std::cmp::Reverse(b.dur_chapter_time.unwrap_or(0)));
        self.demo.history = books
            .iter()
            .filter(|b| b.dur_chapter_time.unwrap_or(0) > 0)
            .map(|b| {
                let book = book_view(b);
                crate::demo::History {
                    time: book.last_read.clone(),
                    book,
                }
            })
            .collect();
        self.books = books.iter().map(book_view).collect();
        let live = self.live.as_mut().unwrap();
        for book in books {
            live.domain.insert(book_id(&book), book);
        }
        self.shelf_sel = self.shelf_sel.min(self.books.len().saturating_sub(1));
        self.demo.history_sel = self
            .demo
            .history_sel
            .min(self.demo.history.len().saturating_sub(1));
        self.rebuild_nav();
    }

    pub fn apply_live_event(&mut self, event: Event) {
        if self.live.is_none() {
            return;
        }
        match event {
            Event::Snapshot(Snapshot {
                books,
                sources,
                categories,
            }) => {
                let focused_source = self
                    .source_indices()
                    .get(self.source_browser.cursor)
                    .map(|&i| self.sources[i].id.clone());
                self.sources = sources
                    .iter()
                    .enumerate()
                    .map(|(i, s)| data::Source {
                        id: s.book_source_url.clone(),
                        name: s.book_source_name.clone(),
                        group: s.book_source_group.clone().unwrap_or_default(),
                        url: s.book_source_url.clone(),
                        enabled: s.is_enabled(),
                        explore: s.enabled_explore != Some(false),
                        respond_ms: s.respond_time.unwrap_or(0).clamp(0, i32::MAX as i64) as i32,
                        categories: categories[i].iter().map(|c| c.title.clone()).collect(),
                    })
                    .collect();
                self.demo.checks = vec![crate::demo::Check::Idle; sources.len()];
                self.demo.source_sel = self.demo.source_sel.min(sources.len().saturating_sub(1));
                let live = self.live.as_mut().unwrap();
                live.ready = true;
                live.sources = sources;
                live.categories = categories;
                self.source_browser
                    .selected
                    .retain(|id| self.sources.iter().any(|s| &s.id == id));
                let visible = self.source_indices();
                self.source_browser.cursor = focused_source
                    .and_then(|id| visible.iter().position(|&i| self.sources[i].id == id))
                    .unwrap_or(
                        self.source_browser
                            .cursor
                            .min(visible.len().saturating_sub(1)),
                    );
                let live = self.live.as_mut().unwrap();
                live.status = if self.sources.is_empty() {
                    "书源管理按 i 导入 JSON；/ 搜索书籍".into()
                } else {
                    "书源已就绪 · / 搜索书籍".into()
                };
                self.load_shelf(books);
            }
            Event::Shelf(books) => self.load_shelf(books),
            Event::QueryPart { id, source, result } => {
                let live = self.live.as_mut().unwrap();
                if id != live.query_id {
                    return;
                }
                match result {
                    Ok(books) => {
                        let output = if live.query_discover {
                            &mut live.discover
                        } else {
                            &mut live.search
                        };
                        for book in books {
                            let view = book_view(&book);
                            if output.len() < 2000 && !output.iter().any(|b| b.id == view.id) {
                                output.push(view);
                                live.domain.insert(book_id(&book), book);
                            }
                        }
                        live.status = format!(
                            "查询中 · {} 条结果 · {} 个书源失败",
                            output.len(),
                            live.failures
                        );
                    }
                    Err(error) => {
                        live.failures += 1;
                        live.last_error = format!("{source}：{error}");
                    }
                }
            }
            Event::QueryDone(id) => {
                let live = self.live.as_mut().unwrap();
                if id != live.query_id {
                    return;
                }
                live.querying = false;
                let count = if live.query_discover {
                    live.discover.len()
                } else {
                    live.search.len()
                };
                live.status = format!(
                    "查询完成 · {count} 条结果（第一页，上限 2000） · {} 个书源失败",
                    live.failures
                );
                let error = live.last_error.clone();
                if !error.is_empty() {
                    self.toast(error, ToastTone::Err);
                }
            }
            Event::Opened { id, result } => {
                if id != self.live.as_ref().unwrap().read_id {
                    return;
                }
                self.live.as_mut().unwrap().opening = false;
                match result {
                    Ok(reading) => {
                        let mut view = book_view(&reading.book);
                        view.read = reading.index as u32;
                        let mut reader = crate::reader::Reader::new(view, self.sidebar_hidden);
                        reader.options = self.demo.prefs.clone();
                        reader.set_real(
                            reading.chapters.iter().map(|c| c.title.clone()).collect(),
                            reading.text,
                            reading.book.dur_chapter_pos.unwrap_or(0).max(0) as usize,
                        );
                        let live = self.live.as_mut().unwrap();
                        live.current = Some(reading.book);
                        live.chapters = reading.chapters;
                        live.last_saved = None;
                        live.status.clear();
                        self.reader = Some(reader);
                        self.sidebar_hidden = false;
                        self.focus = Focus::Main;
                        self.toast = None;
                    }
                    Err(error) => {
                        self.live.as_mut().unwrap().status = "加载失败，可按 Enter 重试".into();
                        self.toast(error, ToastTone::Err);
                    }
                }
            }
            Event::Chapter {
                id,
                index,
                end,
                result,
            } => {
                if id != self.live.as_ref().unwrap().read_id {
                    return;
                }
                self.live.as_mut().unwrap().opening = false;
                match result {
                    Ok(text) => {
                        if let Some(reader) = &mut self.reader {
                            reader.set_chapter(index as u32, text, end);
                        }
                        self.live.as_mut().unwrap().status.clear();
                    }
                    Err(error) => {
                        self.live.as_mut().unwrap().status =
                            "章节加载失败，正文保留；可重新选择章节重试".into();
                        self.toast(error, ToastTone::Err);
                    }
                }
            }
            Event::Notice(message) => self.toast(message, ToastTone::Ok),
            Event::Error(error) => {
                self.live.as_mut().unwrap().last_saved = None;
                self.toast(error, ToastTone::Err);
            }
            Event::Fatal(error) => {
                self.live.as_mut().unwrap().ready = false;
                self.live.as_mut().unwrap().status = format!("{error} · q 退出");
                self.toast(error, ToastTone::Err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn book(name: &str) -> Book {
        Book {
            name: name.into(),
            author: "测试作者".into(),
            book_url: format!("http://fixture.invalid/{name}"),
            origin: "http://fixture.invalid".into(),
            total_chapter_num: Some(2),
            ..Default::default()
        }
    }
    fn app() -> App {
        let mut app = App::new_live();
        app.apply_live_event(Event::Snapshot(Snapshot {
            books: vec![],
            categories: vec![vec![]],
            sources: vec![BookSource {
                book_source_name: "测试源".into(),
                book_source_url: "http://fixture.invalid".into(),
                search_url: Some("/search".into()),
                ..Default::default()
            }],
        }));
        app.goto_id_for_test("discover:search");
        app.sync_live();
        app.search_query = "真实".into();
        app
    }

    #[test]
    fn old_search_results_and_completion_cannot_replace_new_query() {
        let mut app = app();
        app.start_live_query(false);
        let old = app.live.as_ref().unwrap().query_id;
        app.start_live_query(false);
        let current = app.live.as_ref().unwrap().query_id;
        app.apply_live_event(Event::QueryPart {
            id: old,
            source: "旧".into(),
            result: Ok(vec![book("过期")]),
        });
        app.apply_live_event(Event::QueryDone(old));
        assert!(app.search_results().is_empty());
        assert!(app.live.as_ref().unwrap().querying);
        app.apply_live_event(Event::QueryPart {
            id: current,
            source: "正常".into(),
            result: Ok(vec![book("当前")]),
        });
        app.apply_live_event(Event::QueryPart {
            id: current,
            source: "失败源".into(),
            result: Err("请求超时".into()),
        });
        app.apply_live_event(Event::QueryDone(current));
        assert_eq!(app.search_results()[0].title, "当前");
        assert_eq!(app.live.as_ref().unwrap().failures, 1);
        assert!(!app.live.as_ref().unwrap().querying);
    }

    #[test]
    fn home_discovery_uses_source_urls_instead_of_demo_ids() {
        let mut app = app();
        app.sources[0].categories.push("推荐".into());
        app.sources[0].enabled = true;
        app.sources[0].explore = true;
        app.rebuild_nav();
        app.goto_id_for_test("home");
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(app.route(), Route::ExploreSources));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.route(), Route::Discover { source_idx: 0 }));
    }

    #[test]
    fn cancelled_open_never_enters_reader() {
        let mut app = app();
        let book = book("取消的书");
        let view = book_view(&book);
        app.live
            .as_mut()
            .unwrap()
            .domain
            .insert(view.id.clone(), book.clone());
        app.open_live(&view);
        let id = app.live.as_ref().unwrap().read_id;
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(crate::backend::Reading {
                book,
                chapters: vec![],
                index: 0,
                text: "不应显示".into(),
            })),
        });
        assert!(app.reader.is_none());
    }

    #[test]
    fn input_modal_captures_shortcuts_and_preserves_unicode_paths() {
        let mut app = app();
        app.goto_id_for_test("set:sources");
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        for c in "C:/q 书源.json".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert!(!app.should_quit);
        assert!(!app.search_input_mode);
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(app.commands.last(), Some(Command::Import(p)) if p == &PathBuf::from("C:/q 书源.json"))
        );
    }

    #[test]
    fn source_json_preview_never_applies_demo_toggles() {
        let mut app = app();
        app.goto_id_for_test("set:sources");
        app.commands.clear();
        let enabled = app.sources[0].enabled;
        app.demo.json = true;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(app.sources[0].enabled, enabled);
        assert!(app.commands.is_empty());
        app.demo.json = false;
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(app.source_browser.selected.len(), 1);
        assert!(app.commands.is_empty());
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.commands.last(),
            Some(Command::SetSources { explore: false, .. })
        ));
        assert_eq!(
            app.sources[0].enabled, enabled,
            "wait for persisted snapshot"
        );
    }

    #[test]
    fn empty_live_pages_render_without_demo_sources_or_books() {
        let mut app = App::new_live();
        app.apply_live_event(Event::Snapshot(Snapshot {
            books: vec![],
            sources: vec![],
            categories: vec![],
        }));
        assert!(app.books.is_empty() && app.sources.is_empty() && app.demo.history.is_empty());
        for route in [
            "home",
            "shelf:all",
            "discover:search",
            "set:sources",
            "set:history",
        ] {
            app.goto_id_for_test(route);
            for (width, height) in [(120, 40), (80, 24), (40, 12), (2, 2)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(!text.contains("诡秘之主"));
                assert!(!text.contains("离线演示"));
                assert!(!text.contains("38.2 MB"));
            }
        }
    }

    #[test]
    fn failed_chapter_keeps_loaded_text_and_reader_position() {
        let mut app = app();
        let book = book("真实小说");
        let live = app.live.as_mut().unwrap();
        live.read_id = 7;
        app.apply_live_event(Event::Opened {
            id: 7,
            result: Ok(Box::new(crate::backend::Reading {
                book,
                chapters: vec![
                    BookChapter {
                        title: "第一章".into(),
                        ..Default::default()
                    },
                    BookChapter {
                        title: "第二章".into(),
                        ..Default::default()
                    },
                ],
                index: 0,
                text: "原来已加载的正文".into(),
            })),
        });
        app.on_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        app.sync_live();
        let id = app.live.as_ref().unwrap().read_id;
        app.apply_live_event(Event::Chapter {
            id,
            index: 1,
            end: false,
            result: Err("连接超时".into()),
        });
        assert_eq!(app.reader.as_ref().unwrap().chapter, 0);
        assert_eq!(app.reader.as_ref().unwrap().title(0), "第一章");
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        // TestBackend includes placeholder cells after double-width characters.
        assert!(text.replace(' ', "").contains("原来已加载的正文"), "{text}");
    }
}
