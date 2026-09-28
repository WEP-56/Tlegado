//! 阅读排版与章节导航；真实正文与显式演示模式共享渲染。
use crate::data::Book;
use crate::theme::{list_scroll, panel, row_line, s, sb, truncate, THEME};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthChar;

pub struct Reader {
    pub request: Option<(u32, bool)>,
    real: Option<RealText>,
    pub options: crate::demo::Prefs,
    pub rules: Vec<crate::demo::Rule>,
    pub book: Book,
    pub chapter: u32,
    pub selected: u32,
    pub offset: usize,
    pub previous_sidebar_hidden: bool,
    lines: Vec<String>,
    width: u16,
    height: usize,
}

struct RealText {
    titles: Vec<String>,
    text: String,
    positions: Vec<usize>,
    restore: Option<usize>,
}

pub fn chapter_title(index: u32) -> String {
    let names = [
        "雾中来客",
        "旧城钟声",
        "灯下残卷",
        "第二封信",
        "长夜将尽",
        "归途",
        "雨夜",
        "晨钟",
    ];
    format!("第{}章 {}", index + 1, names[index as usize % names.len()])
}

impl Reader {
    pub fn new(book: Book, previous_sidebar_hidden: bool) -> Self {
        let chapter = book.read.min(book.total.saturating_sub(1));
        Self {
            request: None,
            real: None,
            options: crate::demo::Prefs::default(),
            rules: Vec::new(),
            book,
            chapter,
            selected: chapter,
            offset: 0,
            previous_sidebar_hidden,
            lines: Vec::new(),
            width: 0,
            height: 1,
        }
    }

    pub fn select(&mut self, delta: i32) {
        self.selected = (i64::from(self.selected) + i64::from(delta))
            .clamp(0, i64::from(self.book.total.saturating_sub(1))) as u32;
    }

    pub fn open_selected(&mut self) {
        if self.real.is_some() {
            self.request = Some((self.selected, false));
            return;
        }
        let width = self.width;
        self.chapter = self.selected;
        self.offset = 0;
        self.width = 0;
        // Rebuild immediately so consecutive inputs use the new chapter's bounds.
        self.prepare(width, self.height as u16);
    }

    pub fn change_chapter(&mut self, delta: i32) {
        self.selected = self.chapter;
        self.select(delta);
        if self.selected != self.chapter {
            self.open_selected();
        }
    }

    pub fn scroll(&mut self, delta: isize) {
        if self.width == 0 || self.lines.is_empty() {
            return;
        }
        // Show the final partial page before a subsequent input crosses chapters.
        if delta > 0 && self.offset == self.max_offset() {
            self.change_chapter(1);
            return;
        }
        if delta < 0 && self.offset == 0 && self.chapter > 0 {
            if self.real.is_some() {
                self.request = Some((self.chapter - 1, true));
                return;
            }
            self.change_chapter(-1);
            self.offset = self.max_offset();
            return;
        }
        self.offset = self
            .offset
            .saturating_add_signed(delta)
            .min(self.max_offset());
    }

    pub fn page(&mut self, forward: bool) {
        let step = if self.options.0[4] == 1 {
            1
        } else {
            self.height.max(1) as isize
        };
        self.scroll(if forward { step } else { -step });
    }

    pub fn max_offset(&self) -> usize {
        self.lines.len().saturating_sub(self.height)
    }

    pub fn is_real(&self) -> bool {
        self.real.is_some()
    }

    pub fn title(&self, index: u32) -> String {
        self.real
            .as_ref()
            .and_then(|r| r.titles.get(index as usize))
            .cloned()
            .unwrap_or_else(|| chapter_title(index))
    }

    pub fn set_real(&mut self, titles: Vec<String>, text: String, position: usize) {
        self.real = Some(RealText {
            titles,
            text,
            positions: vec![],
            restore: Some(position),
        });
        self.width = 0;
    }

