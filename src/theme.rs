//! 设计系统 —— 对齐 React 范例的配色 / 边框 / 通用绘制。
//!
//! 写新页面时优先用这里的 helper，不要直接 `Style::default().fg(Color::Yellow)`，
//! 那是「效果差」的最大来源。

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};
use unicode_width::UnicodeWidthStr;

// ─────────────────────────────────────────────────────────────
//  调色板（与 src/index.css 的 --color-* 一一对应）
// ─────────────────────────────────────────────────────────────
pub struct Theme {
    pub bg: Color,           // #141414  页面底
    pub bg2: Color,          // #1b1b1b  次级底
    pub bg3: Color,          // #242424  选中底
    pub border: Color,       // #3a3a3a  普通边框
    pub border_focus: Color, // #5c5c5c  焦点边框
    pub fg: Color,           // #d4d4d4  正文
    pub hi: Color,           // #f2f2f2  高亮 / 标题
    pub dim: Color,          // #6e6e6e  次要
    pub mute: Color,         // #8f8f8f  更次要
    pub accent: Color,       // #e3a35a  琥珀色强调
    pub ok: Color,           // #8cc08a
    pub err: Color,          // #e07a7a
    pub info: Color,         // #7fa9d9
    pub mag: Color,          // #c49ae0
}

pub const THEME: Theme = Theme {
    bg: Color::Rgb(0x14, 0x14, 0x14),
    bg2: Color::Rgb(0x1b, 0x1b, 0x1b),
    bg3: Color::Rgb(0x24, 0x24, 0x24),
    border: Color::Rgb(0x3a, 0x3a, 0x3a),
    border_focus: Color::Rgb(0x5c, 0x5c, 0x5c),
    fg: Color::Rgb(0xd4, 0xd4, 0xd4),
    hi: Color::Rgb(0xf2, 0xf2, 0xf2),
    dim: Color::Rgb(0x6e, 0x6e, 0x6e),
    mute: Color::Rgb(0x8f, 0x8f, 0x8f),
    accent: Color::Rgb(0xe3, 0xa3, 0x5a),
    ok: Color::Rgb(0x8c, 0xc0, 0x8a),
    err: Color::Rgb(0xe0, 0x7a, 0x7a),
    info: Color::Rgb(0x7f, 0xa9, 0xd9),
    mag: Color::Rgb(0xc4, 0x9a, 0xe0),
};

// ── 快捷 Style 构造 ──────────────────────────────────────────
pub fn s(fg: Color) -> Style {
    Style::default().fg(fg)
}
pub fn sb(fg: Color) -> Style {
    Style::default().fg(fg).add_modifier(Modifier::BOLD)
}
pub fn sbg(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg)
}

/// 给 span 强制铺上 bg，用来「嵌」进边框线
fn nest(sp: Span<'_>) -> Span<'_> {
    let bg = sp.style.bg.unwrap_or(THEME.bg);
    Span::styled(sp.content, sp.style.bg(bg))
}

fn nest_line<'a>(mut spans: Vec<Span<'a>>) -> Line<'a> {
    // 左右各垫一个空格，视觉上标题不贴边框圆角
    let mut out = Vec::with_capacity(spans.len() + 2);
    out.push(Span::styled(" ", sbg(THEME.bg, THEME.bg)));
    out.append(&mut spans);
    out.push(Span::styled(" ", sbg(THEME.bg, THEME.bg)));
    Line::from(out.into_iter().map(nest).collect::<Vec<_>>())
}

// ─────────────────────────────────────────────────────────────
//  圆角边框面板（对应 React 的 <Box>）
//
//  关键细节：
//  1. BorderType::Rounded  → ╭╮╰╯ 而不是 ┌┐└┘
//  2. title 带 bg(THEME.bg) → 盖住边框线，标题「嵌」在框上
//  3. focused 时边框用 border_focus，更亮一档
//  4. 用 title_top / title_bottom（ratatui 0.29+）
// ─────────────────────────────────────────────────────────────
pub fn panel<'a>(
    title_left: Vec<Span<'a>>,
    title_right: Option<Span<'a>>,
    focused: bool,
) -> Block<'a> {
    panel_full(title_left, title_right, None, focused)
}

