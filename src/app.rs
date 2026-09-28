use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;

use crate::event::{is_left_click, Event};

#[derive(Debug, Clone, Copy)]
pub struct NavEntry {
    pub label: &'static str,
    #[allow(dead_code)]
    pub count: &'static str,
    pub route: Route,
}

pub const NAV_ENTRIES: [NavEntry; 11] = [
    NavEntry {
        label: "首页",
        count: "",
        route: Route::Home,
    },
    NavEntry {
        label: "全部书籍",
        count: "10",
        route: Route::Shelf,
    },
    NavEntry {
        label: "本地书库",
        count: "4",
        route: Route::Shelf,
    },
    NavEntry {
        label: "网络书库",
        count: "+31",
        route: Route::Shelf,
    },
    NavEntry {
        label: "搜索书籍",
        count: "/",
        route: Route::Search,
    },
    NavEntry {
        label: "起点中文网",
        count: "7类",
        route: Route::Search,
    },
    NavEntry {
        label: "番茄小说",
        count: "5类",
        route: Route::Search,
    },
    NavEntry {
        label: "笔趣阁①",
        count: "6类",
        route: Route::Search,
    },
    NavEntry {
        label: "阅读历史",
        count: "8",
        route: Route::History,
    },
    NavEntry {
        label: "书源管理",
        count: "6/8",
        route: Route::Sources,
    },
    NavEntry {
        label: "阅读偏好",
        count: "",
        route: Route::Preferences,
    },
];

#[derive(Debug, Clone, Copy)]
pub enum HitTarget {
    Sidebar(usize),
    MainRow(usize),
}

#[derive(Debug, Clone, Copy)]
pub struct HitRegion {
    pub area: Rect,
    pub target: HitTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Shelf,
    Search,
    History,
    Sources,
    Preferences,
    Reader,
}

impl Route {
    #[allow(dead_code)]
    pub const ALL: [Self; 6] = [
        Self::Home,
        Self::Shelf,
        Self::Search,
        Self::History,
        Self::Sources,
        Self::Preferences,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "首页",
            Self::Shelf => "书架",
            Self::Search => "搜索",
            Self::History => "阅读历史",
            Self::Sources => "书源管理",
            Self::Preferences => "阅读偏好",
            Self::Reader => "阅读",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Key(KeyCode, KeyModifiers),
    Mouse(MouseEventKind, u16, u16),
    Resize(u16, u16),
}

impl From<Event> for Action {
    fn from(event: Event) -> Self {
        match event {
            Event::Key(key) => Self::Key(key.code, key.modifiers),
            Event::Mouse { kind, column, row } => Self::Mouse(kind, column, row),
            Event::Resize(width, height) => Self::Resize(width, height),
        }
    }
}

#[derive(Debug)]
pub struct App {
    pub route: Route,
    pub focus: Focus,
    pub sidebar_visible: bool,
    pub help_visible: bool,
    pub should_quit: bool,
    pub selected: usize,
    pub main_selected: usize,
    pub notice: Option<String>,
    pub sidebar_area: Rect,
    pub hit_regions: Vec<HitRegion>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            route: Route::Home,
            focus: Focus::Sidebar,
            sidebar_visible: true,
            help_visible: false,
            should_quit: false,
            selected: 0,
            main_selected: 0,
            notice: Some("Tlegado TUI 骨架已启动".to_string()),
            sidebar_area: Rect::default(),
            hit_regions: Vec::new(),
        }
    }
}

