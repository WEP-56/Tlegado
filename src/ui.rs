use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Route};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("~/.tlegado", Style::default().fg(Color::DarkGray)),
            Span::raw("  ›  "),
            Span::styled(
                app.route.title(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        outer[0],
    );

    if app.sidebar_visible {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(0)])
            .split(outer[1]);
        app.set_sidebar_area(chunks[0]);
        draw_sidebar(frame, app, chunks[0]);
        draw_main(frame, app, chunks[1]);
    } else {
        draw_main(frame, app, outer[1]);
    }

    let status = app
        .notice
        .as_deref()
        .unwrap_or("tab 切换焦点 · j/k 移动 · ? 帮助 · q 退出");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(status, Style::default().fg(Color::Gray)),
            Span::raw("  "),
            Span::styled("[alpha]", Style::default().fg(Color::DarkGray)),
        ])),
        outer[2],
    );

    if app.help_visible {
        draw_help(frame, area);
    }
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let items = Route::ALL.iter().enumerate().map(|(index, route)| {
        let marker = if index == app.selected { "› " } else { "  " };
        let style = if index == app.selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        ListItem::new(Line::from(Span::styled(
            format!("{marker}{}", route.title()),
            style,
        )))
    });
    let border = if app.focus == Focus::Sidebar {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        List::new(items).block(
            Block::bordered()
                .title(" Tlegado ")
                .border_style(Style::default().fg(border)),
        ),
        area,
    );
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(format!(" {} ", app.route.title()))
        .border_style(Style::default().fg(if app.focus == Focus::Main {
            Color::Cyan
        } else {
            Color::DarkGray
        }));
    let body = match app.route {
        Route::Home => vec![
            "欢迎使用 Tlegado",
            "",
            "这是 Rust + Ratatui 终端版 Legado 的交互骨架。",
            "先完善 TUI，再接入真实书源与阅读服务。",
            "",
            "按 Enter 查看当前页面，按 ? 查看帮助。",
        ],
        Route::Shelf => vec![
            "书架暂使用演示状态",
            "",
            "暂无演示书籍。后续接入书籍 service。",
        ],
        Route::Search => vec![
            "搜索暂使用演示状态",
            "",
            "按 Enter 将在后续版本打开搜索输入。",
        ],
        Route::History => vec!["阅读历史暂使用演示状态", "", "暂无阅读记录。"],
        Route::Sources => vec![
            "书源管理暂使用演示状态",
            "",
            "这里将接入书源导入、启停和测试。",
        ],
        Route::Preferences => vec![
            "阅读偏好暂使用演示状态",
            "",
            "这里将调整主题、字号、行距和分页。",
        ],
    };
    let text = Text::from(body.into_iter().map(Line::from).collect::<Vec<_>>());
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(58);
    let height = 13.min(area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect::new(x, y, width, height);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            "快捷键",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("Tab       切换侧栏 / 主体焦点"),
        Line::from("j / k     移动选中项"),
        Line::from("Enter     打开当前项"),
        Line::from("/         打开搜索"),
        Line::from("Ctrl+B    显示 / 隐藏侧栏"),
        Line::from("Esc / q   退出或关闭弹层"),
        Line::from("?         关闭帮助"),
        Line::from(""),
        Line::from("鼠标：点击侧栏项目，滚轮查看状态"),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" 帮助 ")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        popup,
    );
}