    pub fn set_chapter(&mut self, index: u32, text: String, end: bool) {
        if let Some(real) = &mut self.real {
            real.restore = Some(if end { text.chars().count() } else { 0 });
            real.text = text;
            real.positions.clear();
        }
        self.chapter = index;
        self.selected = index;
        self.offset = 0;
        self.width = 0;
        self.lines.clear();
    }

    /// Tlegado stores a Unicode scalar offset here, never a terminal row number.
    pub fn position(&self) -> usize {
        self.real
            .as_ref()
            .map(|r| {
                r.restore
                    .unwrap_or_else(|| r.positions.get(self.offset).copied().unwrap_or(0))
            })
            .unwrap_or(0)
    }

    fn prepare(&mut self, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        let position = self.position();
        if let Some(real) = &mut self.real {
            self.height = height as usize;
            if self.width != width {
                self.lines.clear();
                real.positions.clear();
                let indent = if self.options.0[3] == 1 { "　　" } else { "" };
                let mut base = 0;
                for paragraph in real.text.split('\n') {
                    let decorated = format!("{indent}{paragraph}");
                    let wrapped = wrap(&decorated, width as usize);
                    let mut used: usize = 0;
                    for line in wrapped {
                        real.positions.push(
                            base + used
                                .saturating_sub(indent.chars().count())
                                .min(paragraph.chars().count()),
                        );
                        used += line.chars().count();
                        self.lines.push(line);
                        if self.options.0[2] == 1 {
                            real.positions.push(
                                base + used
                                    .saturating_sub(indent.chars().count())
                                    .min(paragraph.chars().count()),
                            );
                            self.lines.push(String::new());
                        }
                    }
                    real.positions.push(base + paragraph.chars().count());
                    self.lines.push(String::new());
                    base += paragraph.chars().count() + 1;
                }
                self.offset = real
                    .positions
                    .partition_point(|p| *p <= position)
                    .saturating_sub(1);
                real.restore = None;
                self.width = width;
            }
            self.offset = self.offset.min(self.max_offset());
            return;
        }
        if self.width != width {
            self.lines.clear();
            let intro = format!(
                "《{}》 · {}。以下为阅读排版预览，章节名与正文均为演示内容。",
                self.book.title,
                chapter_title(self.chapter)
            );
            for paragraph in std::iter::once(intro.as_str()).chain(
                PARAGRAPHS
                    .iter()
                    .copied()
                    .cycle()
                    .skip(self.chapter as usize % PARAGRAPHS.len())
                    .take(18),
            ) {
                let paragraph = if self.options.0[6] == 0 {
                    crate::demo::purify(paragraph, &self.rules)
                } else {
                    paragraph.to_string()
                };
                let indent = if self.options.0[3] == 1 { "　　" } else { "" };
                for line in wrap(&format!("{indent}{paragraph}"), width as usize) {
                    self.lines.push(line);
                    if self.options.0[2] == 1 {
                        self.lines.push(String::new());
                    }
                }
                self.lines.push(String::new());
            }
            self.width = width;
        }
        self.height = height as usize;
        self.offset = self.offset.min(self.max_offset());
    }
}

const PARAGRAPHS: [&str; 6] = [
    "雨停在黄昏之前。街角的书店还亮着灯，玻璃窗上倒映着缓缓走过的人影。他推开门，听见风铃在头顶轻轻响了一声。",
    "柜台后放着一本没有署名的旧书。纸页边缘微微卷起，像是经历过许多漫长的旅程。他伸出手，却发现书里夹着一封尚未拆开的信。",
    "信封上只写着一个地点：旧城钟楼。墨色已经淡了，最后一笔却依然清晰。他望向窗外，远处的钟声恰好穿过暮色，落在安静的街道上。",
    "“你可以明天再去。”店主说着，将一盏小灯放到桌边。他没有立刻回答，只把信收进口袋。灯光照见纸上细密的纹路，也照见他犹豫的神情。",
    "走出书店时，积水映着初升的月亮。城市仿佛翻过了一页，白日的喧闹渐渐退去。只有脚步声沿着石板路向前，一直通往钟楼下那扇半开的门。",
    "门后没有预想中的黑暗。一张木桌、一杯温茶，还有另一封信，静静等待着来人。他忽然觉得，这段旅程也许才刚刚开始。（演示站点水印）",
];

