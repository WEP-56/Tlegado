//! TUI adaptation of domain data; all effects are queued as jobs.
use crate::{
    app::{App, Focus, Route, ToastTone},
    backend::Snapshot,
    data,
    jobs::{Command, Event},
    query::{self, Query},
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
    Rules,
    Layout,
    Import,
    LocalBook,
    Export,
}
pub struct Input {
    pub kind: InputKind,
    pub text: String,
}
pub struct SourcePicker {
    pub target: Book,
    pub candidates: Vec<Book>,
    pub selected: usize,
    pub switching: bool,
}
pub struct Live {
    pub library: crate::library::Library,
    pub modal: Option<crate::library::Modal>,
    pub rule_sample: String,
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
    pub picker: Option<SourcePicker>,
    queries: [Query; 3], // search, discovery, source picker
    query_target: usize,
    pending_switch: Option<Book>,
    switch_committing: bool,
    sequence: u64,
    query_id: u64,
    read_id: u64,
    querying: bool,
    opening: bool,
    failures: usize,
    last_error: String,
    discover_key: Option<(String, usize)>,
    category_request: Option<(u64, String)>,
    category_errors: HashMap<String, String>,
    last_route: String,
    last_saved: Option<(String, u32, usize)>,
}
impl Default for Live {
    fn default() -> Self {
        Self {
            library: Default::default(),
            modal: None,
            rule_sample: "夜色渐深...（演示站点水印）故事仍在继续。求月票！".into(),
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
            picker: None,
            queries: std::array::from_fn(|_| Query::default()),
            query_target: 0,
            pending_switch: None,
            switch_committing: false,
            sequence: 0,
            query_id: 0,
            read_id: 0,
            querying: false,
            opening: false,
            failures: 0,
            last_error: String::new(),
            discover_key: None,
            category_request: None,
            category_errors: HashMap::new(),
            last_route: "home".into(),
            last_saved: None,
        }
    }
}
impl Live {
    pub fn busy(&self) -> bool {
        !self.ready || self.querying || self.opening || self.category_request.is_some()
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
        kind: if crate::backend::is_local(book) {
            data::Kind::Local
        } else {
            data::Kind::Network
        },
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
        if self.library_key(key) {
            return true;
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
                            InputKind::Rules => Command::ImportRules(path.to_owned()),
                            InputKind::Layout => Command::ImportLayout(path.to_owned()),
                            InputKind::Import => Command::Import(PathBuf::from(path)),
                            InputKind::LocalBook => Command::ImportLocal(PathBuf::from(path)),
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
        if self.live.as_ref().unwrap().picker.is_some() {
            self.source_picker_key(key);
            return true;
        }
        if !self.live.as_ref().unwrap().ready {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return true;
        }
        if self.reader.is_some() {
            if key.code == KeyCode::Char('a') {
                let live = self.live.as_ref().unwrap();
                if let (Some(book), Some(reader)) = (&live.current, &self.reader) {
                    if live.switch_committing || live.opening {
                        return true;
                    }
                    if self.books.iter().any(|b| b.id == book_id(book)) {
                        self.toast("已在书架中", ToastTone::Info);
                    } else {
                        let mut book = book.clone();
                        book.dur_chapter_index = Some(reader.chapter as i32);
                        book.dur_chapter_pos =
                            Some(reader.position().min(i32::MAX as usize) as i32);
                        book.dur_chapter_title = Some(reader.title(reader.chapter));
                        book.dur_chapter_time = Some(chrono::Utc::now().timestamp_millis());
                        self.commands.push(Command::Add(book));
                    }
                }
                return true;
            }
            if key.code == KeyCode::Char('s') {
                self.open_source_picker();
                return true;
            }
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
            (Route::Discover { source_idx }, KeyCode::Char('R')) => {
                self.load_discovery_categories(source_idx);
                true
            }
            (Route::Discover { source_idx }, KeyCode::Char('r'))
                if self
                    .live
                    .as_ref()
                    .unwrap()
                    .categories
                    .get(source_idx)
                    .is_none_or(|c| c.is_empty()) =>
            {
                self.load_discovery_categories(source_idx);
                true
            }
            (Route::Search | Route::Discover { .. }, KeyCode::Char('n' | 'r')) => {
                let target = usize::from(matches!(self.route(), Route::Discover { .. }));
                self.continue_live_query(target, key.code == KeyCode::Char('r'));
                true
            }
            (Route::Search | Route::Discover { .. }, KeyCode::Char('s')) => {
                self.open_source_picker();
                true
            }
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
            (Route::Home | Route::Shelf { .. }, KeyCode::Char('o')) => {
                self.live.as_mut().unwrap().prompt = Some(Input {
                    kind: InputKind::LocalBook,
                    text: String::new(),
                });
                true
            }
            (Route::Shelf { .. }, KeyCode::Char('r')) => {
                self.toast("批量更新检查将在下一阶段接入", ToastTone::Info);
                true
            }
            (Route::Shelf { filter }, KeyCode::Char('x')) => {
                if let Some(view) = self.shelf_list(filter).get(self.shelf_sel) {
                    if let Some(book) = self.live.as_ref().unwrap().domain.get(&view.id) {
                        let book = book.clone();
                        let label = if crate::backend::is_local(&book) {
                            format!("删除《{}》及导入副本、缓存和进度？原文件保留。", book.name)
                        } else {
                            format!("将《{}》移出书架并清除缓存和进度？", book.name)
                        };
                        self.live.as_mut().unwrap().modal = Some(crate::library::Modal::Confirm {
                            change: crate::library::Change::RemoveBook(Box::new(book)),
                            label,
                        });
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
            if live.switch_committing {
                return;
            }
            live.queries[live.query_target].cancel();
            live.pending_switch = None;
            live.query_id = 0;
            live.read_id = 0;
            live.querying = false;
            live.opening = false;
            if let Some((_, source)) = live.category_request.take() {
                live.category_errors
                    .insert(source, "发现分类加载已取消 · r 重试".into());
            }
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

    fn load_discovery_categories(&mut self, index: usize) {
        self.cancel_live_reads();
        let live = self.live.as_mut().unwrap();
        let Some(source) = live.sources.get(index).cloned() else {
            return;
        };
        let id = live.next_id();
        live.category_errors.remove(&source.book_source_url);
        live.category_request = Some((id, source.book_source_url.clone()));
        live.queries[1] = Query::default();
        live.query_target = 1;
        live.categories[index].clear();
        self.sources[index].categories.clear();
        live.discover.clear();
        live.discover_key = None;
        self.discover_cat = 0;
        self.discover_sel = 0;
        live.status = "正在加载发现分类… · Esc 取消".into();
        self.commands.push(Command::ExploreKinds { id, source });
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
        live.queries[live.query_target].cancel();
        live.query_target = usize::from(discover);
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
        live.queries[live.query_target] =
            Query::new(sources, self.search_query.trim().to_string(), explore);
        let query = &mut live.queries[live.query_target];
        let requests = query.requests(false);
        if !requests.is_empty() {
            self.commands.push(Command::Query {
                id,
                requests,
                keyword: query.keyword.clone(),
                explore: query.explore.clone(),
            });
        }
        self.toast = None;
    }

    pub fn continue_live_query(&mut self, target: usize, retry_only: bool) {
        let live = self.live.as_mut().unwrap();
        if live.querying {
            return;
        }
        let requests = live.queries[target].requests(retry_only);
        if requests.is_empty() {
            live.status = live.queries[target].status();
            return;
        }
        let id = live.next_id();
        live.query_id = id;
        live.query_target = target;
        live.querying = true;
        live.status = format!("正在加载 {} 个书源的后续结果… · Esc 取消", requests.len());
        self.commands.push(Command::Query {
            id,
            requests,
            keyword: live.queries[target].keyword.clone(),
            explore: live.queries[target].explore.clone(),
        });
        self.toast = None;
    }

    fn open_source_picker(&mut self) {
        let switching = self.reader.is_some();
        let target = if switching {
            let mut book = self.live.as_ref().unwrap().current.clone();
            if let (Some(book), Some(reader)) = (&mut book, &self.reader) {
                book.dur_chapter_index = Some(reader.chapter as i32);
                book.dur_chapter_pos = Some(reader.position().min(i32::MAX as usize) as i32);
                book.dur_chapter_title = Some(reader.title(reader.chapter));
            }
            book
        } else {
            let live = self.live.as_ref().unwrap();
            let view = match self.route() {
                Route::Search => live.search.get(self.search_sel),
                Route::Discover { .. } => live.discover.get(self.discover_sel),
                _ => None,
            };
            view.and_then(|v| live.domain.get(&v.id)).cloned()
        };
        let Some(target) = target else {
            return;
        };
        if crate::backend::is_local(&target) {
            self.toast("本地书籍无需换源", ToastTone::Info);
            return;
        }
        self.save_live_progress();
        self.cancel_live_reads();
        let live = self.live.as_mut().unwrap();
        live.picker = Some(SourcePicker {
            candidates: vec![target.clone()],
            target: target.clone(),
            selected: 0,
            switching,
        });
        live.queries[2] = Query::new(
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
            target.name.clone(),
            None,
        );
        self.refresh_source_picker();
        self.continue_live_query(2, false);
    }

    fn refresh_source_picker(&mut self) {
        let live = self.live.as_mut().unwrap();
        let Some(picker) = &mut live.picker else {
            return;
        };
        let hits = picker
            .target
            .source_candidates
            .iter()
            .flatten()
            .cloned()
            .chain(
                live.queries
                    .iter()
                    .flat_map(|q| q.books.iter())
                    .filter(|b| query::same_book(b, &picker.target))
                    .flat_map(|b| b.source_candidates.iter().flatten().cloned()),
            );
        for hit in hits {
            if picker.candidates.len() >= query::RESULT_LIMIT {
                break;
            }
            let book = query::from_candidate(&hit, &live.sources);
            if query::same_book(&picker.target, &book)
                && live
                    .sources
                    .iter()
                    .any(|s| s.book_source_url == book.origin && s.is_enabled())
                && !picker
                    .candidates
                    .iter()
                    .any(|b| book_id(b) == book_id(&book))
            {
                picker.candidates.push(book);
            }
        }
    }

    fn source_picker_key(&mut self, key: KeyEvent) {
        let live = self.live.as_mut().unwrap();
        if live.switch_committing {
            return;
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            self.cancel_live_reads();
            self.live.as_mut().unwrap().picker = None;
            return;
        }
        if live.opening {
            return;
        }
        let picker = live.picker.as_mut().unwrap();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                picker.selected =
                    (picker.selected + 1).min(picker.candidates.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::PageDown => {
                picker.selected =
                    (picker.selected + 10).min(picker.candidates.len().saturating_sub(1))
            }
            KeyCode::PageUp => picker.selected = picker.selected.saturating_sub(10),
            KeyCode::Home => picker.selected = 0,
            KeyCode::End => picker.selected = picker.candidates.len().saturating_sub(1),
            KeyCode::Char('n' | 'r') => self.continue_live_query(2, key.code == KeyCode::Char('r')),
            KeyCode::Enter => {
                let Some(mut book) = picker.candidates.get(picker.selected).cloned() else {
                    return;
                };
                if picker.switching && book_id(&book) == book_id(&picker.target) {
                    self.cancel_live_reads();
                    self.live.as_mut().unwrap().picker = None;
                    return;
                }
                book.source_candidates =
                    Some(picker.candidates.iter().map(query::candidate).collect());
                let previous = picker.target.clone();
                let switching = picker.switching;
                self.cancel_live_reads();
                let live = self.live.as_mut().unwrap();
                let id = live.next_id();
                live.read_id = id;
                live.opening = true;
                live.status = "正在加载候选书源目录和正文… · Esc 取消".into();
                if switching {
                    live.pending_switch = Some(previous.clone());
                    self.commands.push(Command::Switch {
                        id,
                        previous: Box::new(previous),
                        book,
                    });
                } else {
                    self.commands.push(Command::Open { id, book });
                }
                self.toast = None;
            }
            _ => {}
        }
    }

    pub fn save_live_progress(&mut self) {
        let (Some(live), Some(reader)) = (&mut self.live, &self.reader) else {
            return;
        };
        if live.switch_committing {
            return;
        }
        let Some(book) = live.current.clone() else {
            return;
        };
        if !self.books.iter().any(|b| b.id == book_id(&book)) {
            return;
        }
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
                let index = *source_idx;
                let live = self.live.as_ref().unwrap();
                if let Some(source) = self.sources.get(index) {
                    if live.categories.get(index).is_none_or(|c| c.is_empty()) {
                        if let Some(error) = live.category_errors.get(&source.id) {
                            self.live.as_mut().unwrap().status = error.clone();
                        } else if live.category_request.is_none() {
                            self.load_discovery_categories(index);
                        }
                    } else if live.discover_key != Some((source.id.clone(), self.discover_cat)) {
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
                let prefetch = self.demo.prefs.0.get(7).copied().unwrap_or(0);
                if prefetch > 0 {
                    if let Some(book) = live.current.clone() {
                        for next in 1..=prefetch {
                            if let Some(chapter) = live.chapters.get(index as usize + next).cloned()
                            {
                                self.commands.push(Command::Prefetch {
                                    book: book.clone(),
                                    chapter,
                                });
                            }
                        }
                    }
                }
            }
        }
        if self.tick.is_multiple_of(20) {
            self.save_live_progress();
        }
    }

    fn load_shelf(&mut self, mut books: Vec<Book>) {
        let removed_local = self
            .live
            .as_ref()
            .and_then(|l| l.current.as_ref())
            .is_some_and(|current| {
                crate::backend::is_local(current)
                    && !books.iter().any(|b| book_id(b) == book_id(current))
            });
        if removed_local {
            self.cancel_live_reads();
            if let Some(reader) = self.reader.take() {
                self.sidebar_hidden = reader.previous_sidebar_hidden;
            }
            let live = self.live.as_mut().unwrap();
            live.current = None;
            live.chapters.clear();
            live.last_saved = None;
        }
        books.sort_by_key(|b| std::cmp::Reverse(b.dur_chapter_time.unwrap_or(0)));
        self.demo.history = books
            .iter()
            .filter(|b| b.dur_chapter_time.unwrap_or(0) > 0)
            .map(|b| {
                let mut book = book_view(b);
                if let Some(group) = self
                    .live
                    .as_ref()
                    .unwrap()
                    .library
                    .groups
                    .iter()
                    .find(|g| Some(g.group_id) == b.group)
                {
                    book.group = group.group_name.clone();
                }
                crate::demo::History {
                    time: book.last_read.clone(),
                    book,
                }
            })
            .collect();
        self.books = books
            .iter()
            .map(|book| {
                let mut view = book_view(book);
                if let Some(group) = self
                    .live
                    .as_ref()
                    .unwrap()
                    .library
                    .groups
                    .iter()
                    .find(|g| Some(g.group_id) == book.group)
                {
                    view.group = group.group_name.clone();
                }
                view
            })
            .collect();
        let live = self.live.as_mut().unwrap();
        if let Some(reader) = &mut self.reader {
            reader.on_shelf = live
                .current
                .as_ref()
                .is_some_and(|book| self.books.iter().any(|b| b.id == book_id(book)));
        }
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
            Event::Library(library) => {
                if let (Some(reader), Some(book)) = (
                    &mut self.reader,
                    self.live.as_ref().and_then(|l| l.current.as_ref()),
                ) {
                    reader.options = library.prefs.clone();
                    reader.layout = library.layout.clone();
                    reader.live_rules = crate::purify::for_book(&library.rules, book);
                    reader.title_rules = crate::purify::for_title(&library.rules, book);
                    reader.refresh_layout();
                }
                if self.shelf_group >= 2 {
                    let selected_id = self
                        .live
                        .as_ref()
                        .unwrap()
                        .library
                        .groups
                        .get(self.shelf_group - 2)
                        .map(|g| g.group_id);
                    self.shelf_group = selected_id
                        .and_then(|id| library.groups.iter().position(|g| g.group_id == id))
                        .map_or(0, |i| i + 2);
                }
                self.demo.prefs = library.prefs.clone();
                self.demo.rule_sel = self
                    .demo
                    .rule_sel
                    .min(library.rules.len().saturating_sub(1));
                self.live.as_mut().unwrap().library = library;
                self.rebuild_nav();
            }
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
                live.category_request = None;
                live.category_errors.clear();
                live.discover_key = None;
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
            Event::ExploreKinds { id, source, result } => {
                let live = self.live.as_mut().unwrap();
                if live.category_request.as_ref() != Some(&(id, source.clone())) {
                    return;
                }
                live.category_request = None;
                let Some(index) = live
                    .sources
                    .iter()
                    .position(|s| s.book_source_url == source)
                else {
                    return;
                };
                let result = result.and_then(|kinds| {
                    let guidance = kinds
                        .iter()
                        .filter(|k| k.url.as_deref().is_none_or(|u| u.trim().is_empty()))
                        .map(|k| k.title.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    // Legado's empty-URL entries are section labels, not links.
                    let links: Vec<_> = kinds
                        .into_iter()
                        .filter(|k| k.url.as_deref().is_some_and(|u| !u.trim().is_empty()))
                        .collect();
                    if links.is_empty() {
                        Err(if guidance.is_empty() {
                            "书源没有可打开的发现分类".into()
                        } else {
                            guidance
                        })
                    } else {
                        Ok(links)
                    }
                });
                match result {
                    Ok(kinds) => {
                        self.sources[index].categories =
                            kinds.iter().map(|k| k.title.clone()).collect();
                        live.categories[index] = kinds;
                        live.category_errors.remove(&source);
                        live.discover_key = None;
                        live.status.clear();
                    }
                    Err(error) => {
                        let error = format!("发现分类加载失败：{error} · r 重试");
                        live.category_errors.insert(source, error.clone());
                        live.status = error;
                    }
                }
                self.rebuild_nav();
            }
            Event::Shelf(books) => self.load_shelf(books),
            Event::QueryPart {
                id,
                source,
                page,
                result,
            } => {
                let live = self.live.as_mut().unwrap();
                if id != live.query_id {
                    return;
                }
                let query = &mut live.queries[live.query_target];
                if !query.accept(&source, page, result) {
                    return;
                }
                live.failures = query.failures();
                live.last_error = query.last_error().unwrap_or_default().to_owned();
                live.status = format!("查询中 · {}", query.status());
                if live.query_target < 2 {
                    let views = query
                        .books
                        .iter()
                        .map(|book| {
                            live.domain.insert(book_id(book), book.clone());
                            let mut view = book_view(book);
                            let count = book.source_candidates.as_ref().map_or(1, Vec::len);
                            if count > 1 {
                                view.origin = format!("{} · {count} 个候选", view.origin);
                            }
                            view
                        })
                        .collect();
                    if live.query_target == 1 {
                        live.discover = views;
                    } else {
                        live.search = views;
                    }
                } else {
                    self.refresh_source_picker();
                }
            }
            Event::QueryDone(id) => {
                let live = self.live.as_mut().unwrap();
                if id != live.query_id {
                    return;
                }
                live.querying = false;
                live.query_id = 0;
                let query = &mut live.queries[live.query_target];
                query.finish();
                live.failures = query.failures();
                live.last_error = query.last_error().unwrap_or_default().to_owned();
                live.status = query.status();
                let error = live.last_error.clone();
                if !error.is_empty() {
                    self.toast(error, ToastTone::Err);
                }
            }
            Event::Opened { id, result } => {
                if id == 0
                    || id != self.live.as_ref().unwrap().read_id
                    || self.live.as_ref().unwrap().switch_committing
                {
                    return;
                }
                let live = self.live.as_mut().unwrap();
                if let (Some(previous), Ok(reading)) = (
                    live.pending_switch
                        .clone()
                        .filter(|book| self.books.iter().any(|b| b.id == book_id(book))),
                    &result,
                ) {
                    live.switch_committing = true;
                    live.status = "正在保存换源结果…".into();
                    self.commands.push(Command::CommitSwitch {
                        id,
                        previous,
                        reading: reading.clone(),
                    });
                    return;
                }
                live.opening = false;
                live.pending_switch = None;
                match result {
                    Ok(reading) => self.install_reading(*reading),
                    Err(error) => {
                        self.live.as_mut().unwrap().status = "加载失败，可按 Enter 重试".into();
                        self.toast(error, ToastTone::Err);
                    }
                }
            }
            Event::Switched { id, result } => {
                let live = self.live.as_mut().unwrap();
                if id != live.read_id || !live.switch_committing {
                    return;
                }
                live.switch_committing = false;
                live.opening = false;
                live.pending_switch = None;
                match result {
                    Ok(reading) => {
                        self.install_reading(*reading);
                        self.toast(
                            "已换源：优先匹配章节标题，否则按原序号定位；从章首阅读",
                            ToastTone::Ok,
                        );
                    }
                    Err(error) => {
                        live.status = "换源保存失败，原书籍和正文保留；Enter 重试".into();
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

    fn install_reading(&mut self, reading: crate::backend::Reading) {
        let mut view = book_view(&reading.book);
        view.read = reading.index as u32;
        let previous_sidebar = self
            .reader
            .as_ref()
            .map_or(self.sidebar_hidden, |r| r.previous_sidebar_hidden);
        let mut reader = crate::reader::Reader::new(view, previous_sidebar);
        reader.on_shelf = self.books.iter().any(|b| b.id == book_id(&reading.book));
        reader.options = self.demo.prefs.clone();
        reader.layout = self.live.as_ref().unwrap().library.layout.clone();
        reader.title_rules =
            crate::purify::for_title(&self.live.as_ref().unwrap().library.rules, &reading.book);
        reader.live_rules =
            crate::purify::for_book(&self.live.as_ref().unwrap().library.rules, &reading.book);
        reader.set_real(
            reading.chapters.iter().map(|c| c.title.clone()).collect(),
            reading.text,
            reading.book.dur_chapter_pos.unwrap_or(0).max(0) as usize,
        );
        let live = self.live.as_mut().unwrap();
        live.domain
            .insert(book_id(&reading.book), reading.book.clone());
        live.current = Some(reading.book);
        live.chapters = reading.chapters;
        live.last_saved = None;
        live.read_id = 0;
        live.picker = None;
        live.status.clear();
        self.reader = Some(reader);
        self.sidebar_hidden = false;
        self.focus = Focus::Main;
        self.search_input_mode = false;
        self.toast = None;
        self.save_live_progress();
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

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn reading(book: Book) -> crate::backend::Reading {
        crate::backend::Reading {
            book,
            index: 1,
            text: "保留的正文".into(),
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
        }
    }

    fn reader_app() -> (App, Book) {
        let mut app = app();
        let mut first = book("换源测试");
        let mut second = first.clone();
        second.origin = "http://second.invalid".into();
        second.book_url = "http://second.invalid/book".into();
        second.origin_name = Some("第二书源".into());
        let mut source = app.live.as_ref().unwrap().sources[0].clone();
        source.book_source_url = second.origin.clone();
        source.book_source_name = "第二书源".into();
        app.live.as_mut().unwrap().sources.push(source);
        first.source_candidates = Some(vec![query::candidate(&first), query::candidate(&second)]);
        second.source_candidates = first.source_candidates.clone();
        app.sidebar_hidden = true;
        app.load_shelf(vec![first.clone()]);
        app.install_reading(reading(first));
        app.commands.clear();
        (app, second)
    }

    #[test]
    fn preview_add_saves_current_position_and_removal_stops_autosave() {
        let mut app = app();
        app.commands.clear();
        let book = book("试读");
        app.install_reading(reading(book.clone()));
        assert!(app.commands.is_empty());
        assert!(!app.reader.as_ref().unwrap().on_shelf);
        app.reader.as_mut().unwrap().set_real(
            vec!["一".into(), "第二章".into()],
            "正文".repeat(100),
            57,
        );
        key(&mut app, KeyCode::Char('a'));
        assert!(
            matches!(app.commands.last(), Some(Command::Add(b)) if b.dur_chapter_pos == Some(57) && b.dur_chapter_index == Some(1))
        );
        app.apply_live_event(Event::Shelf(vec![book]));
        assert!(app.reader.as_ref().unwrap().on_shelf);
        app.commands.clear();
        app.save_live_progress();
        assert!(matches!(
            app.commands.last(),
            Some(Command::Progress { position: 57, .. })
        ));
        app.apply_live_event(Event::Shelf(vec![]));
        app.commands.clear();
        app.save_live_progress();
        assert!(app.commands.is_empty());
        assert!(!app.reader.as_ref().unwrap().on_shelf);
    }

    #[test]
    fn preview_switch_skips_shelf_commit() {
        let (mut app, second) = reader_app();
        app.apply_live_event(Event::Shelf(vec![]));
        key(&mut app, KeyCode::Char('s'));
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        let id = app.live.as_ref().unwrap().read_id;
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(reading(second.clone()))),
        });
        assert_eq!(app.reader.as_ref().unwrap().book.id, book_id(&second));
        assert!(!app
            .commands
            .iter()
            .any(|c| matches!(c, Command::CommitSwitch { .. } | Command::Progress { .. })));
    }

    #[test]
    fn shelf_removal_requires_confirmation_and_import_prompts_capture_keys() {
        let mut app = app();
        let mut local = book("本地书");
        local.origin = "local-txt".into();
        app.load_shelf(vec![local.clone()]);
        app.goto_id_for_test("shelf:all");
        app.focus = Focus::Main;
        app.commands.clear();
        key(&mut app, KeyCode::Char('x'));
        assert!(
            matches!(&app.live.as_ref().unwrap().modal, Some(crate::library::Modal::Confirm{label,..}) if label.contains("原文件保留"))
        );
        assert!(app.commands.is_empty());
        key(&mut app, KeyCode::Esc);
        assert!(app.commands.is_empty());
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Char('y'));
        assert!(
            matches!(app.commands.last(), Some(Command::Library(crate::library::Change::RemoveBook(b))) if b.book_url == local.book_url)
        );
        app.goto_id_for_test("set:purify");
        key(&mut app, KeyCode::Char('i'));
        assert!(matches!(
            app.live.as_ref().unwrap().prompt.as_ref().map(|p| p.kind),
            Some(InputKind::Rules)
        ));
        for ch in "https://fixture.invalid/rules.json".chars() {
            key(&mut app, KeyCode::Char(ch));
        }
        key(&mut app, KeyCode::Enter);
        assert!(
            matches!(app.commands.last(),Some(Command::ImportRules(url)) if url == "https://fixture.invalid/rules.json")
        );
        app.goto_id_for_test("set:prefs");
        key(&mut app, KeyCode::Char('i'));
        assert!(matches!(
            app.live.as_ref().unwrap().prompt.as_ref().map(|p| p.kind),
            Some(InputKind::Layout)
        ));
    }

    #[test]
    fn load_more_preserves_selection_and_ignores_late_previous_page() {
        let mut app = app();
        app.start_live_query(false);
        let first_id = app.live.as_ref().unwrap().query_id;
        app.apply_live_event(Event::QueryPart {
            id: first_id,
            source: "http://fixture.invalid".into(),
            page: 1,
            result: Ok(vec![book("甲"), book("乙")]),
        });
        app.apply_live_event(Event::QueryDone(first_id));
        app.search_sel = 1;
        key(&mut app, KeyCode::Char('n'));
        assert!(
            matches!(app.commands.last(), Some(Command::Query { requests, keyword, .. }) if requests[0].page == 2 && keyword == "真实")
        );
        let id = app.live.as_ref().unwrap().query_id;
        app.apply_live_event(Event::QueryPart {
            id: first_id,
            source: "http://fixture.invalid".into(),
            page: 1,
            result: Ok(vec![book("过期")]),
        });
        app.apply_live_event(Event::QueryPart {
            id,
            source: "http://fixture.invalid".into(),
            page: 2,
            result: Ok(vec![book("乙"), book("丙")]),
        });
        app.apply_live_event(Event::QueryDone(id));
        assert_eq!(app.search_sel, 1);
        assert_eq!(
            app.search_results()
                .iter()
                .map(|b| b.title.as_str())
                .collect::<Vec<_>>(),
            vec!["甲", "乙", "丙"]
        );
        key(&mut app, KeyCode::Char('n'));
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char('r'));
        assert!(
            matches!(app.commands.last(), Some(Command::Query { requests, .. }) if requests[0].page == 3)
        );
    }

    #[test]
    fn discovery_category_change_resets_pagination_and_discards_old_results() {
        let mut app = app();
        app.sources[0].categories = vec!["甲分类".into(), "乙分类".into()];
        app.sources[0].enabled = true;
        app.sources[0].explore = true;
        app.live.as_mut().unwrap().categories[0] = ["/a?page={{page}}", "/b?page={{page}}"]
            .into_iter()
            .map(|url| ExploreKind {
                title: url.into(),
                url: Some(url.into()),
                ..Default::default()
            })
            .collect();
        app.rebuild_nav();
        app.goto_id_for_test("discover:sources");
        key(&mut app, KeyCode::Enter);
        app.sync_live();
        let old = app.live.as_ref().unwrap().query_id;
        app.apply_live_event(Event::QueryPart {
            id: old,
            source: "http://fixture.invalid".into(),
            page: 1,
            result: Ok(vec![book("甲")]),
        });
        app.apply_live_event(Event::QueryDone(old));
        key(&mut app, KeyCode::Char('n'));
        let old = app.live.as_ref().unwrap().query_id;
        assert!(
            matches!(app.commands.last(), Some(Command::Query { requests, explore, .. }) if requests[0].page == 2 && explore.as_deref() == Some("/a?page={{page}}"))
        );
        key(&mut app, KeyCode::Right);
        app.sync_live();
        assert!(
            matches!(app.commands.last(), Some(Command::Query { requests, explore, .. }) if requests[0].page == 1 && explore.as_deref() == Some("/b?page={{page}}"))
        );
        app.apply_live_event(Event::QueryPart {
            id: old,
            source: "http://fixture.invalid".into(),
            page: 2,
            result: Ok(vec![book("旧分类")]),
        });
        app.apply_live_event(Event::QueryDone(old));
        assert!(app.live.as_ref().unwrap().discover.is_empty());
        assert!(app.live.as_ref().unwrap().querying);
    }

    #[test]
    fn discovery_lists_enabled_sources_before_categories_and_skips_heading_requests() {
        let mut app = app();
        app.live.as_mut().unwrap().sources[0].explore_url = Some("@js:dynamic()".into());
        app.sources[0].enabled = false; // Discovery is independent of search enablement.
        app.sources[0].explore = true;
        app.rebuild_nav();
        app.goto_id_for_test("discover:sources");
        assert_eq!(app.source_indices(), vec![0]);
        key(&mut app, KeyCode::Enter);
        app.sync_live();
        let (id, source) = app.live.as_ref().unwrap().category_request.clone().unwrap();
        assert!(matches!(
            app.commands.last(),
            Some(Command::ExploreKinds { .. })
        ));
        let count = app.commands.len();
        app.sync_live();
        assert_eq!(app.commands.len(), count);
        app.apply_live_event(Event::ExploreKinds {
            id,
            source,
            result: Ok(vec![
                ExploreKind {
                    title: "分组标题".into(),
                    url: Some("".into()),
                    ..Default::default()
                },
                ExploreKind {
                    title: "玄幻".into(),
                    url: Some("/fantasy?page={{page}}".into()),
                    ..Default::default()
                },
            ]),
        });
        app.sync_live();
        assert_eq!(app.sources[0].categories, vec!["玄幻"]);
        assert!(
            matches!(app.commands.last(), Some(Command::Query { explore:Some(url), .. }) if url=="/fantasy?page={{page}}")
        );
    }

    #[test]
    fn discovery_errors_are_visible_retryable_and_cancelled_results_are_ignored() {
        let mut app = app();
        app.live.as_mut().unwrap().sources[0].explore_url = Some("@js:dynamic()".into());
        app.sources[0].explore = true;
        app.rebuild_nav();
        app.goto_id_for_test("discover:sources");
        key(&mut app, KeyCode::Enter);
        app.sync_live();
        let (id, source) = app.live.as_ref().unwrap().category_request.clone().unwrap();
        app.apply_live_event(Event::ExploreKinds {
            id,
            source: source.clone(),
            result: Err("缺少接口".into()),
        });
        app.sync_live();
        assert!(app.live.as_ref().unwrap().status.contains("缺少接口"));
        assert!(app.live.as_ref().unwrap().category_request.is_none());
        key(&mut app, KeyCode::Char('r'));
        let (retry, _) = app.live.as_ref().unwrap().category_request.clone().unwrap();
        assert_ne!(retry, id);
        app.apply_live_event(Event::ExploreKinds {
            id,
            source: source.clone(),
            result: Ok(vec![]),
        });
        assert!(app.live.as_ref().unwrap().category_request.is_some());
        app.cancel_live_reads();
        app.apply_live_event(Event::ExploreKinds {
            id: retry,
            source: source.clone(),
            result: Ok(vec![ExploreKind {
                title: "旧结果".into(),
                url: Some("/old".into()),
                ..Default::default()
            }]),
        });
        assert!(app.live.as_ref().unwrap().categories[0].is_empty());
        key(&mut app, KeyCode::Char('r'));
        let (id, _) = app.live.as_ref().unwrap().category_request.clone().unwrap();
        app.apply_live_event(Event::ExploreKinds {
            id,
            source,
            result: Ok(vec![ExploreKind {
                title: "请先登录".into(),
                url: Some("".into()),
                ..Default::default()
            }]),
        });
        assert!(app.live.as_ref().unwrap().status.contains("请先登录"));
    }

    #[test]
    fn live_help_shows_pagination_and_source_actions_at_standard_size() {
        let mut app = app();
        key(&mut app, KeyCode::Char('?'));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
            .replace(' ', "");
        for hint in ["加载后续页", "重试失败页", "选择书源"] {
            assert!(text.contains(hint), "{hint}");
        }
    }

    #[test]
    fn source_picker_failure_and_cancel_preserve_reader_and_original_sidebar() {
        let (mut app, second) = reader_app();
        let original = app.reader.as_ref().unwrap().book.id.clone();
        key(&mut app, KeyCode::Char('s'));
        assert_eq!(
            app.live
                .as_ref()
                .unwrap()
                .picker
                .as_ref()
                .unwrap()
                .candidates
                .len(),
            2
        );
        for (w, h) in [(120, 40), (80, 24), (40, 12), (2, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            if w == 120 {
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>()
                    .replace(' ', "");
                assert!(text.contains("第二书源"));
                assert!(text.contains("Enter"));
            }
        }
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        let id = app.live.as_ref().unwrap().read_id;
        assert!(
            matches!(app.commands.last(), Some(Command::Switch { previous, .. }) if previous.dur_chapter_index == Some(1) && previous.dur_chapter_title.as_deref() == Some("第二章"))
        );
        app.apply_live_event(Event::Opened {
            id,
            result: Err("正文为空".into()),
        });
        assert_eq!(app.reader.as_ref().unwrap().book.id, original);
        assert!(!app
            .commands
            .iter()
            .any(|c| matches!(c, Command::CommitSwitch { .. })));
        key(&mut app, KeyCode::Enter);
        let id = app.live.as_ref().unwrap().read_id;
        key(&mut app, KeyCode::Esc);
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(reading(second))),
        });
        assert_eq!(app.reader.as_ref().unwrap().book.id, original);
        assert!(app.live.as_ref().unwrap().picker.is_none());
        key(&mut app, KeyCode::Char('q'));
        assert!(app.reader.is_none() && app.sidebar_hidden);
    }

    #[test]
    fn switch_waits_for_persisted_ack_and_failed_commit_is_retryable() {
        let (mut app, second) = reader_app();
        let original = app.reader.as_ref().unwrap().book.id.clone();
        key(&mut app, KeyCode::Char('s'));
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        let id = app.live.as_ref().unwrap().read_id;
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(reading(second.clone()))),
        });
        assert!(matches!(
            app.commands.last(),
            Some(Command::CommitSwitch { .. })
        ));
        let count = app.commands.len();
        app.save_live_progress();
        key(&mut app, KeyCode::Esc);
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(reading(second.clone()))),
        });
        assert_eq!(app.commands.len(), count);
        assert_eq!(app.reader.as_ref().unwrap().book.id, original);
        app.apply_live_event(Event::Switched {
            id,
            result: Err("保存失败".into()),
        });
        assert_eq!(app.reader.as_ref().unwrap().book.id, original);
        key(&mut app, KeyCode::Enter);
        let id = app.live.as_ref().unwrap().read_id;
        app.apply_live_event(Event::Opened {
            id,
            result: Ok(Box::new(reading(second.clone()))),
        });
        app.apply_live_event(Event::Switched {
            id,
            result: Ok(Box::new(reading(second.clone()))),
        });
        assert_eq!(app.reader.as_ref().unwrap().book.id, book_id(&second));
        assert!(app.live.as_ref().unwrap().picker.is_none());
        assert!(app.reader.as_ref().unwrap().previous_sidebar_hidden);
        app.apply_live_event(Event::Shelf(vec![second.clone()]));
        app.save_live_progress();
        assert!(
            matches!(app.commands.last(), Some(Command::Progress { book, .. }) if book.origin == second.origin)
        );
    }

    #[test]
    fn old_search_results_and_completion_cannot_replace_new_query() {
        let mut app = app();
        let mut second = app.live.as_ref().unwrap().sources[0].clone();
        second.book_source_url = "http://failed.invalid".into();
        app.live.as_mut().unwrap().sources.push(second);
        app.start_live_query(false);
        let old = app.live.as_ref().unwrap().query_id;
        app.start_live_query(false);
        let current = app.live.as_ref().unwrap().query_id;
        app.apply_live_event(Event::QueryPart {
            id: old,
            source: "旧".into(),
            page: 1,
            result: Ok(vec![book("过期")]),
        });
        app.apply_live_event(Event::QueryDone(old));
        assert!(app.search_results().is_empty());
        assert!(app.live.as_ref().unwrap().querying);
        app.apply_live_event(Event::QueryPart {
            id: current,
            source: "http://fixture.invalid".into(),
            page: 1,
            result: Ok(vec![book("当前")]),
        });
        app.apply_live_event(Event::QueryPart {
            id: current,
            source: "http://failed.invalid".into(),
            page: 1,
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
