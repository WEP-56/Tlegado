//! 状态机 · 按键路由

use crate::data::{self, Book, Kind, Source};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Route {
    Home,
    Shelf { filter: ShelfFilter },
    Discover { source_idx: usize },
    ExploreSources,
    Search,
    History,
    Sources,
    Purify,
    Prefs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShelfFilter {
    All,
    Local,
    Network,
}

/// 侧栏每一项
#[derive(Clone)]
pub struct NavItem {
    pub id: String,
    pub section: Option<&'static str>, // None = 顶级（首页）
    pub label: String,
    pub route: Route,
    pub right: String,
}

pub struct App {
    pub live: Option<crate::live::Live>,
    pub commands: Vec<crate::jobs::Command>,
    pub demo: crate::demo::Demo,
    pub reader: Option<crate::reader::Reader>,
    pub books: Vec<Book>,
    pub sources: Vec<Source>,
    pub source_browser: crate::sources::SourceBrowser,

    pub focus: Focus,
    pub nav: Vec<NavItem>,
    pub nav_idx: usize,
    pub collapsed: std::collections::HashMap<&'static str, bool>,
    pub sidebar_hidden: bool,

    // 主体内的局部选中
    pub shelf_group: usize, // 0 = 全部
    pub shelf_sel: usize,
    pub shelf_sort: usize,
    pub discover_cat: usize,
    pub discover_sel: usize,
    pub search_sel: usize,
    pub search_expanded: bool,
    pub search_query: String,
    pub search_input_mode: bool, // true = 正在输入

    pub help: bool,
    pub boss_mode: bool,
    pub toast: Option<(String, ToastTone, u8)>, // msg, tone, ticks left
    pub tick: u64,
    pub should_quit: bool,
}

#[derive(Clone, Copy)]
pub enum ToastTone {
    Ok,
    Err,
    Info,
}

impl App {
    pub fn new() -> Self {
        let books = data::shelf();
        let sources = data::sources();
        let mut app = Self {
            live: None,
            commands: Vec::new(),
            demo: crate::demo::Demo::new(&books, sources.len()),
            reader: None,
            books,
            sources,
            source_browser: crate::sources::SourceBrowser::default(),
            focus: Focus::Sidebar,
            nav: Vec::new(),
            nav_idx: 0,
            collapsed: std::collections::HashMap::new(),
            sidebar_hidden: false,
            shelf_group: 0,
            shelf_sel: 0,
            shelf_sort: 0,
            discover_cat: 0,
            discover_sel: 0,
            search_sel: 0,
            search_expanded: false,
            search_query: String::new(),
            search_input_mode: false,
            help: false,
            boss_mode: false,
            toast: None,
            tick: 0,
            should_quit: false,
        };
        app.rebuild_nav();
        app
    }

    pub fn rebuild_nav(&mut self) {
        let cur_id = self.nav.get(self.nav_idx).map(|n| n.id.clone());
        let mut items = Vec::new();

        items.push(NavItem {
            id: "home".into(),
            section: None,
            label: "首页".into(),
            route: Route::Home,
            right: String::new(),
        });

        let updates: u32 = self.books.iter().map(|b| b.new_count).sum();
        let local = self.books.iter().filter(|b| b.kind == Kind::Local).count();
        let total = self.books.len();

        items.push(NavItem {
            id: "shelf:all".into(),
            section: Some("书架"),
            label: "全部书籍".into(),
            route: Route::Shelf {
                filter: ShelfFilter::All,
            },
            right: total.to_string(),
        });
        items.push(NavItem {
            id: "shelf:local".into(),
            section: Some("书架"),
            label: "本地图书".into(),
            route: Route::Shelf {
                filter: ShelfFilter::Local,
            },
            right: local.to_string(),
        });
        items.push(NavItem {
            id: "shelf:network".into(),
            section: Some("书架"),
            label: "网络图书".into(),
            route: Route::Shelf {
                filter: ShelfFilter::Network,
            },
            right: if updates > 0 {
                format!("+{updates} {}", total - local)
            } else {
                (total - local).to_string()
            },
        });

        items.push(NavItem {
            id: "discover:search".into(),
            section: Some("发现"),
            label: "搜索书籍".into(),
            route: Route::Search,
            right: "/".into(),
        });

        items.push(NavItem {
            id: "discover:sources".into(),
            section: Some("发现"),
            label: "发现书源".into(),
            route: Route::ExploreSources,
            right: self
                .sources
                .iter()
                .enumerate()
                .filter(|(i, _)| self.discovery_available(*i))
                .count()
                .to_string(),
        });
        for (i, s) in self.sources.iter().enumerate() {
            if self.source_browser.active_explore.as_deref() == Some(s.id.as_str())
                && self.discovery_available(i)
            {
                items.push(NavItem {
                    id: format!("discover:{}", s.id),
                    section: Some("发现"),
                    label: s.name.clone(),
                    route: Route::Discover { source_idx: i },
                    right: format!("{}类", s.categories.len()),
                });
            }
        }

        let enabled = self.sources.iter().filter(|s| s.enabled).count();
        items.push(NavItem {
            id: "set:history".into(),
            section: Some("设置"),
            label: "阅读历史".into(),
            route: Route::History,
            right: self.demo.history.len().to_string(),
        });
        items.push(NavItem {
            id: "set:sources".into(),
            section: Some("设置"),
            label: "书源管理".into(),
            route: Route::Sources,
            right: format!("{enabled}/{}", self.sources.len()),
        });
        items.push(NavItem {
            id: "set:purify".into(),
            section: Some("设置"),
            label: "净化规则".into(),
            route: Route::Purify,
            right: self
                .live
                .as_ref()
                .map_or_else(
                    || self.demo.rules.iter().filter(|r| r.enabled).count(),
                    |live| live.library.rules.iter().filter(|r| r.is_enabled).count(),
                )
                .to_string(),
        });
        items.push(NavItem {
            id: "set:prefs".into(),
            section: Some("设置"),
            label: "阅读偏好".into(),
            route: Route::Prefs,
            right: String::new(),
        });

        self.nav = items;
        if let Some(id) = cur_id {
            if let Some(i) = self.nav.iter().position(|n| n.id == id) {
                self.nav_idx = i;
            } else {
                self.nav_idx = if id.starts_with("discover:") {
                    self.nav
                        .iter()
                        .position(|n| n.id == "discover:sources")
                        .unwrap_or(0)
                } else {
                    self.nav_idx.min(self.nav.len().saturating_sub(1))
                };
            }
        }
    }

    pub fn route(&self) -> &Route {
        &self.nav[self.nav_idx].route
    }

    /// 可见的侧栏索引（折叠的 section 子项被跳过）
    pub fn visible_nav(&self) -> Vec<usize> {
        self.nav
            .iter()
            .enumerate()
            .filter(|(_, n)| match n.section {
                Some(sec) => !self.collapsed.get(sec).copied().unwrap_or(false),
                None => true,
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn toast(&mut self, msg: impl Into<String>, tone: ToastTone) {
        // ~3 秒（假设 10 ticks/s → 30）
        self.toast = Some((msg.into(), tone, 30));
    }

    pub fn on_tick(&mut self) {
        if self.live.is_none() {
            self.demo.tick(&self.sources);
        }
        self.tick = self.tick.wrapping_add(1);
        if !self.boss_mode {
            if let Some(reader) = &mut self.reader {
                reader.tick_auto(100);
            }
        }
        if let Some((_, _, ref mut left)) = self.toast {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.toast = None;
            }
        }
    }

    // ── 按键 ────────────────────────────────────────────────
    pub fn on_key(&mut self, key: KeyEvent) {
        // Most terminals encode Ctrl+Shift+Q as Ctrl+Q and omit the Shift bit.
        // Accept both encodings so the boss key works across terminal emulators.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('q' | 'Q'))
        {
            self.boss_mode = !self.boss_mode;
            self.help = false;
            return;
        }
        if self.boss_mode {
            return;
        }
        if self.live_key(key) {
            return;
        }
        if self.source_browser_key(key) {
            return;
        }
        // Ctrl+B 切侧栏
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
            self.sidebar_hidden = !self.sidebar_hidden;
            if self.sidebar_hidden {
                self.focus = Focus::Main;
            }
            return;
        }

        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => self.help = false,
                _ => {}
            }
            return;
        }

        if self.reader.is_some() {
            self.on_key_reader(key);
            return;
        }
        if self.demo.json
            && matches!(self.route(), Route::Sources)
            && matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('v')
            )
        {
            self.demo.json = false;
            self.demo.detail_scroll = 0;
            return;
        }

        // 搜索输入模式：把字符吃进 query
        if self.search_input_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => self.search_input_mode = false,
                KeyCode::Enter => {
                    self.search_input_mode = false;
                    if self.search_query.trim().is_empty() {
                        self.toast("请输入关键词", ToastTone::Err);
                    } else {
                        self.toast(
                            format!(
                                "搜索「{}」（演示：结果为本地过滤）",
                                self.search_query.trim()
                            ),
                            ToastTone::Info,
                        );
                        self.search_sel = 0;
                        self.search_expanded = false;
                    }
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.search_query.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Tab => {
                if self.sidebar_hidden {
                    self.focus = Focus::Main;
                } else {
                    self.focus = match self.focus {
                        Focus::Sidebar => Focus::Main,
                        Focus::Main => Focus::Sidebar,
                    };
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('/') => {
                // 跳到搜索
                if let Some(i) = self.nav.iter().position(|n| n.id == "discover:search") {
                    self.nav_idx = i;
                    self.focus = Focus::Main;
                    self.search_input_mode = true;
                }
            }
            KeyCode::Char('q') if self.focus == Focus::Sidebar => self.should_quit = true,
            KeyCode::Esc if self.focus == Focus::Main => {
                if !self.sidebar_hidden {
                    self.focus = Focus::Sidebar;
                }
            }
            _ => match self.focus {
                Focus::Sidebar => self.on_key_sidebar(key),
                Focus::Main => self.on_key_main(key),
            },
        }
    }

    fn on_key_sidebar(&mut self, key: KeyEvent) {
        let vis = self.visible_nav();
        let pos = vis.iter().position(|&i| i == self.nav_idx).unwrap_or(0);
        let move_by = |pos: usize, d: isize, vis: &[usize]| -> usize {
            let np = (pos as isize + d).clamp(0, vis.len().saturating_sub(1) as isize) as usize;
            vis[np]
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.nav_idx = move_by(pos, 1, &vis);
                self.reset_main_sel();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.nav_idx = move_by(pos, -1, &vis);
                self.reset_main_sel();
            }
            KeyCode::Char('g') => {
                if let Some(&i) = vis.first() {
                    self.nav_idx = i;
                    self.reset_main_sel();
                }
            }
            KeyCode::Char('G') => {
                if let Some(&i) = vis.last() {
                    self.nav_idx = i;
                    self.reset_main_sel();
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.focus = Focus::Main;
                if matches!(self.route(), Route::Search) {
                    self.search_input_mode = true;
                }
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char(' ') => {
                // 折叠 / 展开 section
                if let Some(sec) = self.nav[self.nav_idx].section {
                    let now = self.collapsed.get(sec).copied().unwrap_or(false);
                    self.collapsed.insert(sec, !now);
                    if !now {
                        // 刚折叠：把光标移到该 section 第一项
                        if let Some(i) = self.nav.iter().position(|n| n.section == Some(sec)) {
                            self.nav_idx = i;
                        }
                    }
                }
            }
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    fn on_key_main(&mut self, key: KeyEvent) {
        match self.route().clone() {
            Route::Home => self.on_key_home(key),
            Route::Shelf { filter } => self.on_key_shelf(key, filter),
            Route::Discover { source_idx } => self.on_key_discover(key, source_idx),
            Route::ExploreSources => {}
            Route::Search => self.on_key_search(key),
            Route::History | Route::Sources | Route::Purify | Route::Prefs => {
                self.demo_key(key.code)
            }
        }
    }

    fn on_key_home(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('b') => self.goto_id("shelf:all"),
            KeyCode::Char('e') => {
                self.goto_id("discover:sources");
            }
            KeyCode::Char('s') => self.goto_id("set:sources"),
            KeyCode::Char('o') => {
                self.toast("已扫描 ~/Books ，发现 2 本新书（演示）", ToastTone::Info)
            }
            KeyCode::Char('c') => {
                if let Some(book) = self
                    .demo
                    .history
                    .first()
                    .map(|h| h.book.clone())
                    .or_else(|| self.books.first().cloned())
                {
                    self.open_reader(book);
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    fn on_key_shelf(&mut self, key: KeyEvent, filter: ShelfFilter) {
        let list = self.shelf_list(filter);
        let groups = self.shelf_groups(filter);
        let len = list.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if len > 0 => {
                self.shelf_sel = (self.shelf_sel + 1).min(len - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.shelf_sel = self.shelf_sel.saturating_sub(1);
            }
            KeyCode::Char('g') => self.shelf_sel = 0,
            KeyCode::Char('G') => self.shelf_sel = len.saturating_sub(1),
            KeyCode::Char('h') | KeyCode::Left if !groups.is_empty() => {
                self.shelf_group = (self.shelf_group + groups.len() - 1) % groups.len();
                self.shelf_sel = 0;
            }
            KeyCode::Char('l') | KeyCode::Right if !groups.is_empty() => {
                self.shelf_group = (self.shelf_group + 1) % groups.len();
                self.shelf_sel = 0;
            }
            KeyCode::Char('s') => {
                self.shelf_sort = (self.shelf_sort + 1) % 4;
                self.shelf_sel = 0;
                let names = ["最近阅读", "书名", "更新数", "进度"];
                self.toast(
                    format!("排序 → {}", names[self.shelf_sort]),
                    ToastTone::Info,
                );
            }
            KeyCode::Char('r') => {
                // 模拟检查更新
                for b in &mut self.books {
                    if b.kind == Kind::Network && b.status == "连载" {
                        b.new_count += 1;
                        b.total += 1;
                    }
                }
                self.rebuild_nav();
                self.toast("检查更新完成：3 本书有新章节", ToastTone::Ok);
            }
            KeyCode::Enter => {
                if let Some(b) = list.get(self.shelf_sel) {
                    self.open_reader((*b).clone());
                }
            }
            KeyCode::Char('x') => {
                if let Some(b) = list.get(self.shelf_sel) {
                    let id = b.id.clone();
                    let title = b.title.to_string();
                    self.books.retain(|x| x.id != id);
                    self.rebuild_nav();
                    if self.shelf_sel >= self.shelf_list(filter).len() && self.shelf_sel > 0 {
                        self.shelf_sel -= 1;
                    }
                    self.toast(format!("已移出《{title}》"), ToastTone::Err);
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    fn on_key_discover(&mut self, key: KeyEvent, source_idx: usize) {
        let cats = self
            .sources
            .get(source_idx)
            .map(|s| s.categories.as_slice())
            .unwrap_or(&[]);
        let books = self.discover_list(source_idx);
        let len = books.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if len > 0 => {
                self.discover_sel = (self.discover_sel + 1).min(len - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.discover_sel = self.discover_sel.saturating_sub(1);
            }
            KeyCode::Char('h') | KeyCode::Left if !cats.is_empty() => {
                self.discover_cat = (self.discover_cat + cats.len() - 1) % cats.len();
                self.discover_sel = 0;
            }
            KeyCode::Char('l') | KeyCode::Right if !cats.is_empty() => {
                self.discover_cat = (self.discover_cat + 1) % cats.len();
                self.discover_sel = 0;
            }
            KeyCode::Enter => {
                if let Some(b) = books.get(self.discover_sel) {
                    self.open_reader(b.clone());
                }
            }
            KeyCode::Char('a') => {
                if let Some(b) = books.get(self.discover_sel) {
                    self.add_demo_book(b.clone());
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    fn on_key_search(&mut self, key: KeyEvent) {
        let results = self.search_results();
        let selected_book = results.get(self.search_sel).map(|b| (*b).clone());
        let current_origin = results.get(self.search_sel).map(|b| b.origin.clone());
        let mut source_first = Vec::new();
        let mut seen_sources = std::collections::HashSet::new();
        for (index, book) in results.iter().enumerate() {
            if seen_sources.insert(book.origin.clone()) {
                source_first.push((index, book.origin.clone()));
            }
        }
        let source_pos = source_first
            .iter()
            .position(|(_, origin)| Some(origin) == current_origin.as_ref())
            .unwrap_or(0);
        let group: Vec<usize> = results
            .iter()
            .enumerate()
            .filter_map(|(index, book)| {
                (Some(&book.origin) == current_origin.as_ref()).then_some(index)
            })
            .collect();
        let group_pos = group
            .iter()
            .position(|index| *index == self.search_sel)
            .unwrap_or(0);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down
                if !self.search_expanded && !source_first.is_empty() =>
            {
                self.search_sel = source_first[(source_pos + 1).min(source_first.len() - 1)].0;
            }
            KeyCode::Char('k') | KeyCode::Up
                if !self.search_expanded && !source_first.is_empty() =>
            {
                self.search_sel = source_first[source_pos.saturating_sub(1)].0;
            }
            KeyCode::Char('j') | KeyCode::Down if self.search_expanded && !group.is_empty() => {
                self.search_sel = group[(group_pos + 1).min(group.len() - 1)];
            }
            KeyCode::Char('k') | KeyCode::Up if self.search_expanded && !group.is_empty() => {
                self.search_sel = group[group_pos.saturating_sub(1)];
            }
            KeyCode::Char('i') | KeyCode::Char('/') => {
                self.search_input_mode = true;
            }
            KeyCode::Enter if !self.search_expanded => {
                self.search_expanded = !source_first.is_empty();
                if self.live.is_none() && source_first.len() == 1 {
                    if let Some(b) = selected_book.clone() {
                        self.open_reader(b);
                    }
                }
            }
            KeyCode::Enter if self.search_expanded => {
                if let Some(b) = selected_book.clone() {
                    self.open_reader(b);
                }
            }
            KeyCode::Char('a') if self.search_expanded => {
                if let Some(b) = selected_book {
                    self.add_demo_book(b);
                }
            }
            KeyCode::Char('q') | KeyCode::Esc if self.search_expanded => {
                self.search_expanded = false;
                self.search_sel = source_first
                    .get(source_pos)
                    .map(|(index, _)| *index)
                    .unwrap_or(0);
            }
            KeyCode::Char('q') | KeyCode::Esc => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    pub(crate) fn open_reader(&mut self, book: Book) {
        if self.live.is_some() {
            self.open_live(&book);
            return;
        }
        if book.total == 0 {
            self.toast("暂无章节可预览", ToastTone::Info);
            return;
        }
        self.reader = Some(crate::reader::Reader::new(book, self.sidebar_hidden));
        if let Some(reader) = &mut self.reader {
            reader.options = self.demo.prefs.clone();
            reader.rules = self.demo.rules.clone();
        }
        self.sidebar_hidden = false;
        self.focus = Focus::Main;
        self.search_input_mode = false;
        self.toast = None;
    }

    fn on_key_reader(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            if let Some(reader) = self.reader.take() {
                if self.live.is_none() {
                    self.demo.record(reader.book.clone(), reader.chapter);
                }
                self.sidebar_hidden = reader.previous_sidebar_hidden;
                if let Some(book) = self.books.iter_mut().find(|b| b.id == reader.book.id) {
                    book.read = reader.chapter;
                }
                self.rebuild_nav();
            }
            self.focus = Focus::Main;
            return;
        }
        let reader = self.reader.as_mut().expect("active reader");
        match key.code {
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('A') => reader.toggle_auto(),
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.sidebar_hidden || self.focus == Focus::Sidebar {
                    Focus::Main
                } else {
                    Focus::Sidebar
                };
            }
            KeyCode::Char('t') => {
                self.sidebar_hidden = false;
                self.focus = Focus::Sidebar;
                reader.selected = reader.chapter;
            }
            KeyCode::Char('[') => reader.change_chapter(-1),
            KeyCode::Char(']') => reader.change_chapter(1),
            _ if self.focus == Focus::Sidebar => match key.code {
                KeyCode::Char('j') | KeyCode::Down => reader.select(1),
                KeyCode::Char('k') | KeyCode::Up => reader.select(-1),
                KeyCode::PageDown => reader.select(10),
                KeyCode::PageUp => reader.select(-10),
                KeyCode::Char('g') | KeyCode::Home => reader.selected = 0,
                KeyCode::Char('G') | KeyCode::End => {
                    reader.selected = reader.book.total.saturating_sub(1)
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    reader.open_selected();
                    self.focus = Focus::Main;
                }
                _ => {}
            },
            KeyCode::Char('j') | KeyCode::Down => reader.scroll(1),
            KeyCode::Char('k') | KeyCode::Up => reader.scroll(-1),
            KeyCode::Left | KeyCode::Char('h') => reader.horizontal_arrow(false),
            KeyCode::Right | KeyCode::Char('l') => reader.horizontal_arrow(true),
            KeyCode::Char(' ') | KeyCode::PageDown => reader.page(true),
            KeyCode::PageUp => reader.page(false),
            KeyCode::Char('g') | KeyCode::Home => reader.offset = 0,
            KeyCode::Char('G') | KeyCode::End => reader.offset = reader.max_offset(),
            _ => {}
        }
    }

    pub(crate) fn goto_id(&mut self, id: &str) {
        if let Some(i) = self.nav.iter().position(|n| n.id == id) {
            self.nav_idx = i;
            self.focus = Focus::Main;
            self.reset_main_sel();
        }
    }

    #[cfg(test)]
    pub(crate) fn goto_id_for_test(&mut self, id: &str) {
        self.goto_id(id);
    }

    fn reset_main_sel(&mut self) {
        self.shelf_group = 0;
        self.shelf_sel = 0;
        self.discover_cat = 0;
        self.discover_sel = 0;
        self.search_sel = 0;
    }

    // ── 派生数据 ────────────────────────────────────────────
    pub fn shelf_base(&self, filter: ShelfFilter) -> Vec<&Book> {
        self.books
            .iter()
            .filter(|b| match filter {
                ShelfFilter::All => true,
                ShelfFilter::Local => b.kind == Kind::Local,
                ShelfFilter::Network => b.kind == Kind::Network,
            })
            .collect()
    }

    pub fn shelf_groups(&self, filter: ShelfFilter) -> Vec<String> {
        let mut g = vec!["全部".to_string()];
        if let Some(live) = &self.live {
            g.push("未分组".into());
            g.extend(
                live.library
                    .groups
                    .iter()
                    .map(|group| group.group_name.clone()),
            );
        }
        for b in self.shelf_base(filter) {
            if !g.iter().any(|x| x == &b.group) {
                g.push(b.group.to_string());
            }
        }
        g
    }

    pub fn shelf_list(&self, filter: ShelfFilter) -> Vec<&Book> {
        let groups = self.shelf_groups(filter);
        let g = groups
            .get(self.shelf_group)
            .map(|s| s.as_str())
            .unwrap_or("全部");
        let mut list: Vec<&Book> = self
            .shelf_base(filter)
            .into_iter()
            .filter(|b| g == "全部" || b.group == g)
            .collect();
        match self.shelf_sort {
            1 => list.sort_by(|a, b| a.title.cmp(&b.title)),
            2 => list.sort_by_key(|b| std::cmp::Reverse(b.new_count)),
            3 => list.sort_by(|a, b| {
                let pa = a.read as f64 / a.total.max(1) as f64;
                let pb = b.read as f64 / b.total.max(1) as f64;
                pb.partial_cmp(&pa).unwrap()
            }),
            _ => {}
        }
        list
    }

    pub fn discover_list(&self, source_idx: usize) -> Vec<Book> {
        if let Some(live) = &self.live {
            return live.discover.clone();
        }
        let s = match self.sources.get(source_idx) {
            Some(s) => s,
            None => return vec![],
        };
        let cat = s
            .categories
            .get(self.discover_cat)
            .map(String::as_str)
            .unwrap_or("");
        let mut books = data::discover_books(&s.name, cat);
        // 把 origin 显示名塞进 intro 前缀，绘制时用 s.name
        for b in &mut books {
            b.origin = s.name.clone();
        }
        books
    }

    pub fn search_results(&self) -> Vec<&Book> {
        if let Some(live) = &self.live {
            return live.search.iter().collect();
        }
        let q = self.search_query.trim();
        if q.is_empty() {
            return vec![];
        }
        self.books
            .iter()
            .filter(|b| b.title.contains(q) || b.author.contains(q))
            .collect()
    }

    pub fn crumb(&self) -> String {
        if let Some(reader) = &self.reader {
            return format!("阅读 › {}", reader.book.title);
        }
        let n = &self.nav[self.nav_idx];
        match n.section {
            Some(sec) => format!("{sec} › {}", n.label),
            None => n.label.clone(),
        }
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn shelf_opens_selected_book_and_restores_selection() {
        let mut app = App::new();
        app.goto_id("shelf:all");
        app.shelf_sel = 2;
        let id = app.shelf_list(ShelfFilter::All)[2].id.clone();
        app.sidebar_hidden = true;
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.reader.as_ref().unwrap().book.id, id);
        assert!(!app.sidebar_hidden);
        key(&mut app, KeyCode::Char(']'));
        let chapter = app.reader.as_ref().unwrap().chapter;
        key(&mut app, KeyCode::Esc);
        assert!(app.reader.is_none());
        assert!(app.sidebar_hidden);
        assert_eq!(app.shelf_sel, 2);
        assert_eq!(app.books.iter().find(|b| b.id == id).unwrap().read, chapter);
        assert!(!app.should_quit);
    }

    #[test]
    fn chapters_select_before_open_and_clamp_at_bounds() {
        let mut app = App::new();
        app.open_reader(app.books[0].clone());
        let original = app.reader.as_ref().unwrap().chapter;
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Down);
        assert_eq!(app.reader.as_ref().unwrap().chapter, original);
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.reader.as_ref().unwrap().chapter, original + 1);
        key(&mut app, KeyCode::Char('t'));
        key(&mut app, KeyCode::Char('G'));
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.reader.as_ref().unwrap().chapter, app.books[0].total - 1);
        key(&mut app, KeyCode::Char('?'));
        key(&mut app, KeyCode::Char('q'));
        assert!(!app.help && app.reader.is_some());
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Char('q'));
        assert!(app.reader.is_none() && !app.should_quit);
    }

    #[test]
    fn home_search_discover_open_preview() {
        let mut app = App::new();
        app.focus = Focus::Main;
        key(&mut app, KeyCode::Char('c'));
        assert!(app.reader.is_some());
        key(&mut app, KeyCode::Esc);
        app.goto_id("discover:search");
        app.search_query = "三体".into();
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.reader.as_ref().unwrap().book.title, "三体");
        key(&mut app, KeyCode::Esc);
        app.goto_id("discover:sources");
        key(&mut app, KeyCode::Enter);
        let expected = app.discover_list(0)[0].title.clone();
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.reader.as_ref().unwrap().book.title, expected);
    }

    #[test]
    fn reader_renders_chapters_scrolls_and_handles_resize() {
        let mut app = App::new();
        app.open_reader(app.books[0].clone());
        for (width, height) in [(120, 40), (80, 24), (44, 16), (20, 8), (2, 2), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            if width >= 80 {
                let buffer = terminal.backend().buffer();
                let mut text = String::new();
                for y in 0..height {
                    let mut x = 0;
                    while x < width {
                        let symbol = buffer[(x, y)].symbol();
                        text.push_str(symbol);
                        x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                    }
                    text.push(' ');
                }
                assert!(text.contains("章节目录"));
                assert!(text.contains("阅读预览"));
                assert!(!text.contains("全部书籍"));
                key(&mut app, KeyCode::PageDown);
                assert!(app.reader.as_ref().unwrap().offset > 0);
                key(&mut app, KeyCode::Char('G'));
                let chapter = app.reader.as_ref().unwrap().chapter;
                key(&mut app, KeyCode::PageDown);
                let reader = app.reader.as_ref().unwrap();
                assert_eq!(reader.chapter, chapter + 1);
                assert_eq!(reader.selected, reader.chapter);
                assert_eq!(reader.offset, 0);
            }
        }
    }

    #[test]
    fn empty_book_does_not_enter_reader() {
        let mut app = App::new();
        let mut book = app.books[0].clone();
        book.total = 0;
        app.open_reader(book);
        assert!(app.reader.is_none());
    }

    #[test]
    fn ctrl_shift_q_toggles_boss_terminal_without_closing_reader() {
        let mut app = App::new();
        assert!(!app.boss_mode);
        app.on_key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert!(app.boss_mode && !app.should_quit);
        app.open_reader(app.books[0].clone());
        app.on_key(KeyEvent::new(
            KeyCode::Char('Q'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert!(!app.boss_mode && app.reader.is_some());
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(app.boss_mode);
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(!app.boss_mode);
    }

    #[test]
    fn auto_reader_toggle_and_tick_move_at_configured_interval() {
        let mut app = App::new();
        app.open_reader(app.books[0].clone());
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let reader = app.reader.as_mut().unwrap();
        reader.options.0[10] = 0;
        reader.toggle_auto();
        assert!(reader.auto_active());
        assert_eq!(reader.auto_progress(), 0.0);
        reader.tick_auto(900);
        assert_eq!(reader.offset, 0);
        assert!(reader.auto_progress() > 0.8);
        reader.tick_auto(100);
        assert!(reader.offset > 0 || reader.request.is_some());
        reader.toggle_auto();
        let offset = reader.offset;
        reader.tick_auto(5_000);
        assert_eq!(reader.offset, offset);
    }

    #[test]
    fn horizontal_controls_block_vertical_scrolling_and_follow_arrow_direction() {
        let mut app = App::new();
        app.demo.prefs.0[4] = 1;
        app.demo.prefs.0[8] = 1;
        app.open_reader(app.books[0].clone());
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        for direction in [0, 1] {
            app.reader.as_mut().unwrap().options.0[9] = direction;
            key(&mut app, KeyCode::Home);
            for code in [
                KeyCode::Down,
                KeyCode::Up,
                KeyCode::Char('j'),
                KeyCode::Char('k'),
            ] {
                key(&mut app, code);
                assert_eq!(app.reader.as_ref().unwrap().offset, 0);
            }
            key(
                &mut app,
                if direction == 0 {
                    KeyCode::Right
                } else {
                    KeyCode::Left
                },
            );
            let offset = app.reader.as_ref().unwrap().offset;
            assert!(offset > 0);
            key(&mut app, KeyCode::Down);
            assert_eq!(app.reader.as_ref().unwrap().offset, offset);
            key(&mut app, KeyCode::Char(' '));
            assert!(app.reader.as_ref().unwrap().offset > offset);
            key(&mut app, KeyCode::PageUp);
            assert_eq!(app.reader.as_ref().unwrap().offset, offset);
            key(
                &mut app,
                if direction == 0 {
                    KeyCode::Left
                } else {
                    KeyCode::Right
                },
            );
            assert_eq!(app.reader.as_ref().unwrap().offset, 0);
        }
        key(&mut app, KeyCode::Tab);
        let selected = app.reader.as_ref().unwrap().selected;
        key(&mut app, KeyCode::Down);
        assert_eq!(app.reader.as_ref().unwrap().selected, selected + 1);
    }
}
