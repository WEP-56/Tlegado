use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, Focus, HitTarget, Route, NAV_ENTRIES};

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.begin_frame();
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

fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = vec![
        nav_item(app, 0, "首页", ""),
        section_item("▾ 书架"),
        nav_item(app, 1, "全部书籍", "10"),
        nav_item(app, 2, "本地书库", "4"),
        nav_item(app, 3, "网络书库", "+31"),
        section_item("▾ 发现"),
        nav_item(app, 4, "搜索书籍", "/"),
        nav_item(app, 5, "起点中文网", "7类"),
        nav_item(app, 6, "番茄小说", "5类"),
        nav_item(app, 7, "笔趣阁①", "6类"),
        section_item("▾ 设置"),
        nav_item(app, 8, "阅读历史", "8"),
        nav_item(app, 9, "书源管理", "6/8"),
        nav_item(app, 10, "阅读偏好", ""),
    ];
    for (row, _) in NAV_ENTRIES.iter().enumerate() {
        let offset = match row {
            0 => 0,
            1..=3 => 1,
            4..=7 => 2,
            _ => 3,
        };
        app.register_hit(
            Rect::new(
                area.x + 1,
                area.y + 1 + (row + offset) as u16,
                area.width.saturating_sub(2),
                1,
            ),
            HitTarget::Sidebar(row),
        );
    }
    let border = if app.focus == Focus::Sidebar {
        Color::Rgb(227, 163, 90)
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

fn nav_item(app: &App, index: usize, label: &str, count: &str) -> ListItem<'static> {
    let selected = index == app.selected;
    let marker = if selected { "› " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker}{label}"), style),
        Span::styled(format!("{count:>5}"), Style::default().fg(Color::DarkGray)),
    ]))
}

fn section_item(label: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        label.to_string(),
        Style::default().fg(Color::DarkGray),
    )))
}

fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .title(format!(" {} ", app.route.title()))
        .border_style(Style::default().fg(if app.focus == Focus::Main {
            Color::Rgb(227, 163, 90)
        } else {
            Color::DarkGray
        }));
    match app.route {
        Route::Home => draw_home(frame, app, area, block),
        Route::Shelf => draw_shelf(frame, app, area, block),
        Route::Search => draw_search(frame, app, area, block),
        Route::History => draw_table_page(
            frame,
            app,
            area,
            block,
            "阅读历史",
            &["诡秘之主", "道诡异仙", "大奉打更人"],
        ),
        Route::Sources => draw_sources(frame, app, area, block),
        Route::Preferences => draw_preferences(frame, app, area, block),
        Route::Reader => draw_reader(frame, app, area, block),
    }
}

fn draw_reader(frame: &mut Frame, app: &App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(block.title(" 诡秘之主  ·  爱潜水的乌贼 "), area);
    let paragraphs = [
        "第813章 雾中来客",
        "",
        "他忽然明白，自己并不是偶然走到这里的。每一次犹豫，每一次转身，都早已写在那本书的某一页上。",
        "",
        "他站在书店门口，指尖还残留着纸页的触感。那本没有署名的旧书，此刻正安静地躺在他的外套口袋里。",
        "",
        "他想起多年前那个下午，老人坐在藤椅上对他说过的话：真正的故事，从来不是写给所有人看的。",
        "",
        "他忽然明白，自己并不是偶然走到这里的。每一次犹豫，每一次转身，都早已写在那本书的某一页上。",
    ];
    let mut lines = Vec::new();
    for (i, paragraph) in paragraphs.iter().enumerate() {
        let style = if i == 0 {
            Style::default()
                .fg(Color::Rgb(227, 163, 90))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(*paragraph, style)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().padding(ratatui::widgets::Padding::new(2, 2, 1, 2))),
        inner,
    );
    let footer = Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2);
    let progress = format!(
        "第813章  雾中来客                                      1/2 页   ━━━━━  56.7%   12:52"
    );
    frame.render_widget(
        Paragraph::new(progress).style(Style::default().fg(Color::DarkGray)),
        footer,
    );
    let _ = app;
}

fn draw_home(frame: &mut Frame, app: &App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "Tlegado",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  0.1.0-alpha", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(
            "基于 legado（阅读 3.0）书源规则的终端阅读器。书源 6/8 可用，书架 31 章更新待读。",
        ),
        Line::from(vec![
            Span::styled(
                "[继续阅读 诡秘之主 · 第813章 雾中来客]",
                Style::default().fg(Color::Rgb(227, 163, 90)),
            ),
            Span::styled("  or press c", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "› 打开书架                                      b",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  发现 · 按书源浏览                              e"),
        Line::from("  搜索书籍                                      /"),
        Line::from("  导入本地书籍                                    o"),
        Line::from("  书源管理                                      s"),
        Line::from("  快捷键帮助                                    ?"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().padding(ratatui::widgets::Padding::new(4, 2, 1, 2))),
        inner,
    );
    let stats = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3); 3])
        .split(Rect::new(inner.x, inner.y + 15, inner.width, 4));
    for (rect, (label, value)) in stats.iter().zip([
        ("今日阅读", "2小时05分"),
        ("本周章节", "146 章"),
        ("缓存占用", "38.2 MB"),
    ]) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(label, Style::default().fg(Color::DarkGray))),
                Line::from(Span::styled(
                    value,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
            ]),
            *rect,
        );
    }
    let _ = app;
}