pub fn panel_full<'a>(
    title_left: Vec<Span<'a>>,
    title_right: Option<Span<'a>>,
    footer: Option<Span<'a>>,
    focused: bool,
) -> Block<'a> {
    let border = if focused {
        THEME.border_focus
    } else {
        THEME.border
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(s(border))
        .style(sbg(THEME.fg, THEME.bg)) // 面板内部也铺底色
        .title_top(nest_line(title_left));

    if let Some(right) = title_right {
        block = block.title_top(nest_line(vec![right]).right_aligned());
    }
    if let Some(f) = footer {
        block = block.title_bottom(nest_line(vec![f]).right_aligned());
    }
    block
}

// ─────────────────────────────────────────────────────────────
//  列表行（对应 React 的 <Row>）
//  选中 + 焦点 → bg3 + › 琥珀色
//  选中 + 失焦 → bg2 + › 暗色
//  未选中     → 透明 › + 默认底
// ─────────────────────────────────────────────────────────────
pub fn row_style(selected: bool, focused: bool) -> Style {
    if selected && focused {
        sbg(THEME.hi, THEME.bg3)
    } else if selected {
        sbg(THEME.hi, THEME.bg2)
    } else {
        sbg(THEME.fg, THEME.bg)
    }
}

pub fn caret(selected: bool, focused: bool) -> Span<'static> {
    if selected && focused {
        Span::styled("› ", sb(THEME.accent))
    } else if selected {
        Span::styled("› ", s(THEME.dim))
    } else {
        Span::styled("  ", s(THEME.bg))
    }
}

/// 组装一行：caret + 内容 spans
pub fn row_line(selected: bool, focused: bool, content: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![caret(selected, focused)];
    let base = row_style(selected, focused);
    for sp in content {
        let st = sp.style.bg(base.bg.unwrap_or(THEME.bg));
        let st = if sp.style.fg.is_none() {
            st.fg(base.fg.unwrap_or(THEME.fg))
        } else {
            st
        };
        spans.push(Span::styled(sp.content, st));
    }
    Line::from(spans).style(base)
}

// ─────────────────────────────────────────────────────────────
//  进度条  ━━━━━━━───────
// ─────────────────────────────────────────────────────────────
pub fn bar_spans(value: f64, width: usize) -> Vec<Span<'static>> {
    let v = value.clamp(0.0, 1.0);
    let n = ((v * width as f64).round() as usize).min(width);
    vec![
        Span::styled("━".repeat(n), s(THEME.accent)),
        Span::styled("━".repeat(width - n), s(THEME.border)),
    ]
}

// ─────────────────────────────────────────────────────────────
//  文本截断 / pad（按显示列宽，不是 char count！）
// ─────────────────────────────────────────────────────────────
pub fn truncate(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if s.width() <= max_cols {
        return s.to_string();
    }
    if max_cols <= 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_cols - 1 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

pub fn pad(s: &str, cols: usize) -> String {
    let w = s.width();
    if w >= cols {
        truncate(s, cols)
    } else {
        format!("{s}{}", " ".repeat(cols - w))
    }
}

// ─────────────────────────────────────────────────────────────
//  列表滚动：保证 selected 落在视口内
// ─────────────────────────────────────────────────────────────
pub fn list_scroll(selected: usize, height: usize, total: usize) -> usize {
    if height == 0 || total == 0 {
        return 0;
    }
    if selected < height / 3 {
        0
    } else if selected + height / 3 >= total {
        total.saturating_sub(height)
    } else {
        selected.saturating_sub(height / 3)
    }
}

// ─────────────────────────────────────────────────────────────
//  快捷键提示行
// ─────────────────────────────────────────────────────────────
pub fn hints(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, label)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", s(THEME.dim)));
        }
        spans.push(Span::styled((*k).to_string(), s(THEME.mute)));
        spans.push(Span::styled(format!(" {label}"), s(THEME.dim)));
    }
    Line::from(spans)
}

pub fn center_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

pub fn fill_bg(area: Rect, buf: &mut Buffer) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(THEME.bg);
            }
        }
    }
}

#[allow(dead_code)]
pub fn clear(area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
}

pub const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
