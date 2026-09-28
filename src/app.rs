use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;

use crate::event::{is_left_click, Event};

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
        if is_left_click(kind)
            && self.sidebar_visible
            && self.sidebar_area.contains((column, row).into())
        {
            let row_offset = row.saturating_sub(self.sidebar_area.y + 1) as usize;
            let route = match row_offset {
                0 => Some(Route::Home),
                2..=4 => Some(Route::Shelf),
                6..=9 => Some(Route::Search),
                11 => Some(Route::History),
                12 => Some(Route::Sources),
                13 => Some(Route::Preferences),
                _ => None,
            };
            if let Some(route) = route {
                self.select_route(route);
                self.focus = Focus::Sidebar;
            }
        } else if matches!(kind, MouseEventKind::ScrollDown) {
            self.notice = Some("向下滚动".to_string());
        } else if matches!(kind, MouseEventKind::ScrollUp) {
            self.notice = Some("向上滚动".to_string());
        }
    }

    fn move_sidebar(&mut self, delta: i32) {
        let max = Route::ALL.len().saturating_sub(1) as i32;
        self.selected = (self.selected as i32 + delta).clamp(0, max) as usize;
        self.route = Route::ALL[self.selected];
    }

    fn select_route(&mut self, route: Route) {
        self.route = route;
        self.selected = Route::ALL
            .iter()
            .position(|item| *item == route)
            .unwrap_or(0);
        self.notice = Some(format!("当前页面：{}", route.title()));
    }
}
