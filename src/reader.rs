//! 阅读排版与章节导航；真实正文与显式演示模式共享渲染。
use crate::data::Book;
use crate::theme::{list_scroll, panel, row_line, s, sb, sbg, truncate, THEME};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthChar;

pub struct Reader {
    pub on_shelf: bool,
    pub layout: Option<crate::plugins::Layout>,
    pub title_rules: Vec<crate::purify::CompiledRule>,
    pub purify_errors: Vec<String>,
    title_cache: std::cell::RefCell<std::collections::HashMap<u32, String>>,
    pub request: Option<(u32, bool)>,
    real: Option<RealText>,
    pub options: crate::demo::Prefs,
    pub rules: Vec<crate::demo::Rule>,
    pub live_rules: Vec<crate::purify::CompiledRule>,
    pub book: Book,
    pub chapter: u32,
    pub selected: u32,
    pub offset: usize,
    pub previous_sidebar_hidden: bool,
    lines: Vec<String>,
    width: u16,
    height: usize,
    pending_join: Option<(u32, isize)>,
    joined_scroll: isize,
    horizontal_layout: bool,
    auto_active: bool,
    auto_elapsed_ms: u64,
    aloud: AloudState,
    demo_segments: Vec<AloudSegment>,
    pending_aloud: Option<(u32, bool)>,
    aloud_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AloudStatus {
    Inactive,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AloudSegment {
    start: usize,
    end: usize,
    line_start: usize,
    line_end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AloudState {
    status: AloudStatus,
    segment: usize,
    speed: usize,
    timer: usize,
    timer_remaining_ms: u64,
    revision: u64,
    progress: Option<usize>,
    preparing: bool,
    finished: bool,
}

const ALOUD_SPEEDS: [&str; 5] = ["0.8x", "1.0x", "1.25x", "1.5x", "2.0x"];
const ALOUD_TIMERS: [u64; 5] = [0, 15, 30, 60, 90];

struct RealText {
    titles: Vec<String>,
    text: String,
    positions: Vec<usize>,
    restore: Option<usize>,
    chapters: Vec<ChapterRange>,
}

struct ChapterRange {
    index: u32,
    start: usize,
    body: usize,
    end: usize,
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
            on_shelf: false,
            layout: None,
            title_rules: Vec::new(),
            purify_errors: Vec::new(),
            title_cache: Default::default(),
            request: None,
            real: None,
            options: crate::demo::Prefs::default(),
            rules: Vec::new(),
            live_rules: Vec::new(),
            book,
            chapter,
            selected: chapter,
            offset: 0,
            previous_sidebar_hidden,
            lines: Vec::new(),
            width: 0,
            height: 1,
            pending_join: None,
            joined_scroll: 0,
            horizontal_layout: false,
            auto_active: false,
            auto_elapsed_ms: 0,
            demo_segments: Vec::new(),
            pending_aloud: None,
            aloud_error: None,
            aloud: AloudState {
                status: AloudStatus::Inactive,
                segment: 0,
                speed: 1,
                timer: 0,
                timer_remaining_ms: 0,
                revision: 0,
                progress: None,
                preparing: false,
                finished: false,
            },
        }
    }

    pub fn aloud_active(&self) -> bool {
        self.aloud.status != AloudStatus::Inactive
    }

    pub fn aloud_playing(&self) -> bool {
        self.aloud.status == AloudStatus::Playing
    }

    pub fn aloud_loading(&self) -> bool {
        self.pending_aloud.is_some()
    }

    pub fn aloud_speed_label(&self) -> &'static str {
        ALOUD_SPEEDS[self.aloud.speed]
    }

    pub fn aloud_speed_percent(&self) -> u16 {
        [80, 100, 125, 150, 200][self.aloud.speed]
    }
    pub fn aloud_revision(&self) -> u64 {
        self.aloud.revision
    }
    pub fn aloud_preparing(&self) -> bool {
        self.aloud.preparing
    }
    pub fn aloud_error(&self) -> Option<&str> {
        self.aloud_error.as_deref()
    }
    pub fn set_aloud_preparing(&mut self, preparing: bool) {
        self.aloud.preparing = preparing;
    }
    pub fn set_aloud_finished(&mut self) {
        self.aloud.finished = true;
    }
    pub fn set_aloud_progress(&mut self, position: usize) {
        self.aloud.progress = Some(position);
        self.aloud.preparing = false;
        self.follow_aloud();
    }
    pub fn fail_aloud(&mut self, error: String) {
        self.aloud.status = AloudStatus::Paused;
        self.aloud.preparing = false;
        self.aloud_error = Some(error);
    }
    fn reset_aloud_segment(&mut self) {
        self.aloud.revision = self.aloud.revision.wrapping_add(1);
        self.aloud.progress = None;
        self.aloud.preparing = false;
        self.aloud.finished = false;
        self.aloud_error = None;
    }

    pub fn speech_text(&self) -> crate::audio::SpeechText {
        self.speech_text_at(self.aloud.segment, self.aloud.progress)
    }

    pub fn next_speech_text(&self) -> Option<String> {
        if self.aloud.segment + 1 >= self.aloud_segments().len() {
            return None;
        }
        Some(self.speech_text_at(self.aloud.segment + 1, None).text)
    }

    fn speech_text_at(&self, index: usize, progress: Option<usize>) -> crate::audio::SpeechText {
        let Some(segment) = self.aloud_segments().get(index).copied() else {
            return Default::default();
        };
        if let Some(real) = &self.real {
            let Some(chapter) = real
                .chapters
                .iter()
                .find(|chapter| chapter.index == self.chapter)
            else {
                return Default::default();
            };
            let start = progress
                .map_or(segment.start, |position| chapter.body + position)
                .clamp(segment.start, segment.end);
            let rules = if self.options.0[6] == 0 {
                self.live_rules.as_slice()
            } else {
                &[]
            };
            let (display, mapping, _) = crate::purify::apply_report(&real.text, rules);
            let mut spoken = crate::audio::SpeechText::default();
            for (ch, position) in display.chars().zip(mapping) {
                if position >= start && position < segment.end {
                    spoken.text.push(ch);
                    spoken.positions.push(position.saturating_sub(chapter.body));
                }
            }
            spoken
        } else {
            let paragraph = if index == 0 {
                format!(
                    "《{}》 · {}。以下为阅读排版预览，章节名与正文均为演示内容。",
                    self.book.title,
                    chapter_title(self.chapter)
                )
            } else {
                PARAGRAPHS[(self.chapter as usize + index - 1) % PARAGRAPHS.len()].to_string()
            };
            let text = if self.options.0[6] == 0 {
                crate::demo::purify(&paragraph, &self.rules)
            } else {
                paragraph
            };
            crate::audio::SpeechText {
                positions: vec![0; text.chars().count()],
                text,
            }
        }
    }

    pub fn aloud_timer_label(&self) -> String {
        if self.aloud.timer_remaining_ms == 0 {
            return "定时关".into();
        }
        let minutes = self.aloud.timer_remaining_ms.div_ceil(60_000);
        format!("定时 {minutes}分")
    }

    pub fn start_aloud(&mut self) {
        if self.request.is_some() {
            return;
        }
        let segments = self.aloud_segments();
        if segments.is_empty() {
            return;
        }
        let position = if self.real.is_some() {
            self.absolute_position()
        } else {
            self.offset
        };
        self.aloud.segment = segments
            .iter()
            .position(|segment| position >= segment.start && position < segment.end)
            .or_else(|| segments.iter().position(|segment| position < segment.start))
            .unwrap_or(segments.len().saturating_sub(1));
        self.auto_active = false;
        self.reset_aloud_segment();
        if let Some(real) = &self.real {
            if let Some(chapter) = real
                .chapters
                .iter()
                .find(|chapter| chapter.index == self.chapter)
            {
                self.aloud.progress = Some(
                    position
                        .max(segments[self.aloud.segment].start)
                        .saturating_sub(chapter.body),
                );
            }
        }
        self.aloud.status = AloudStatus::Playing;
        self.follow_aloud();
    }