fn draw_shelf(frame: &mut Frame, app: &mut App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let header = Row::new(["书名", "作者", "进度", "最新章节", "来源"])
        .style(Style::default().fg(Color::DarkGray))
        .bottom_margin(1);
    let books = [
        (
            "诡秘之主",
            "爱潜水的乌贼",
            "57%",
            "第1432章 新的征程",
            "起点中文网",
        ),
        (
            "道诡异仙 +6",
            "狐尾的笔",
            "100%",
            "第1068章 心素",
            "起点中文网",
        ),
        (
            "大奉打更人",
            "卖报小郎君",
            "28%",
            "第1284章 大结局",
            "番茄小说",
        ),
        (
            "深空彼岸 +22",
            "辰东",
            "99%",
            "第1520章 彼岸花开",
            "笔趣阁①",
        ),
        (
            "凡人修仙传",
            "忘语",
            "100%",
            "第2446章 飞升仙界",
            "纵横中文网",
        ),
        (
            "我在精神病院学斩神 +3",
            "三九音域",
            "67%",
            "第1356章 天庭",
            "番茄小说",
        ),
        ("三体", "刘慈欣", "37%", "第104章 尾声", "本地·EPUB"),
        ("活着", "余华", "100%", "第12章 老人与牛", "本地·TXT"),
    ];
    for (index, _) in books.iter().enumerate() {
        app.register_hit(
            Rect::new(inner.x, inner.y + 2 + index as u16, inner.width, 1),
            HitTarget::MainRow(index),
        );
    }
    let rows = books.iter().enumerate().map(|(i, b)| {
        Row::new([b.0, b.1, b.2, b.3, b.4]).style(if i == app.main_selected % books.len() {
            Style::default().bg(Color::Rgb(38, 38, 38)).fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        })
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(25),
                Constraint::Length(17),
                Constraint::Length(10),
                Constraint::Length(27),
                Constraint::Min(15),
            ],
        )
        .header(header)
        .column_spacing(1),
        inner,
    );
}

fn draw_search(frame: &mut Frame, _app: &App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "› 剑来",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "                                      enter 搜索",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "● SearchBook(key=\"剑来\", sources=6)",
            Style::default().fg(Color::Gray),
        )),
        Line::from("  ├ 起点中文网       ✓ 1 条 212ms"),
        Line::from("  ├ 番茄小说         ✓ 1 条 188ms"),
        Line::from("  ├ 笔趣阁①         ✓ 1 条 540ms"),
        Line::from("  ├ 纵横中文网       ✓ 无结果 301ms"),
        Line::from(""),
        Line::from("› 剑来             烽火戏诸侯       起点中文网"),
        Line::from("  剑来             烽火戏诸侯       番茄小说"),
        Line::from("  剑来             烽火戏诸侯       笔趣阁①"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().padding(ratatui::widgets::Padding::new(2, 1, 1, 1))),
        inner,
    );
}

fn draw_table_page(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    block: Block<'static>,
    title: &str,
    values: &[&str],
) {
    let inner = block.inner(area);
    frame.render_widget(block.title(format!(" {title} / 最近阅读 ")), area);
    let rows = values.iter().enumerate().map(|(i, v)| {
        Row::new([*v, "作者", "第 812 章", "10分钟前"]).style(
            if i == app.main_selected % values.len() {
                Style::default().bg(Color::Rgb(38, 38, 38)).fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            },
        )
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(24),
                Constraint::Length(18),
                Constraint::Length(16),
                Constraint::Min(12),
            ],
        )
        .header(
            Row::new(["书名", "作者", "章节", "时间"]).style(Style::default().fg(Color::DarkGray)),
        ),
        inner,
    );
}

fn draw_sources(frame: &mut Frame, app: &App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(block.title(" 书源管理 / 6/8 可用 "), area);
    let rows = [
        ("[✓]", "起点中文网", "正版", "212ms", "✓"),
        ("[✓]", "番茄小说", "正版", "188ms", "✓"),
        ("[✓]", "笔趣阁①", "聚合", "540ms", "✓"),
        ("[✓]", "纵横中文网", "正版", "301ms", "✓"),
        ("[ ]", "69书吧", "聚合", "690ms", "✓"),
        ("[ ]", "书海阁", "聚合", "超时", "-"),
    ];
    let items = rows.iter().enumerate().map(|(i, r)| {
        Row::new([r.0, r.1, r.2, r.3, r.4]).style(if i == app.main_selected % rows.len() {
            Style::default().bg(Color::Rgb(38, 38, 38)).fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        })
    });
    frame.render_widget(
        Table::new(
            items,
            [
                Constraint::Length(6),
                Constraint::Length(20),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(
            Row::new(["启用", "书源名", "分组", "响应", "发现"])
                .style(Style::default().fg(Color::DarkGray)),
        ),
        inner,
    );
}

fn draw_preferences(frame: &mut Frame, app: &App, area: Rect, block: Block<'static>) {
    let inner = block.inner(area);
    frame.render_widget(
        block.title(" 阅读偏好 / ~/.config/tlegado/config.toml "),
        area,
    );
    let rows = [
        ("配色主题", "<终端默认>  护眼绿  羊皮纸  高对比"),
        ("行宽", "<自适应> 28 36 44"),
        ("行距", "<1.0> 1.5 2.0"),
        ("段首缩进", "<0> 2"),
        ("分页方式", "<整页> 滚动"),
        ("自动翻页", "<关> 5s 10s 20s"),
        ("底部进度条", "<开> 关"),
        ("启用净化规则", "<开> 关"),
        ("简繁转换", "<关闭> 简→繁 繁→简"),
    ];
    let items = rows.iter().enumerate().map(|(i, r)| {
        Row::new([r.0, r.1, ""]).style(if i == app.main_selected % rows.len() {
            Style::default().bg(Color::Rgb(38, 38, 38)).fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        })
    });
    frame.render_widget(
        Table::new(
            items,
            [
                Constraint::Length(18),
                Constraint::Length(35),
                Constraint::Min(10),
            ],
        )
        .header(Row::new(["", "", ""]).style(Style::default().fg(Color::DarkGray))),
        inner,
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