impl App {
    pub fn dispatch(&mut self, action: Action) {
        if self.help_visible {
            if matches!(
                action,
                Action::Key(KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q'), _)
            ) {
                self.help_visible = false;
            }
            return;
        }

        match action {
            Action::Key(KeyCode::Char('q') | KeyCode::Esc, _) => {
                if self.route == Route::Reader {
                    self.select_route(Route::Shelf);
                    self.focus = Focus::Main;
                } else {
                    self.should_quit = true;
                }
            }
            Action::Key(KeyCode::Char('?'), _) => self.help_visible = true,
            Action::Key(KeyCode::Char('b'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.sidebar_visible = !self.sidebar_visible;
                if !self.sidebar_visible {
                    self.focus = Focus::Main;
                }
            }
            Action::Key(KeyCode::Tab, _) => {
                self.focus = match self.focus {
                    Focus::Sidebar if self.sidebar_visible => Focus::Main,
                    _ => Focus::Sidebar,
                };
            }
            Action::Key(KeyCode::Char('/'), _) => self.select_route(Route::Search),
            Action::Key(code, _) => self.handle_key(code),
            Action::Mouse(kind, column, row) => self.handle_mouse(kind, column, row),
            Action::Resize(width, height) => {
                self.notice = Some(format!("终端尺寸：{width}x{height}"));
            }
        }
    }

    pub fn set_sidebar_area(&mut self, area: Rect) {
        self.sidebar_area = area;
    }

    pub fn begin_frame(&mut self) {
        self.hit_regions.clear();
    }

    pub fn register_hit(&mut self, area: Rect, target: HitTarget) {
        self.hit_regions.push(HitRegion { area, target });
    }

    fn handle_key(&mut self, code: KeyCode) {
        match self.focus {
            Focus::Sidebar => match code {
                KeyCode::Down | KeyCode::Char('j') => self.move_sidebar(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_sidebar(-1),
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Main,
                _ => {}
            },
            Focus::Main => match code {
                KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Sidebar,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.main_selected = self.main_selected.saturating_add(1);
                    self.notice = Some("主体区域：下一项".to_string())
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.main_selected = self.main_selected.saturating_sub(1);
                    self.notice = Some("主体区域：上一项".to_string())
                }
                KeyCode::Enter if self.route == Route::Shelf => {
                    self.route = Route::Reader;
                    self.notice = Some("正在阅读：诡秘之主 · 第813章 雾中来客".to_string());
                }
                KeyCode::Enter => self.notice = Some(format!("已打开{}", self.route.title())),
                _ => {}
            },
        }
    }

    fn handle_mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        if is_left_click(kind) {
            if let Some(region) = self
                .hit_regions
                .iter()
                .rev()
                .find(|region| region.area.contains((column, row).into()))
                .copied()
            {
                match region.target {
                    HitTarget::Sidebar(index) => self.select_entry(index),
                    HitTarget::MainRow(index) => {
                        self.main_selected = index;
                        self.focus = Focus::Main;
                        self.notice = Some(format!("已选择第 {} 项，按 Enter 打开", index + 1));
                    }
                }
            }
        } else if matches!(kind, MouseEventKind::ScrollDown) {
            if self.focus == Focus::Sidebar {
                self.move_sidebar(1);
            } else {
                self.main_selected = self.main_selected.saturating_add(1);
            }
            self.notice = Some("向下滚动".to_string());
        } else if matches!(kind, MouseEventKind::ScrollUp) {
            if self.focus == Focus::Sidebar {
                self.move_sidebar(-1);
            } else {
                self.main_selected = self.main_selected.saturating_sub(1);
            }
            self.notice = Some("向上滚动".to_string());
        }
    }

    fn move_sidebar(&mut self, delta: i32) {
        let max = NAV_ENTRIES.len().saturating_sub(1) as i32;
        self.selected = (self.selected as i32 + delta).clamp(0, max) as usize;
        self.route = NAV_ENTRIES[self.selected].route;
    }

    fn select_entry(&mut self, index: usize) {
        if let Some(entry) = NAV_ENTRIES.get(index).copied() {
            self.selected = index;
            self.route = entry.route;
            self.focus = Focus::Sidebar;
            self.notice = Some(format!("当前页面：{}", entry.label));
        }
    }

    fn select_route(&mut self, route: Route) {
        self.route = route;
        self.selected = NAV_ENTRIES
            .iter()
            .position(|item| item.route == route)
            .unwrap_or(0);
        self.notice = Some(format!("当前页面：{}", route.title()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};

    #[test]
    fn sidebar_keyboard_visits_every_entry() {
        let mut app = App::default();
        let mut visited = vec![app.selected];
        for _ in 0..(NAV_ENTRIES.len() - 1) {
            app.dispatch(Action::Key(KeyCode::Down, KeyModifiers::NONE));
            visited.push(app.selected);
        }
        assert_eq!(visited, (0..NAV_ENTRIES.len()).collect::<Vec<_>>());
    }

    #[test]
    fn mouse_click_selects_exact_sidebar_entry() {
        let mut app = App::default();
        let area = Rect::new(0, 0, 24, 20);
        app.register_hit(Rect::new(1, 8, 22, 1), HitTarget::Sidebar(4));
        app.dispatch(Action::Mouse(MouseEventKind::Down(MouseButton::Left), 4, 8));
        assert_eq!(app.selected, 4);
        assert_eq!(app.route, Route::Search);
        assert_eq!(app.focus, Focus::Sidebar);
        let _ = area;
    }

    #[test]
    fn mouse_click_selects_main_row_without_opening_it() {
        let mut app = App::default();
        app.route = Route::Shelf;
        app.register_hit(Rect::new(25, 4, 70, 1), HitTarget::MainRow(3));
        app.dispatch(Action::Mouse(
            MouseEventKind::Down(MouseButton::Left),
            30,
            4,
        ));
        assert_eq!(app.main_selected, 3);
        assert_eq!(app.route, Route::Shelf);
        app.dispatch(Action::Key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.route, Route::Reader);
    }
}