    pub fn toggle_aloud(&mut self) {
        match self.aloud.status {
            AloudStatus::Inactive => self.start_aloud(),
            AloudStatus::Playing => self.aloud.status = AloudStatus::Paused,
            AloudStatus::Paused => {
                self.aloud.status = AloudStatus::Playing;
                if self.aloud_error.take().is_some() {
                    self.aloud.revision = self.aloud.revision.wrapping_add(1);
                }
                if self.aloud.finished {
                    self.next_aloud_segment();
                }
            }
        }
    }

    pub fn stop_aloud(&mut self) {
        self.aloud.status = AloudStatus::Inactive;
        self.aloud.timer = 0;
        self.aloud.timer_remaining_ms = 0;
        self.pending_aloud = None;
        self.reset_aloud_segment();
    }

    pub fn next_aloud_segment(&mut self) {
        if !self.aloud_active() || self.aloud_loading() {
            return;
        }
        let count = self.aloud_segments().len();
        if self.aloud.segment + 1 < count {
            self.aloud.segment += 1;
            self.reset_aloud_segment();
        } else if self.chapter + 1 < self.book.total {
            self.jump_aloud_chapter(1, false);
            return;
        } else {
            self.stop_aloud();
            return;
        }
        self.follow_aloud();
    }

    pub fn previous_aloud_segment(&mut self) {
        if !self.aloud_active() || self.aloud_loading() {
            return;
        }
        if self.aloud.segment > 0 {
            self.aloud.segment -= 1;
        } else if self.chapter > 0 {
            self.jump_aloud_chapter(-1, true);
            return;
        }
        self.reset_aloud_segment();
        self.follow_aloud();
    }

    pub fn next_aloud_chapter(&mut self) {
        self.jump_aloud_chapter(1, false);
    }

    pub fn previous_aloud_chapter(&mut self) {
        self.jump_aloud_chapter(-1, false);
    }

    fn jump_aloud_chapter(&mut self, delta: i32, last: bool) {
        if !self.aloud_active() || self.aloud_loading() {
            return;
        }
        let target = (i64::from(self.chapter) + i64::from(delta))
            .clamp(0, i64::from(self.book.total.saturating_sub(1))) as u32;
        if target == self.chapter {
            return;
        }
        self.pending_aloud = Some((target, last));
        self.change_chapter(delta);
        if self.chapter == target {
            self.finish_aloud_chapter();
        }
    }

    fn finish_aloud_chapter(&mut self) {
        if let Some((target, last)) = self.pending_aloud {
            if self.chapter == target {
                self.pending_aloud = None;
                self.reset_aloud_segment();
                self.aloud.segment = if last {
                    self.aloud_segments().len().saturating_sub(1)
                } else {
                    0
                };
                if self.aloud_segments().is_empty() {
                    self.stop_aloud();
                }
            }
        }
    }

    pub fn aloud_chapter_failed(&mut self) {
        if self.pending_aloud.take().is_some() && self.aloud_active() {
            self.aloud.status = AloudStatus::Paused;
            self.selected = self.chapter;
        }
    }

    pub fn adjust_aloud_speed(&mut self, delta: i32) {
        if !self.aloud_active() {
            return;
        }
        self.aloud.speed =
            (self.aloud.speed as i32 + delta).clamp(0, ALOUD_SPEEDS.len() as i32 - 1) as usize;
    }

    pub fn cycle_aloud_timer(&mut self) {
        if !self.aloud_active() {
            return;
        }
        self.aloud.timer = (self.aloud.timer + 1) % ALOUD_TIMERS.len();
        self.aloud.timer_remaining_ms = ALOUD_TIMERS[self.aloud.timer] * 60_000;
    }

    pub fn tick_aloud(&mut self, elapsed_ms: u64) {
        if !self.aloud_active() || self.aloud.timer_remaining_ms == 0 {
            return;
        }
        self.aloud.timer_remaining_ms = self.aloud.timer_remaining_ms.saturating_sub(elapsed_ms);
        if self.aloud.timer_remaining_ms == 0 {
            self.stop_aloud();
        }
    }

    fn aloud_highlight(&self) -> Option<AloudSegment> {
        if !self.aloud_active() {
            return None;
        }
        self.aloud_segments().get(self.aloud.segment).copied()
    }

    fn follow_aloud(&mut self) {
        if self.width == 0 || self.aloud_loading() {
            return;
        }
        if let Some(segment) = self.aloud_highlight() {
            if segment.line_start >= segment.line_end {
                return;
            }
            let anchor = self
                .real
                .as_ref()
                .and_then(|real| {
                    let progress = self.aloud.progress?;
                    let chapter = real
                        .chapters
                        .iter()
                        .find(|chapter| chapter.index == self.chapter)?;
                    Some(
                        real.positions
                            .partition_point(|position| *position <= chapter.body + progress)
                            .saturating_sub(1)
                            .clamp(segment.line_start, segment.line_end - 1),
                    )
                })
                .unwrap_or(segment.line_start);
            let end = if self.aloud.progress.is_some() {
                anchor + 1
            } else {
                segment.line_end
            };
            if anchor < self.offset || end > self.offset + self.height {
                self.offset = anchor.min(self.max_offset());
                if self.is_horizontal() {
                    self.offset = self.offset / self.height * self.height;
                }
            }
        }
    }

    pub fn select(&mut self, delta: i32) {
        self.selected = (i64::from(self.selected) + i64::from(delta))
            .clamp(0, i64::from(self.book.total.saturating_sub(1))) as u32;
    }