#[cfg(test)]
mod real_tests {
    use super::*;
    fn reader() -> Reader {
        let mut book = crate::data::shelf().remove(0);
        book.read = 0;
        book.total = 2;
        let mut reader = Reader::new(book, false);
        reader.set_real(
            vec!["真实第一章".into(), "真实第二章".into()],
            "中文段落 English 内容。".repeat(150),
            0,
        );
        reader.prepare(44, 12);
        reader
    }
    #[test]
    fn character_progress_restores_after_width_change() {
        let mut original = reader();
        original.page(true);
        let position = original.position();
        assert!(
            position > 12,
            "position must be characters, not screen rows"
        );
        let mut reopened = reader();
        reopened.set_real(
            vec!["真实第一章".into(), "真实第二章".into()],
            "中文段落 English 内容。".repeat(150),
            position,
        );
        assert_eq!(
            reopened.position(),
            position,
            "unrendered progress must not reset to zero"
        );
        reopened.prepare(28, 12);
        assert!(reopened.position() <= position);
        assert!(position - reopened.position() < 30);
    }
    #[test]
    fn real_chapter_changes_wait_for_successful_load() {
        let mut reader = reader();
        reader.offset = reader.max_offset();
        reader.page(true);
        assert_eq!(reader.request, Some((1, false)));
        assert_eq!(reader.chapter, 0);
        reader.set_chapter(1, "第二章实际内容".repeat(100), false);
        reader.prepare(44, 12);
        assert_eq!(reader.chapter, 1);
        assert!(reader.lines.join("").contains("第二章实际内容"));
        reader.page(false);
        assert_eq!(reader.request, Some((0, true)));
        assert_eq!(reader.chapter, 1);
    }
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let size = ch.width().unwrap_or(0);
        if used + size > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push(ch);
        used += size;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn draw_chapters(f: &mut Frame, reader: &Reader, area: Rect, focused: bool) {
    let block = panel(
        vec![Span::styled("章节目录", sb(THEME.hi))],
        Some(Span::styled(
            format!("{}章", reader.book.total),
            s(THEME.dim),
        )),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(truncate(&reader.book.title, inner.width as usize)).style(s(THEME.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let height = inner.height.saturating_sub(2);
    let start = list_scroll(
        reader.selected as usize,
        height as usize,
        reader.book.total as usize,
    );
    for (row, index) in (start..reader.book.total as usize)
        .take(height as usize)
        .enumerate()
    {
        let current = index as u32 == reader.chapter;
        let title = format!(
            "{}{}",
            if current { "● " } else { "  " },
            reader.title(index as u32)
        );
        let line = row_line(
            index as u32 == reader.selected,
            focused,
            vec![Span::styled(
                truncate(&title, inner.width.saturating_sub(2) as usize),
                if current {
                    s(THEME.accent)
                } else {
                    s(THEME.fg)
                },
            )],
        );
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + 2 + row as u16, inner.width, 1),
        );
    }
}

pub fn draw_page(f: &mut Frame, reader: &mut Reader, area: Rect, focused: bool) {
    let block = panel(
        vec![
            Span::styled(&reader.book.title, sb(THEME.hi)),
            Span::styled(format!(" · {}", reader.book.author), s(THEME.dim)),
        ],
        Some(Span::styled(
            if reader.is_real() {
                "正文"
            } else {
                "阅读预览"
            },
            s(THEME.accent),
        )),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 5 {
        return;
    }
    let limit = match reader.options.0[1] {
        1 => 56,
        2 => 72,
        3 => 88,
        _ => u16::MAX,
    };
    let width = inner.width.saturating_sub(4).min(limit);
    let x = inner.x + (inner.width - width) / 2;
    let content = Rect::new(x, inner.y + 3, width, inner.height - 5);
    reader.prepare(content.width, content.height);
    f.render_widget(
        Paragraph::new(reader.title(reader.chapter))
            .style(sb(THEME.hi))
            .centered(),
        Rect::new(x, inner.y + 1, width, 1),
    );
    let lines: Vec<Line> = reader
        .lines
        .iter()
        .skip(reader.offset)
        .take(content.height as usize)
        .map(|line| Line::from(line.as_str()))
        .collect();
    use ratatui::style::{Color, Style};
    let (bg, fg) = match reader.options.0[0] {
        1 => (Color::Rgb(20, 32, 25), Color::Rgb(188, 217, 180)),
        2 => (Color::Rgb(232, 220, 194), Color::Rgb(58, 47, 34)),
        3 => (Color::Black, Color::White),
        _ => (THEME.bg, THEME.fg),
    };
    f.render_widget(
        Paragraph::new(lines).style(Style::default().fg(fg).bg(bg)),
        content,
    );
    if reader.options.0[5] == 1 {
        return;
    }
    let page = (reader.offset + reader.height).div_ceil(reader.height);
    let pages = reader.lines.len().div_ceil(reader.height).max(1);
    let status = format!(
        "第 {}/{} 章 · {}/{} 页 · {}",
        reader.chapter + 1,
        reader.book.total,
        page,
        pages,
        if reader.is_real() {
            "进度自动保存"
        } else {
            "演示正文"
        }
    );
    f.render_widget(
        Paragraph::new(status).style(s(THEME.dim)).centered(),
        Rect::new(x, inner.bottom() - 1, width, 1),
    );
}

#[cfg(test)]
mod paging_tests {
    use super::*;

    fn reader() -> Reader {
        let mut book = crate::data::shelf().remove(0);
        book.read = 1;
        book.total = 3;
        let mut reader = Reader::new(book, false);
        reader.prepare(44, 12);
        reader
    }

    #[test]
    fn final_partial_page_is_visible_before_next_chapter() {
        let mut reader = reader();
        reader.offset = reader.max_offset() - 1;
        reader.page(true);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.offset, reader.max_offset());
        reader.page(true);
        assert_eq!(reader.chapter, 2);
        assert_eq!(reader.selected, 2);
        assert_eq!(reader.offset, 0);
        assert!(reader.lines[0..4].join("").contains(&chapter_title(2)));
    }

    #[test]
    fn backward_paging_lands_on_previous_chapters_last_page() {
        let mut reader = reader();
        reader.page(false);
        assert_eq!(reader.chapter, 0);
        assert_eq!(reader.selected, 0);
        assert_eq!(reader.offset, reader.max_offset());
        assert!(reader.offset > 0);
        reader.page(true);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.offset, 0);
        reader.page(true);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.offset, 12);
    }

    #[test]
    fn scrolling_crosses_chapters_but_never_wraps_book_bounds() {
        let mut reader = reader();
        reader.offset = reader.max_offset();
        reader.scroll(1);
        assert_eq!(reader.chapter, 2);
        reader.offset = reader.max_offset();
        reader.page(true);
        assert_eq!(reader.chapter, 2);
        assert_eq!(reader.offset, reader.max_offset());
        reader.offset = 0;
        reader.scroll(-1);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.offset, reader.max_offset());
        reader.selected = 0;
        reader.open_selected();
        reader.page(false);
        assert_eq!(reader.chapter, 0);
        assert_eq!(reader.offset, 0);
    }

    #[test]
    fn single_page_chapters_and_unrendered_reader_are_safe() {
        let mut reader = reader();
        reader.prepare(76, 1000);
        assert_eq!(reader.max_offset(), 0);
        reader.page(true);
        assert_eq!(reader.chapter, 2);
        reader.page(false);
        assert_eq!(reader.chapter, 1);
        let mut reader = Reader::new(reader.book, false);
        reader.page(true);
        reader.page(false);
        assert_eq!(reader.chapter, 1);
    }
}