    pub fn open_selected(&mut self) {
        self.pending_join = None;
        self.auto_elapsed_ms = 0;
        if self.real.is_some() {
            if self.seamless() && self.jump_to_loaded(self.selected, false) {
                return;
            }
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
        if self.is_horizontal() || self.width == 0 || self.lines.is_empty() {
            return;
        }
        if self.seamless() && self.real.is_some() {
            let real = self.real.as_ref().unwrap();
            let target = if delta > 0 && self.offset == self.max_offset() {
                real.chapters.last().map(|c| (c.index + 1, false))
            } else if delta < 0 && self.offset == 0 {
                real.chapters
                    .first()
                    .and_then(|c| c.index.checked_sub(1))
                    .map(|i| (i, true))
            } else {
                None
            };
            if let Some((index, end)) = target {
                if index < self.book.total {
                    self.pending_join = Some((index, delta));
                    self.request = Some((index, end));
                }
                return;
            }
            self.offset = self
                .offset
                .saturating_add_signed(delta)
                .min(self.max_offset());
            self.sync_chapter();
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
        if self.is_horizontal() {
            self.horizontal_page(forward);
            return;
        }
        let step = self.height.max(1) as isize;
        self.scroll(if forward { step } else { -step });
    }

    pub fn is_horizontal(&self) -> bool {
        self.options.0[4] == 1
    }

    pub fn auto_active(&self) -> bool {
        self.auto_active
    }

    pub fn toggle_auto(&mut self) {
        self.auto_active = !self.auto_active;
        self.auto_elapsed_ms = 0;
    }

    pub fn auto_progress(&self) -> f64 {
        let interval = self.auto_interval_ms();
        (self.auto_elapsed_ms as f64 / interval as f64).clamp(0.0, 1.0)
    }

    pub fn auto_interval_ms(&self) -> u64 {
        let index = if self.is_horizontal() { 11 } else { 10 };
        [1_000, 2_000, 3_000, 5_000, 10_000][self.options.0.get(index).copied().unwrap_or(0).min(4)]
    }

    pub fn tick_auto(&mut self, elapsed_ms: u64) {
        if !self.auto_active || self.request.is_some() {
            return;
        }
        self.auto_elapsed_ms = self.auto_elapsed_ms.saturating_add(elapsed_ms);
        let interval = self.auto_interval_ms();
        if self.auto_elapsed_ms < interval {
            return;
        }
        if self.width == 0 || self.lines.is_empty() {
            return;
        }
        self.auto_elapsed_ms %= interval;
        if self.is_horizontal() {
            self.page(true);
        } else {
            self.scroll(1);
        }
    }

    pub fn horizontal_arrow(&mut self, right: bool) {
        if self.is_horizontal() {
            self.page(right != (self.options.0[9] == 1));
        }
    }

    fn horizontal_page(&mut self, forward: bool) {
        if self.width == 0 || self.lines.is_empty() {
            return;
        }
        if forward {
            if self.offset == self.max_offset() {
                self.change_chapter(1);
            } else {
                self.offset = (self.offset + self.height).min(self.max_offset());
            }
        } else if self.offset == 0 {
            if self.chapter > 0 {
                if self.real.is_some() {
                    self.request = Some((self.chapter - 1, true));
                    return;
                }
                self.change_chapter(-1);
                self.offset = self.max_offset();
            }
        } else {
            self.offset = self.offset.saturating_sub(self.height);
        }
    }

    pub fn max_offset(&self) -> usize {
        if self.is_horizontal() {
            self.lines.len().saturating_sub(1) / self.height * self.height
        } else {
            self.lines.len().saturating_sub(self.height)
        }
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

    pub fn display_title(&self, index: u32) -> String {
        if self.options.0[6] != 0 {
            return self.title(index);
        }
        if let Some(title) = self.title_cache.borrow().get(&index) {
            return title.clone();
        }
        let title = self.title(index);
        let title = crate::purify::apply(&title, &self.title_rules).0;
        self.title_cache.borrow_mut().insert(index, title.clone());
        title
    }

    pub fn refresh_layout(&mut self) {
        let position = self.absolute_position();
        let local_position = self.position();
        let horizontal = self.is_horizontal();
        if let Some(real) = &mut self.real {
            if horizontal {
                if let Some(range) = real.chapters.iter().find(|c| c.index == self.chapter) {
                    // Horizontal pages contain only the active chapter, without joined titles or neighbors.
                    let length = range.end - range.body;
                    real.text = real.text.chars().skip(range.body).take(length).collect();
                    real.chapters = vec![ChapterRange {
                        index: self.chapter,
                        start: 0,
                        body: 0,
                        end: length,
                    }];
                }
                real.restore = Some(local_position);
                self.pending_join = None;
                self.joined_scroll = 0;
            } else {
                real.restore = Some(position);
            }
        }
        self.title_cache.get_mut().clear();
        self.width = 0;
    }

    pub fn set_real(&mut self, titles: Vec<String>, text: String, position: usize) {
        self.title_cache.get_mut().clear();
        let length = text.chars().count();
        let heading = if self.seamless() {
            format!(
                "【{}】\n",
                titles
                    .get(self.chapter as usize)
                    .cloned()
                    .unwrap_or_else(|| chapter_title(self.chapter))
            )
        } else {
            String::new()
        };
        let body = heading.chars().count();
        self.real = Some(RealText {
            titles,
            text: format!("{heading}{text}"),
            positions: vec![],
            restore: Some(if position == 0 {
                0
            } else {
                body + position.min(length)
            }),
            chapters: vec![ChapterRange {
                index: self.chapter,
                start: 0,
                body,
                end: body + length,
            }],
        });
        self.pending_join = None;
        self.joined_scroll = 0;
        self.width = 0;
    }

    pub fn set_chapter(&mut self, index: u32, text: String, end: bool) {
        self.install_chapter(index, text, end);
        self.finish_aloud_chapter();
    }

    fn install_chapter(&mut self, index: u32, text: String, end: bool) {
        self.auto_elapsed_ms = 0;
        let position = self.absolute_position();
        let pending = self
            .pending_join
            .take()
            .filter(|(target, _)| *target == index);
        let seamless = self.seamless();
        if seamless && self.jump_to_loaded(index, end) {
            return;
        }
        let heading = format!("【{}】\n", self.display_title(index));
        if let Some(real) = &mut self.real {
            let length = text.chars().count();
            let heading_len = heading.chars().count();
            let append = seamless && real.chapters.last().is_some_and(|c| c.index + 1 == index);
            let prepend = seamless && real.chapters.first().is_some_and(|c| index + 1 == c.index);
            if append || prepend {
                let (start, body, target) = if append {
                    let start = real.text.chars().count() + 1;
                    real.text.push_str(&format!("\n{heading}{text}"));
                    (start, start + heading_len, position)
                } else {
                    let prefix = format!("{heading}{text}\n");
                    let shift = prefix.chars().count();
                    for chapter in &mut real.chapters {
                        chapter.start += shift;
                        chapter.body += shift;
                        chapter.end += shift;
                    }
                    real.text.insert_str(0, &prefix);
                    (0, heading_len, position + shift)
                };
                let range = ChapterRange {
                    index,
                    start,
                    body,
                    end: body + length,
                };
                let jump = if end { range.end } else { range.start };
                if append {
                    real.chapters.push(range);
                } else {
                    real.chapters.insert(0, range);
                }
                // Automatic joins retain the viewport anchor; explicit chapter jumps land at the title.
                real.restore = Some(if pending.is_some() { target } else { jump });
                self.joined_scroll = pending.map_or(0, |(_, delta)| delta);
                self.width = 0;
                self.sync_chapter();
                return;
            }
            let body = if seamless { heading_len } else { 0 };
            real.restore = Some(if end { body + length } else { 0 });
            real.text = if seamless {
                format!("{heading}{text}")
            } else {
                text
            };
            real.positions.clear();
            real.chapters = vec![ChapterRange {
                index,
                start: 0,
                body,
                end: body + length,
            }];
        }
        self.chapter = index;
        self.selected = index;
        self.offset = 0;
        self.joined_scroll = 0;
        self.width = 0;
        self.lines.clear();
    }

    fn seamless(&self) -> bool {
        self.options.0[4] == 0 && self.options.0[8] == 1
    }

    fn jump_to_loaded(&mut self, index: u32, end: bool) -> bool {
        let Some(real) = &mut self.real else {
            return false;
        };
        let Some(range) = real.chapters.iter().find(|c| c.index == index) else {
            return false;
        };
        real.restore = Some(if end { range.end } else { range.start });
        self.chapter = index;
        self.selected = index;
        self.joined_scroll = 0;
        self.width = 0;
        true
    }

    fn sync_chapter(&mut self) {
        let position = self.absolute_position();
        if let Some(range) = self
            .real
            .as_ref()
            .and_then(|r| r.chapters.iter().rev().find(|c| c.start <= position))
        {
            if self.chapter != range.index {
                self.chapter = range.index;
                self.selected = range.index;
            }
        }
    }

    /// Tlegado stores a Unicode scalar offset here, never a terminal row number.
    pub fn position(&self) -> usize {
        if self.aloud_active() {
            if let Some(position) = self.aloud.progress {
                return position;
            }
        }
        let position = self
            .aloud_highlight()
            .map_or_else(|| self.absolute_position(), |segment| segment.start);
        self.real
            .as_ref()
            .and_then(|r| r.chapters.iter().find(|c| c.index == self.chapter))
            .map_or(0, |c| position.saturating_sub(c.body).min(c.end - c.body))
    }

    fn absolute_position(&self) -> usize {
        self.real
            .as_ref()
            .map(|r| {
                r.restore
                    .unwrap_or_else(|| r.positions.get(self.offset).copied().unwrap_or(0))
            })
            .unwrap_or(0)
    }

    fn aloud_segments(&self) -> Vec<AloudSegment> {
        if let Some(real) = &self.real {
            let Some(range) = real.chapters.iter().find(|c| c.index == self.chapter) else {
                return Vec::new();
            };
            let chapter: String = real
                .text
                .chars()
                .skip(range.body)
                .take(range.end.saturating_sub(range.body))
                .collect();
            let mut segments = Vec::new();
            let mut start = range.body;
            for paragraph in chapter.split('\n') {
                let len = paragraph.chars().count();
                let end = start + len;
                if !paragraph.trim().is_empty() {
                    let line_start = real.positions.partition_point(|position| *position < start);
                    let line_end = real.positions.partition_point(|position| *position < end);
                    segments.push(AloudSegment {
                        start,
                        end,
                        line_start,
                        line_end,
                    });
                }
                start = end + 1;
            }
            return segments;
        }

        self.demo_segments.clone()
    }

    fn prepare(&mut self, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        let horizontal = self.is_horizontal();
        if horizontal != self.horizontal_layout {
            self.refresh_layout();
            self.horizontal_layout = horizontal;
        }
        let position = self.absolute_position();
        if let Some(real) = &mut self.real {
            self.height = height as usize;
            if self.width != width {
                self.lines.clear();
                real.positions.clear();
                let indent = self
                    .layout
                    .as_ref()
                    .and_then(|l| l.indent.as_deref())
                    .unwrap_or(if self.options.0[3] == 1 { "　　" } else { "" });
                let line_gap = self
                    .layout
                    .as_ref()
                    .and_then(|l| l.line_gap)
                    .unwrap_or(usize::from(self.options.0[2] == 1));
                let paragraph_gap = self
                    .layout
                    .as_ref()
                    .and_then(|l| l.paragraph_gap)
                    .unwrap_or(1);
                let mut base = 0;
                let rules = if self.options.0[6] == 0 {
                    self.live_rules.as_slice()
                } else {
                    &[]
                };
                let (display, mapping, errors) = crate::purify::apply_report(&real.text, rules);
                self.purify_errors = errors;
                for paragraph in display.split('\n') {
                    let heading = real
                        .chapters
                        .iter()
                        .any(|c| c.body > c.start && mapping[base] == c.start);
                    let paragraph_indent = if heading { "" } else { indent };
                    if heading {
                        for _ in 0..2 {
                            real.positions.push(mapping[base]);
                            self.lines.push(String::new());
                        }
                    }
                    let decorated = format!("{paragraph_indent}{paragraph}");
                    let wrapped = wrap(&decorated, width as usize);
                    let mut used: usize = 0;
                    for line in wrapped {
                        real.positions.push(
                            mapping[base
                                + used
                                    .saturating_sub(paragraph_indent.chars().count())
                                    .min(paragraph.chars().count())],
                        );
                        used += line.chars().count();
                        self.lines.push(line);
                        for _ in 0..if heading { 0 } else { line_gap } {
                            real.positions.push(
                                mapping[base
                                    + used
                                        .saturating_sub(paragraph_indent.chars().count())
                                        .min(paragraph.chars().count())],
                            );
                            self.lines.push(String::new());
                        }
                    }
                    for _ in 0..if heading { 2 } else { paragraph_gap } {
                        real.positions
                            .push(mapping[base + paragraph.chars().count()]);
                        self.lines.push(String::new());
                    }
                    base += paragraph.chars().count() + 1;
                }
                if horizontal {
                    while self.lines.last().is_some_and(String::is_empty) {
                        self.lines.pop();
                        real.positions.pop();
                    }
                }
                let next = real.positions.partition_point(|p| *p < position);
                self.offset = if real.positions.get(next) == Some(&position) {
                    next
                } else {
                    next.saturating_sub(1)
                };
                real.restore = None;
                self.width = width;
            }
            self.offset = self.offset.min(self.max_offset());
            self.offset = self
                .offset
                .saturating_add_signed(std::mem::take(&mut self.joined_scroll))
                .min(self.max_offset());
            if horizontal {
                self.offset = self.offset / self.height * self.height;
            }
            if !self.aloud_active() {
                self.sync_chapter();
            }
            self.follow_aloud();
            return;
        }
        if self.width != width {
            self.lines.clear();
            self.demo_segments.clear();
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
                let start = self.lines.len();
                for line in wrap(&format!("{indent}{paragraph}"), width as usize) {
                    self.lines.push(line);
                    if self.options.0[2] == 1 {
                        self.lines.push(String::new());
                    }
                }
                if !paragraph.trim().is_empty() {
                    self.demo_segments.push(AloudSegment {
                        start,
                        end: self.lines.len(),
                        line_start: start,
                        line_end: self.lines.len(),
                    });
                }
                self.lines.push(String::new());
            }
            self.width = width;
        }
        if horizontal {
            while self.lines.last().is_some_and(String::is_empty) {
                self.lines.pop();
            }
        }
        self.height = height as usize;
        self.offset = self.offset.min(self.max_offset());
        if horizontal {
            self.offset = self.offset / self.height * self.height;
        }
        self.follow_aloud();
    }
}

#[cfg(test)]
mod aloud_tests {
    use super::*;

    fn reader(text: &str, horizontal: bool, seamless: bool) -> Reader {
        let mut book = crate::data::shelf().remove(0);
        book.read = 0;
        book.total = 3;
        let mut reader = Reader::new(book, false);
        reader.options.0[4] = usize::from(horizontal);
        reader.options.0[8] = usize::from(seamless);
        reader.set_real(vec!["一".into(), "二".into(), "三".into()], text.into(), 0);
        reader.prepare(18, 5);
        reader
    }

    #[test]
    fn aloud_highlight_is_distinct_across_themes_without_changing_text_layout() {
        use ratatui::{
            backend::TestBackend,
            style::{Color, Modifier},
            Terminal,
        };
        let brightness = |color: Color| -> i32 {
            match color {
                Color::Rgb(r, g, b) => {
                    (i32::from(r) * 299 + i32::from(g) * 587 + i32::from(b) * 114) / 1000
                }
                Color::Black => 0,
                Color::White => 255,
                _ => panic!("expected explicit reading colors"),
            }
        };
        for (theme, custom_background) in [
            (0, None),
            (1, None),
            (2, None),
            (3, None),
            (0, Some("#ffe3a35a")),
            (0, Some("#ff666666")),
        ] {
            let mut reader = reader(
                &format!("{}\n普通段落保持原样。", "当前朗读段落😀ABC。".repeat(4)),
                false,
                false,
            );
            reader.options.0[0] = theme;
            reader.options.0[2] = 1;
            if let Some(color) = custom_background {
                reader.layout = Some(
                    crate::plugins::parse_layout(&format!(
                        r##"{{"bgStr":"{color}","textColor":"#ff999999","lineSpacingExtra":1}}"##
                    ))
                    .unwrap(),
                );
            }
            reader.refresh_layout();
            let mut terminal = Terminal::new(TestBackend::new(60, 22)).unwrap();
            let normal = terminal
                .draw(|frame| draw_page(frame, &mut reader, frame.area(), true))
                .unwrap()
                .buffer
                .clone();
            reader.start_aloud();
            let active = terminal
                .draw(|frame| draw_page(frame, &mut reader, frame.area(), true))
                .unwrap()
                .buffer
                .clone();
            let segment = reader.aloud_highlight().unwrap();
            for row in segment.line_start..segment.line_end {
                let y = 2 + (row - reader.offset) as u16;
                assert_eq!(
                    active[(1, y)].symbol(),
                    if row == segment.line_start {
                        "▶"
                    } else {
                        "┃"
                    }
                );
                for x in 3..57 {
                    let before = &normal[(x, y)];
                    let after = &active[(x, y)];
                    assert_eq!(
                        after.symbol(),
                        before.symbol(),
                        "theme={theme}, x={x}, y={y}"
                    );
                    assert!((brightness(after.bg) - brightness(before.bg)).abs() >= 70);
                    assert!((brightness(after.fg) - brightness(after.bg)).abs() >= 120);
                    assert!(after.modifier.contains(Modifier::BOLD));
                }
            }
            let next_row = 2 + reader.aloud_segments()[1].line_start as u16;
            assert_eq!(normal[(3, next_row)], active[(3, next_row)]);
            reader.toggle_aloud();
            let paused = terminal
                .draw(|frame| draw_page(frame, &mut reader, frame.area(), true))
                .unwrap()
                .buffer
                .clone();
            assert_eq!(active[(3, 2)], paused[(3, 2)]);
            reader.stop_aloud();
            let stopped = terminal
                .draw(|frame| draw_page(frame, &mut reader, frame.area(), true))
                .unwrap()
                .buffer
                .clone();
            assert_eq!(normal, stopped);
        }
    }

    #[test]
    fn spoken_text_uses_purification_and_original_offsets_without_headings_or_layout() {
        use reader_core::model::replace_rule::ReplaceRule;
        let mut reader = reader("广告甲😀第一段。\n第二段。", false, true);
        reader.live_rules = vec![crate::purify::CompiledRule::new(&ReplaceRule {
            name: "删除广告".into(),
            pattern: "广告".into(),
            replacement: "".into(),
            ..Default::default()
        })
        .unwrap()];
        reader.refresh_layout();
        reader.prepare(18, 5);
        reader.start_aloud();
        let spoken = reader.speech_text();
        assert_eq!(spoken.text, "甲😀第一段。");
        assert_eq!(spoken.positions, (2..8).collect::<Vec<_>>());
        reader.set_aloud_progress(4);
        assert_eq!(reader.position(), 4);
        assert_eq!(reader.speech_text().text, "第一段。");
        reader.next_aloud_segment();
        assert_eq!(reader.speech_text().text, "第二段。");
        assert_eq!(reader.position(), 9);
    }

    #[test]
    fn chunk_progress_follows_within_a_long_paragraph_and_keeps_its_identity() {
        let text = "长段落😀。".repeat(300);
        for horizontal in [false, true] {
            let mut reader = reader(&text, horizontal, false);
            reader.start_aloud();
            reader.set_aloud_progress(480);
            let position = reader.position();
            assert_eq!(position, 480);
            assert!(reader.offset > 0);
            assert_eq!(reader.aloud.segment, 0);
            reader.prepare(24, 6);
            assert_eq!(reader.position(), position);
            assert_eq!(reader.aloud.segment, 0);
            assert!(reader.offset > 0);
        }
    }

    #[test]
    fn paragraphs_keep_unicode_offsets_and_follow_after_resize() {
        let first = "中文😀 mixed words。".repeat(12);
        let text = format!("{first}\r\n \t\r\n第二段内容。\n第三段结尾。");
        for horizontal in [false, true] {
            let mut reader = reader(&text, horizontal, false);
            let segments = reader.aloud_segments();
            assert_eq!(segments.len(), 3);
            assert_eq!(segments[1].start, first.chars().count() + 6);
            reader.start_aloud();
            reader.next_aloud_segment();
            let position = reader.position();
            assert_eq!(position, segments[1].start);
            assert!(reader.offset > 0);
            reader.toggle_aloud();
            reader.prepare(10, 4);
            assert!(!reader.aloud_playing());
            assert_eq!(reader.position(), position);
            let highlighted = reader.aloud_highlight().unwrap();
            assert!(highlighted.line_start >= reader.offset);
            assert!(highlighted.line_start < reader.offset + reader.height);
            reader.previous_aloud_segment();
            assert_eq!(reader.position(), 0);
            assert!(!reader.aloud_playing());
        }
    }

    #[test]
    fn chapter_requests_preserve_old_segment_until_success_and_can_retry_failure() {
        for seamless in [false, true] {
            let mut reader = reader("第一段\n第二段", false, seamless);
            reader.start_aloud();
            reader.next_aloud_segment();
            let position = reader.position();
            reader.next_aloud_segment();
            assert_eq!(reader.request.take(), Some((1, false)));
            assert_eq!(reader.chapter, 0);
            assert_eq!(reader.position(), position);
            reader.next_aloud_segment();
            assert!(reader.request.is_none());
            reader.aloud_chapter_failed();
            assert!(!reader.aloud_playing());
            assert!(!reader.aloud_loading());
            assert_eq!(reader.position(), position);
            reader.next_aloud_chapter();
            assert_eq!(reader.request.take(), Some((1, false)));
            reader.set_chapter(1, "新章第一段\n新章第二段\n新章第三段".into(), false);
            reader.prepare(18, 5);
            assert_eq!(reader.chapter, 1);
            assert_eq!(reader.aloud.segment, 0);
            assert!(!reader.aloud_loading());
            reader.previous_aloud_segment();
            if let Some((index, end)) = reader.request.take() {
                reader.set_chapter(index, "第一段\n第二段".into(), end);
            }
            reader.prepare(18, 5);
            assert_eq!(reader.chapter, 0);
            assert_eq!(reader.aloud.segment, 1);
            assert_eq!(reader.position(), "第一段\n".chars().count());
        }
    }

    #[test]
    fn timer_expires_while_paused_and_restarts_at_first_preset() {
        let mut reader = reader("正文", false, false);
        reader.toggle_auto();
        reader.start_aloud();
        assert!(!reader.auto_active());
        reader.cycle_aloud_timer();
        reader.toggle_aloud();
        reader.tick_aloud(15 * 60_000);
        assert!(!reader.aloud_active());
        reader.start_aloud();
        reader.cycle_aloud_timer();
        assert_eq!(reader.aloud.timer_remaining_ms, 15 * 60_000);
        for _ in 0..10 {
            reader.adjust_aloud_speed(1);
        }
        assert_eq!(reader.aloud_speed_label(), "2.0x");
        for _ in 0..10 {
            reader.adjust_aloud_speed(-1);
        }
        assert_eq!(reader.aloud_speed_label(), "0.8x");
    }

    #[test]
    fn demo_line_spacing_does_not_split_paragraphs_and_starts_at_viewport() {
        let mut reader = Reader::new(crate::data::shelf().remove(0), false);
        reader.options.0[2] = 1;
        reader.prepare(18, 5);
        assert_eq!(reader.aloud_segments().len(), 19);
        reader.offset = reader.aloud_segments()[3].line_start;
        reader.start_aloud();
        assert_eq!(reader.aloud.segment, 3);
        reader.prepare(25, 7);
        assert_eq!(reader.aloud.segment, 3);
        assert_eq!(reader.aloud_segments().len(), 19);
    }

    #[test]
    fn empty_chapter_and_end_of_book_stop_cleanly() {
        let mut reader = reader("最后一段", false, false);
        reader.book.total = 1;
        reader.start_aloud();
        reader.next_aloud_segment();
        assert!(!reader.aloud_active());
        reader.book.total = 2;
        reader.start_aloud();
        reader.next_aloud_chapter();
        reader.request.take();
        reader.set_chapter(1, " \n\t\n".into(), false);
        assert!(!reader.aloud_active());
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

    fn seamless_reader(index: u32) -> Reader {
        let mut reader = reader();
        reader.chapter = index;
        reader.book.total = 4;
        reader.options.0[8] = 1;
        reader.set_real(
            vec![
                "第一章".into(),
                "第二章".into(),
                "第三章".into(),
                "第四章".into(),
            ],
            format!("第{index}章正文").repeat(150),
            0,
        );
        reader.prepare(44, 12);
        reader
    }

    #[test]
    fn seamless_explicit_next_chapter_lands_at_title_instead_of_end() {
        let mut reader = seamless_reader(0);
        reader.change_chapter(1);
        reader.set_chapter(1, "第二章开头正文".repeat(150), false);
        reader.prepare(44, 12);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.position(), 0);
        assert!(reader.lines[reader.offset + 2].contains("【第二章】"));
        assert!(reader.offset < reader.max_offset());
        assert!(reader.lines.join("").contains("第0章正文"));
    }

    #[test]
    fn seamless_append_continues_one_row_and_can_scroll_back_without_reloading() {
        let mut reader = seamless_reader(0);
        reader.offset = reader.max_offset();
        let offset = reader.offset;
        reader.scroll(1);
        assert_eq!(reader.request.take(), Some((1, false)));
        reader.set_chapter(1, "第二章正文".repeat(150), false);
        reader.prepare(44, 12);
        assert_eq!(reader.offset, offset + 1);
        assert!(reader.offset < reader.max_offset());
        let title = reader
            .lines
            .iter()
            .position(|l| l.contains("【第二章】"))
            .unwrap();
        assert!(title >= reader.offset);
        reader.scroll(2);
        assert!(title < reader.offset + reader.height);
        reader.offset = title;
        reader.prepare(44, 12);
        assert_eq!((reader.chapter, reader.position()), (1, 0));
        reader.scroll(-3);
        assert_eq!(reader.chapter, 0);
        assert!(reader.request.is_none());
        reader.offset = reader.max_offset();
        reader.prepare(44, 12);
        reader.scroll(1);
        assert_eq!(reader.request, Some((2, false)));
    }

    #[test]
    fn seamless_prepend_preserves_anchor_and_correct_title_order() {
        let mut reader = seamless_reader(1);
        let old_view = reader.lines[..reader.height].to_vec();
        reader.scroll(-1);
        assert_eq!(reader.request.take(), Some((0, true)));
        reader.set_chapter(0, "第一章正文".repeat(150), true);
        reader.prepare(44, 12);
        assert_eq!(
            reader.lines[reader.offset + 1..reader.offset + 1 + reader.height],
            old_view
        );
        let text = reader.lines.join("");
        assert!(text.find("【第一章】").unwrap() < text.find("第一章正文").unwrap());
        assert!(text.find("第一章正文").unwrap() < text.find("【第二章】").unwrap());
        assert_eq!(text.matches("【第一章】").count(), 1);
        reader.scroll(1);
        assert_eq!((reader.chapter, reader.position()), (1, 0));
        assert!(reader.request.is_none());
    }

    #[test]
    fn seamless_progress_is_chapter_local_after_resize_and_reopen() {
        let mut reader = seamless_reader(0);
        let text = "第二章😀正文内容。".repeat(200);
        reader.set_chapter(1, text.clone(), false);
        reader.prepare(44, 12);
        reader.page(true);
        let position = reader.position();
        assert!(position > 0 && position < text.chars().count());
        reader.prepare(28, 12);
        assert_eq!(reader.chapter, 1);
        assert!(reader.position().abs_diff(position) < 28);
        reader.refresh_layout();
        reader.prepare(28, 12);
        assert_eq!(reader.chapter, 1);
        let position = reader.position();
        let mut reopened = seamless_reader(1);
        reopened.set_real(reader.real.as_ref().unwrap().titles.clone(), text, position);
        reopened.prepare(28, 12);
        assert_eq!(reopened.position(), position);
        assert_eq!(reopened.lines[reopened.offset], reader.lines[reader.offset]);
    }

    #[test]
    fn seamless_loaded_chapter_jump_does_not_duplicate_content() {
        let mut reader = seamless_reader(0);
        reader.set_chapter(1, "第二章正文".repeat(100), false);
        reader.prepare(44, 12);
        let text = reader.real.as_ref().unwrap().text.clone();
        reader.selected = 0;
        reader.open_selected();
        reader.prepare(44, 12);
        reader.selected = 1;
        reader.open_selected();
        reader.prepare(44, 12);
        assert!(reader.request.is_none());
        assert_eq!(reader.real.as_ref().unwrap().text, text);
        assert_eq!((reader.chapter, reader.position()), (1, 0));
        reader.set_chapter(3, "第四章正文".repeat(100), false);
        reader.prepare(44, 12);
        assert_eq!(reader.chapter, 3);
        assert!(!reader.real.as_ref().unwrap().text.contains("第二章正文"));
    }

    #[test]
    fn horizontal_mode_does_not_join_chapters() {
        let mut reader = seamless_reader(0);
        reader.options.0[4] = 1;
        reader.set_chapter(1, "第二章正文".repeat(100), false);
        reader.prepare(44, 12);
        assert_eq!(reader.chapter, 1);
        assert!(!reader.real.as_ref().unwrap().text.contains("第0章正文"));
    }

    #[test]
    fn seamless_titles_have_spacing_and_centered_emphasis_at_different_widths() {
        use ratatui::{backend::TestBackend, style::Modifier, Terminal};
        let mut reader = seamless_reader(0);
        reader.layout = Some(
            crate::plugins::parse_layout(
                r#"{"paragraphIndent":"--","lineSpacingExtra":0,"paragraphSpacing":0}"#,
            )
            .unwrap(),
        );
        reader.refresh_layout();
        for (width, height) in [(120, 30), (80, 24), (36, 16)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| draw_page(f, &mut reader, f.area(), true))
                .unwrap();
            assert!(reader.lines[0..2].iter().all(String::is_empty));
            assert_eq!(reader.lines[2], "【第一章】");
            assert!(reader.lines[3..5].iter().all(String::is_empty));
            assert!(reader.lines[5].starts_with("--"));
            let buffer = terminal.backend().buffer();
            let (column, row) = (0..height)
                .find_map(|y| {
                    (0..width)
                        .find(|&x| buffer[(x, y)].symbol() == "【")
                        .map(|x| (x, y))
                })
                .unwrap();
            let cell = &buffer[(column, row)];
            assert!(cell
                .modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED));
            assert!(column > width / 4 && column < width / 2);
            assert_eq!((reader.chapter, reader.position()), (0, 0));
        }
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
            reader.display_title(index as u32)
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
    if inner.width < 4 || inner.height < 3 {
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
    // The first and last inner rows are compact navigation chrome.  Keeping
    // them to one row leaves substantially more room for the actual text.
    let content = Rect::new(x, inner.y + 1, width, inner.height.saturating_sub(2));
    reader.prepare(content.width, content.height);
    let chapter_title = truncate(&reader.display_title(reader.chapter), width as usize);
    let top = if reader.offset == 0 {
        format!("╭─ 第 {}  {} ─╮", reader.chapter + 1, chapter_title)
    } else {
        format!("· {}", chapter_title)
    };
    f.render_widget(
        Paragraph::new(top).style(if reader.offset == 0 {
            sb(THEME.hi)
        } else {
            s(THEME.dim)
        }),
        Rect::new(x, inner.y, width, 1),
    );
    let aloud_highlight = reader.aloud_highlight();
    let lines: Vec<Line> = reader
        .lines
        .iter()
        .enumerate()
        .skip(reader.offset)
        .take(content.height as usize)
        .map(|(row, line)| {
            let heading = !line.is_empty()
                && reader.real.as_ref().is_some_and(|real| {
                    real.positions.get(row).is_some_and(|position| {
                        real.chapters
                            .iter()
                            .any(|c| c.body > c.start && *position >= c.start && *position < c.body)
                    })
                });
            if heading {
                Line::from(line.as_str()).centered().style(
                    ratatui::style::Style::default().add_modifier(
                        ratatui::style::Modifier::BOLD | ratatui::style::Modifier::UNDERLINED,
                    ),
                )
            } else {
                Line::from(line.as_str())
            }
        })
        .collect();
    use ratatui::style::{Color, Style};
    let (mut bg, mut fg) = match reader.options.0[0] {
        1 => (Color::Rgb(20, 32, 25), Color::Rgb(188, 217, 180)),
        2 => (Color::Rgb(232, 220, 194), Color::Rgb(58, 47, 34)),
        3 => (Color::Black, Color::White),
        _ => (THEME.bg, THEME.fg),
    };
    let mut style = Style::default();
    if let Some(layout) = &reader.layout {
        if let Some([r, g, b]) = layout.foreground {
            fg = Color::Rgb(r, g, b);
        }
        if let Some([r, g, b]) = layout.background {
            bg = Color::Rgb(r, g, b);
        }
        if layout.bold {
            style = style.add_modifier(ratatui::style::Modifier::BOLD);
        }
    }
    f.render_widget(Paragraph::new(lines).style(style.fg(fg).bg(bg)), content);
    if let Some(segment) = aloud_highlight {
        let highlight = aloud_paragraph_style(bg);
        let start = segment.line_start.max(reader.offset);
        let end = segment
            .line_end
            .min(reader.offset + content.height as usize);
        for row in start..end {
            let y = content.y + (row - reader.offset) as u16;
            // Paint the full paragraph width, including line spacing and trailing cells.
            // The marker uses the existing margin so text wrapping and offsets stay stable.
            f.buffer_mut()
                .set_style(Rect::new(content.x, y, content.width, 1), highlight);
            f.render_widget(
                Paragraph::new(if row == start { "▶" } else { "┃" }).style(sb(THEME.accent)),
                Rect::new(content.x - 2, y, 1, 1),
            );
        }
    }
    let page = (reader.offset + reader.height).div_ceil(reader.height);
    let pages = reader.lines.len().div_ceil(reader.height).max(1);
    let at_end = reader.offset == reader.max_offset();
    // The preference hides routine page progress, while chapter boundaries
    // remain visible so automatic chapter changes are never ambiguous.
    if reader.options.0[5] == 1 && !at_end && !reader.auto_active() && !reader.aloud_active() {
        return;
    }
    let status = if reader.aloud_active() {
        format!(
            "第 {}/{} · 第 {}/{} 段",
            reader.chapter + 1,
            reader.book.total,
            reader.aloud.segment + 1,
            reader.aloud_segments().len()
        )
    } else if at_end && reader.is_horizontal() {
        format!(
            "第 {}/{} · {}/{} 页 · {}",
            reader.chapter + 1,
            reader.book.total,
            page,
            pages,
            if reader.chapter + 1 < reader.book.total {
                "本章完"
            } else {
                "全书完"
            },
        )
    } else if at_end {
        if reader.chapter + 1 < reader.book.total {
            format!(
                "╰─ 本章完 · ] 下一章 · 第 {}/{} ─╯",
                reader.chapter + 1,
                reader.book.total
            )
        } else {
            format!(
                "╰─ 全书完 · 第 {}/{} ─╯",
                reader.chapter + 1,
                reader.book.total
            )
        }
    } else if !reader.purify_errors.is_empty() {
        format!(
            "第 {}/{} · {}/{} 页 · 净化失败，保留原文",
            reader.chapter + 1,
            reader.book.total,
            page,
            pages
        )
    } else {
        format!(
            "第 {}/{} · {}/{} 页",
            reader.chapter + 1,
            reader.book.total,
            page,
            pages
        )
    };
    let status = if reader.auto_active() {
        let width = 16usize;
        let filled = (reader.auto_progress() * width as f64).round() as usize;
        let bar = format!(
            "{}{}",
            "━".repeat(filled.min(width)),
            "·".repeat(width.saturating_sub(filled))
        );
        format!(
            "▶ {bar} 自动{} · {status}",
            if reader.is_horizontal() {
                "翻页"
            } else {
                "滚动"
            }
        )
    } else {
        status
    };
    f.render_widget(
        Paragraph::new(status)
            .style(if at_end {
                sb(THEME.accent)
            } else {
                s(THEME.dim)
            })
            .centered(),
        Rect::new(x, inner.bottom() - 1, width, 1),
    );
}

fn aloud_paragraph_style(background: ratatui::style::Color) -> ratatui::style::Style {
    use ratatui::style::{Color, Modifier};
    let light_background = match background {
        Color::Rgb(r, g, b) => {
            u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 >= 128_000
        }
        Color::White | Color::Gray => true,
        _ => false,
    };
    let (fg, bg) = if light_background {
        (Color::Rgb(255, 246, 222), Color::Rgb(75, 45, 14))
    } else {
        (Color::Rgb(28, 20, 11), THEME.accent)
    };
    sbg(fg, bg).add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod horizontal_tests {
    use super::*;

    fn reader(text: &str, width: u16, height: u16) -> Reader {
        let mut book = crate::data::shelf().remove(0);
        book.read = 0;
        book.total = 3;
        let mut reader = Reader::new(book, false);
        reader.options.0[4] = 1;
        reader.options.0[8] = 1;
        reader.layout = Some(
            crate::plugins::parse_layout(
                r#"{"paragraphIndent":"","lineSpacingExtra":0,"paragraphSpacing":0}"#,
            )
            .unwrap(),
        );
        reader.set_real(
            vec!["第一章".into(), "第二章".into(), "第三章".into()],
            text.into(),
            0,
        );
        reader.prepare(width, height);
        reader
    }

    #[test]
    fn every_page_contains_complete_unicode_text_without_overlap_or_loss() {
        let raw = "A中😀e\u{301}B".repeat(83);
        for width in [8, 17, 44] {
            for height in [1, 3, 10] {
                let mut reader = reader(&raw, width, height);
                let pages = reader.lines.len().div_ceil(height as usize);
                assert_eq!(reader.max_offset(), (pages - 1) * height as usize);
                let mut displayed = String::new();
                for page in 0..pages {
                    assert_eq!(reader.offset, page * height as usize);
                    for line in reader
                        .lines
                        .iter()
                        .skip(reader.offset)
                        .take(height as usize)
                    {
                        assert!(
                            unicode_width::UnicodeWidthStr::width(line.as_str()) <= width as usize
                        );
                        displayed.push_str(line);
                    }
                    assert!(reader.request.is_none());
                    if page + 1 < pages {
                        reader.page(true);
                    }
                }
                assert_eq!(displayed, raw);
                for page in (0..pages.saturating_sub(1)).rev() {
                    reader.page(false);
                    assert_eq!(reader.offset, page * height as usize);
                }
                assert!(reader.request.is_none());
            }
        }
    }

    #[test]
    fn exact_and_partial_pages_do_not_create_blank_pages() {
        for (length, pages) in [(0, 1), (1, 1), (39, 1), (40, 1), (41, 2), (80, 2), (81, 3)] {
            let raw = "a".repeat(length);
            let mut reader = reader(&raw, 10, 4);
            reader.layout = None;
            reader.options.0[3] = 0;
            reader.refresh_layout();
            reader.prepare(10, 4);
            assert_eq!(reader.lines.len().div_ceil(4).max(1), pages);
            assert_eq!(reader.max_offset(), (pages - 1) * 4);
            assert_eq!(reader.lines.join(""), raw);
        }
    }

    #[test]
    fn arrows_respect_direction_but_logical_next_page_is_always_forward() {
        for direction in [0, 1] {
            let mut reader = reader(&"正文内容。".repeat(150), 20, 5);
            reader.options.0[9] = direction;
            reader.page(true);
            assert_eq!(reader.offset, 5);
            reader.horizontal_arrow(direction == 0);
            assert_eq!(reader.offset, 10);
            reader.horizontal_arrow(direction != 0);
            assert_eq!(reader.offset, 5);
            reader.page(false);
            assert_eq!(reader.offset, 0);
            reader.page(false);
            assert_eq!(reader.chapter, 0);
            assert!(reader.request.is_none());
            reader.scroll(1);
            reader.scroll(-1);
            assert_eq!(reader.offset, 0);
        }
    }

    #[test]
    fn chapter_navigation_waits_for_load_and_previous_chapter_opens_last_page() {
        let mut reader = reader(&"一".repeat(153), 20, 5);
        reader.offset = reader.max_offset();
        let last = reader.offset;
        reader.page(true);
        assert_eq!(reader.request.take(), Some((1, false)));
        assert_eq!((reader.chapter, reader.offset), (0, last));
        reader.set_chapter(1, "二".repeat(71), false);
        reader.prepare(20, 5);
        assert_eq!((reader.chapter, reader.offset), (1, 0));
        assert_eq!(reader.lines.join(""), "二".repeat(71));
        reader.page(false);
        assert_eq!(reader.request.take(), Some((0, true)));
        assert_eq!((reader.chapter, reader.offset), (1, 0));
        reader.set_chapter(0, "一".repeat(153), true);
        reader.prepare(20, 5);
        assert_eq!((reader.chapter, reader.offset), (0, reader.max_offset()));
        assert_eq!(reader.offset % 5, 0);
        assert_eq!(reader.lines.join(""), "一".repeat(153));
    }

    #[test]
    fn resize_and_restart_keep_the_saved_character_on_the_visible_page() {
        let raw = "中😀English正文内容。".repeat(200);
        let mut reader = reader(&raw, 44, 12);
        reader.page(true);
        reader.page(true);
        for (width, height) in [(28, 12), (28, 5), (60, 9), (17, 3)] {
            let position = reader.position();
            reader.prepare(width, height);
            let start = reader.position();
            let end = reader
                .real
                .as_ref()
                .unwrap()
                .positions
                .get(reader.offset + height as usize)
                .copied()
                .unwrap_or(raw.chars().count());
            assert!(start <= position && position <= end);
            assert_eq!(reader.offset % height as usize, 0);
            assert!(reader.request.is_none());
            let mut restored = self::reader(&raw, width, height);
            restored.set_real(
                reader.real.as_ref().unwrap().titles.clone(),
                raw.clone(),
                start,
            );
            restored.prepare(width, height);
            assert_eq!(restored.offset, reader.offset);
            assert_eq!(restored.position(), start);
        }
    }

    #[test]
    fn switching_from_joined_vertical_text_keeps_only_the_current_chapter() {
        let mut reader = reader(&"第一章正文".repeat(150), 20, 5);
        reader.options.0[4] = 0;
        reader.set_real(
            vec!["第一章".into(), "第二章".into()],
            "第一章正文".repeat(150),
            0,
        );
        reader.prepare(20, 5);
        reader.set_chapter(1, "第二章正文".repeat(150), false);
        reader.prepare(20, 5);
        reader.page(true);
        let position = reader.position();
        reader.options.0[4] = 1;
        reader.prepare(20, 5);
        assert_eq!(reader.chapter, 1);
        assert_eq!(reader.lines.join(""), "第二章正文".repeat(150));
        assert_eq!(reader.real.as_ref().unwrap().chapters.len(), 1);
        assert!(reader.position() <= position);
        let offset = reader.offset;
        reader.scroll(1);
        assert_eq!(reader.offset, offset);
    }

    #[test]
    fn demo_previous_chapter_lands_on_a_complete_last_page() {
        let mut book = crate::data::shelf().remove(0);
        book.read = 1;
        book.total = 3;
        let mut reader = Reader::new(book, false);
        reader.options.0[4] = 1;
        reader.prepare(28, 7);
        reader.page(false);
        assert_eq!(reader.chapter, 0);
        assert_eq!(reader.offset, reader.max_offset());
        assert_eq!(reader.offset % 7, 0);
        reader.page(true);
        assert_eq!((reader.chapter, reader.offset), (1, 0));
    }

    #[test]
    fn rendered_pages_have_correct_counts_and_show_the_final_text() {
        use ratatui::{backend::TestBackend, Terminal};
        let raw = format!("{}THEEND", "中😀abc".repeat(251));
        for (width, height) in [(80, 20), (36, 12)] {
            let mut reader = reader(&raw, 20, 5);
            reader.options.0[1] = 0;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut displayed = String::new();
            let mut page = 0;
            loop {
                terminal
                    .draw(|f| draw_page(f, &mut reader, f.area(), true))
                    .unwrap();
                let pages = reader.lines.len().div_ceil(reader.height);
                let buffer = terminal.backend().buffer();
                let mut status = String::new();
                for x in 0..width {
                    status.push_str(buffer[(x, height - 2)].symbol());
                }
                assert!(status.contains(&format!("{}/{pages} 页", page + 1)));
                for y in 2..height - 2 {
                    let mut line = String::new();
                    let content_x = (width - reader.width) / 2;
                    let mut x = content_x;
                    while x < content_x + reader.width {
                        let symbol = buffer[(x, y)].symbol();
                        line.push_str(symbol);
                        x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                    }
                    displayed.push_str(line.trim());
                }
                page += 1;
                if page == pages {
                    break;
                }
                reader.page(true);
            }
            assert_eq!(displayed, raw);
        }
    }

    #[test]
    fn auto_tick_uses_page_interval_for_horizontal_mode() {
        let mut reader = reader(&"正文".repeat(300), 20, 5);
        reader.options.0[10] = 4;
        reader.options.0[11] = 0;
        reader.toggle_auto();
        reader.tick_auto(999);
        assert_eq!(reader.offset, 0);
        reader.tick_auto(1);
        assert_eq!(reader.offset, 5);
    }
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

#[cfg(test)]
mod purification_tests {
    use super::*;
    use reader_core::model::replace_rule::ReplaceRule;

    #[test]
    fn imported_layout_renders_colors_indent_and_keeps_raw_titles() {
        use ratatui::{
            backend::TestBackend,
            style::{Color, Modifier},
            Terminal,
        };
        let mut reader = Reader::new(crate::data::shelf().remove(0), false);
        reader.chapter = 0;
        reader.layout = Some(crate::plugins::parse_layout(r##"{"bgStr":"#ffded9c5","textColor":"#ff5b4928","textBold":1,"paragraphIndent":"--","lineSpacingExtra":0,"paragraphSpacing":0}"##).unwrap());
        reader.title_rules = vec![crate::purify::CompiledRule::new(&ReplaceRule {
            name: "标题去广告".into(),
            pattern: "广告".into(),
            scope_title: true,
            ..Default::default()
        })
        .unwrap()];
        reader.set_real(vec!["广告第一章".into()], "中文正文。".repeat(100), 0);
        assert_eq!(reader.title(0), "广告第一章");
        assert_eq!(reader.display_title(0), "第一章");
        for (w, h) in [(120, 40), (80, 24), (40, 12), (2, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| draw_page(frame, &mut reader, frame.area(), true))
                .unwrap();
            if w == 120 {
                assert!(reader.lines[0].starts_with("--"));
                assert!(reader.lines.iter().all(|line| !line.is_empty()));
                assert!(terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .any(|cell| cell.fg == Color::Rgb(91, 73, 40)
                        && cell.bg == Color::Rgb(222, 217, 197)
                        && cell.modifier.contains(Modifier::BOLD)));
            }
        }
        reader.page(true);
        let position = reader.position();
        reader.refresh_layout();
        assert_eq!(reader.position(), position);
    }

    #[test]
    fn purified_pages_restore_original_offsets_when_rules_are_disabled_or_width_changes() {
        let mut reader = Reader::new(crate::data::shelf().remove(0), false);
        reader.options.0[3] = 0;
        reader.live_rules = vec![crate::purify::CompiledRule::new(&ReplaceRule {
            name: "广告".into(),
            pattern: "广告".into(),
            replacement: String::new(),
            is_enabled: true,
            ..Default::default()
        })
        .unwrap()];
        let raw = "广告中文😀正文内容，下一段的文字。".repeat(100);
        reader.set_real(vec!["真实章节".into()], raw.clone(), 0);
        reader.prepare(20, 5);
        assert!(!reader.lines.join("").contains("广告"));
        reader.page(true);
        let position = reader.position();
        assert!(position > 0);
        reader.set_real(vec!["真实章节".into()], raw, position);
        reader.options.0[6] = 1;
        reader.prepare(34, 5);
        assert!(reader.lines.join("").contains("广告"));
        assert!(reader.position() <= position && position - reader.position() < 34);
    }

    #[test]
    fn replacement_expansion_at_start_does_not_skip_first_displayed_lines() {
        let mut reader = Reader::new(crate::data::shelf().remove(0), false);
        reader.options.0[3] = 0;
        reader.live_rules = vec![crate::purify::CompiledRule::new(&ReplaceRule {
            name: "扩展".into(),
            pattern: "头".into(),
            replacement: "第一行第二行第三行第四行".into(),
            is_enabled: true,
            ..Default::default()
        })
        .unwrap()];
        reader.set_real(vec!["正文".into()], "头后续正文".into(), 0);
        reader.prepare(6, 2);
        assert_eq!(reader.offset, 0);
        assert!(reader.lines[0].starts_with("第一行"));
        assert_eq!(reader.position(), 0);
    }
}
