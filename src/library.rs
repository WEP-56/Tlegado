//! Persisted library configuration and keyboard editors. Writes run in jobs.
use crate::{
    backend::{Backend, NAMESPACE},
    demo::{Prefs, PREFS},
};
use anyhow::{bail, Context, Result};
use reader_core::model::{book_group::BookGroup, replace_rule::ReplaceRule};

const RULES: &str = "replace_rules.json";
const PREFERENCES: &str = "tui_preferences.json";

#[derive(Clone, Debug, Default)]
pub struct Library {
    pub groups: Vec<BookGroup>,
    pub rules: Vec<ReplaceRule>,
    pub prefs: Prefs,
    pub layout: Option<crate::plugins::Layout>,
}

pub enum Change {
    RemoveBook(Box<reader_core::model::book::Book>),
    Preference { index: usize, step: isize },
    ResetPreferences,
    SaveGroup { id: i64, name: String },
    DeleteGroup(i64),
    MoveGroup { id: i64, step: isize },
    AssignGroup { book_url: String, id: i64 },
    SaveRule(ReplaceRule),
    ToggleRule(i64),
    DeleteRule(i64),
    MoveRule { id: i64, step: isize },
}

impl Backend {
    pub async fn library(&self) -> Result<Library> {
        let mut groups = self.groups.get_groups(NAMESPACE).await?;
        groups.sort_by_key(|g| (g.order_no, g.group_id));
        let mut rules: Vec<ReplaceRule> = self.documents.read_list(NAMESPACE, RULES).await?;
        rules.sort_by_key(|r| (r.order, r.id));
        for rule in &mut rules {
            crate::purify::validate_fields(rule)?;
            if crate::purify::validate(rule).is_err() {
                rule.is_enabled = false;
            }
        }
        let layout = self
            .documents
            .get_value(NAMESPACE, crate::plugins::LAYOUT)
            .await?
            .filter(|v| !v.is_null())
            .map(serde_json::from_value)
            .transpose()?;
        let prefs = match self.documents.get_value(NAMESPACE, PREFERENCES).await? {
            Some(value) => Prefs(serde_json::from_value(value).context("阅读偏好格式无效")?),
            None => Prefs::default(),
        };
        if prefs
            .0
            .iter()
            .enumerate()
            .any(|(i, &v)| v >= PREFS[i].1.len())
        {
            bail!("阅读偏好包含无效选项，请检查 tui_preferences.json");
        }
        Ok(Library {
            groups,
            rules,
            prefs,
            layout,
        })
    }

    pub async fn change_library(&self, change: Change) -> Result<()> {
        let mut state = self.library().await?;
        match change {
            Change::RemoveBook(book) => self.remove_book(&book).await?,
            Change::Preference { index, step } => {
                let len = PREFS.get(index).context("未知偏好")?.1.len();
                state.prefs.0[index] =
                    (state.prefs.0[index] as isize + step).rem_euclid(len as isize) as usize;
                self.documents
                    .set_value(NAMESPACE, PREFERENCES, &state.prefs.0)
                    .await?;
                if let Some(mut layout) = state.layout {
                    match index {
                        0 => {
                            layout.foreground = None;
                            layout.background = None;
                        }
                        2 => layout.line_gap = None,
                        3 => layout.indent = None,
                        _ => {}
                    }
                    self.documents
                        .set_value(NAMESPACE, crate::plugins::LAYOUT, &layout)
                        .await?;
                }
            }
            Change::ResetPreferences => {
                self.documents
                    .set_value(NAMESPACE, PREFERENCES, &Prefs::default().0)
                    .await?;
                self.documents
                    .set_value(NAMESPACE, crate::plugins::LAYOUT, &serde_json::Value::Null)
                    .await?;
            }
            Change::SaveGroup { id, name } => {
                let name = name.trim();
                if name.is_empty()
                    || name.chars().count() > 60
                    || ["全部", "未分组"].contains(&name)
                {
                    bail!("分组名须为 1–60 字，且不能为全部或未分组");
                }
                if state
                    .groups
                    .iter()
                    .any(|g| g.group_id != id && g.group_name == name)
                {
                    bail!("分组名称已存在");
                }
                if id != 0 && !state.groups.iter().any(|g| g.group_id == id) {
                    bail!("分组已不存在");
                }
                let order_no = state
                    .groups
                    .iter()
                    .find(|g| g.group_id == id)
                    .map_or(state.groups.len() as i32, |g| g.order_no);
                self.groups
                    .save_group(
                        NAMESPACE,
                        BookGroup {
                            group_id: id,
                            group_name: name.into(),
                            order_no,
                        },
                    )
                    .await?;
            }
            Change::DeleteGroup(id) => {
                if self.shelf().await?.iter().any(|b| b.group == Some(id)) {
                    bail!("分组内仍有书籍，请先用 M 移至其他分组或未分组");
                }
                self.groups.delete_group(NAMESPACE, id).await?;
            }
            Change::MoveGroup { id, step } => {
                let index = state
                    .groups
                    .iter()
                    .position(|g| g.group_id == id)
                    .context("分组已不存在")?;
                let next = index
                    .saturating_add_signed(step)
                    .min(state.groups.len() - 1);
                state.groups.swap(index, next);
                for (i, g) in state.groups.iter_mut().enumerate() {
                    g.order_no = i as i32;
                }
                self.groups.save_groups(NAMESPACE, &state.groups).await?;
            }
            Change::AssignGroup { book_url, id } => {
                if id != 0 && !state.groups.iter().any(|g| g.group_id == id) {
                    bail!("分组已不存在");
                }
                let mut book = self
                    .books
                    .get_shelf_book(NAMESPACE, &book_url)
                    .await?
                    .context("书籍已不在书架")?;
                book.group = Some(id);
                self.books.save_book(NAMESPACE, book).await?;
            }
            Change::SaveRule(mut rule) => {
                crate::purify::validate(&rule)?;
                if rule.id == 0 {
                    if state.rules.len() >= 500 {
                        bail!("最多保存 500 条规则");
                    }
                    rule.id = state
                        .rules
                        .iter()
                        .map(|r| r.id)
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .context("规则 ID 已耗尽")?;
                    rule.order = state.rules.len() as i32;
                    state.rules.push(rule);
                } else {
                    let old = state
                        .rules
                        .iter_mut()
                        .find(|r| r.id == rule.id)
                        .context("规则已不存在")?;
                    *old = rule;
                }
                self.documents
                    .write_list(NAMESPACE, RULES, &state.rules)
                    .await?;
            }
            Change::ToggleRule(id) => {
                let rule = state
                    .rules
                    .iter_mut()
                    .find(|r| r.id == id)
                    .context("规则已不存在")?;
                rule.is_enabled = !rule.is_enabled;
                if rule.is_enabled {
                    crate::purify::validate(rule)?;
                }
                self.documents
                    .write_list(NAMESPACE, RULES, &state.rules)
                    .await?;
            }
            Change::DeleteRule(id) => {
                state.rules.retain(|r| r.id != id);
                self.documents
                    .write_list(NAMESPACE, RULES, &state.rules)
                    .await?;
            }
            Change::MoveRule { id, step } => {
                let index = state
                    .rules
                    .iter()
                    .position(|r| r.id == id)
                    .context("规则已不存在")?;
                let next = index.saturating_add_signed(step).min(state.rules.len() - 1);
                state.rules.swap(index, next);
                for (i, r) in state.rules.iter_mut().enumerate() {
                    r.order = i as i32;
                }
                self.documents
                    .write_list(NAMESPACE, RULES, &state.rules)
                    .await?;
            }
        }
        Ok(())
    }
}

use crate::{
    app::{App, Focus, Route},
    jobs::Command,
    theme::{center_rect, list_scroll, panel, s, sb, THEME},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};

pub enum Editor {
    Preview,
    Group(i64),
    Rule(ReplaceRule),
}
pub enum Modal {
    Groups {
        selected: usize,
        book_url: Option<String>,
    },
    Edit {
        editor: Editor,
        fields: Vec<String>,
        active: usize,
        error: String,
    },
    Confirm {
        change: Change,
        label: String,
    },
}

impl App {
    pub fn library_key(&mut self, key: KeyEvent) -> bool {
        let Some(live) = &mut self.live else {
            return false;
        };
        if let Some(mut modal) = live.modal.take() {
            let mut change = None;
            let mut close = key.code == KeyCode::Esc;
            let mut next = None;
            match &mut modal {
                Modal::Edit {
                    editor,
                    fields,
                    active,
                    error,
                } => {
                    let submit = (key.code == KeyCode::Enter && *active + 1 == fields.len())
                        || (key.code == KeyCode::Char('s')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if submit && matches!(editor, Editor::Preview) {
                        live.rule_sample = fields[0].clone();
                        close = true;
                    } else if submit {
                        match editor_change(editor, fields) {
                            Ok(action) => {
                                change = Some(action);
                                close = true;
                            }
                            Err(e) => *error = format!("{e:#}"),
                        }
                    } else {
                        match key.code {
                            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                                *active = (*active + 1) % fields.len()
                            }
                            KeyCode::BackTab | KeyCode::Up => {
                                *active = (*active + fields.len() - 1) % fields.len()
                            }
                            KeyCode::Backspace => {
                                fields[*active].pop();
                            }
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                fields[*active].clear()
                            }
                            KeyCode::Char(c)
                                if !key.modifiers.contains(KeyModifiers::CONTROL)
                                    && fields[*active].len() < 16_384 =>
                            {
                                fields[*active].push(c)
                            }
                            _ => {}
                        }
                    }
                }
                Modal::Confirm { .. } => {
                    close |= matches!(key.code, KeyCode::Char('n' | 'N'));
                    if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                        if let Modal::Confirm { change: action, .. } = modal {
                            change = Some(action);
                        }
                        if let Some(action) = change {
                            if matches!(action, Change::RemoveBook(_)) {
                                self.cancel_live_reads();
                            }
                            self.commands.push(Command::Library(action));
                        }
                        return true;
                    }
                }
                Modal::Groups { selected, book_url } => {
                    let groups = &live.library.groups;
                    *selected = (*selected).min(groups.len());
                    match key.code {
                        KeyCode::Down | KeyCode::Char('j') => {
                            *selected = (*selected + 1).min(groups.len())
                        }
                        KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                        KeyCode::Enter if book_url.is_some() => {
                            change = Some(Change::AssignGroup {
                                book_url: book_url.clone().unwrap(),
                                id: selected
                                    .checked_sub(1)
                                    .and_then(|i| groups.get(i))
                                    .map_or(0, |g| g.group_id),
                            });
                            close = true;
                        }
                        KeyCode::Char('a') => {
                            next = Some(Modal::Edit {
                                editor: Editor::Group(0),
                                fields: vec![String::new()],
                                active: 0,
                                error: String::new(),
                            })
                        }
                        KeyCode::Char('e' | 'x' | 'J' | 'K') if *selected > 0 => {
                            let group = &groups[*selected - 1];
                            match key.code {
                                KeyCode::Char('e') => {
                                    next = Some(Modal::Edit {
                                        editor: Editor::Group(group.group_id),
                                        fields: vec![group.group_name.clone()],
                                        active: 0,
                                        error: String::new(),
                                    })
                                }
                                KeyCode::Char('x') => {
                                    next = Some(Modal::Confirm {
                                        change: Change::DeleteGroup(group.group_id),
                                        label: format!(
                                            "删除分组「{}」？非空分组会拒绝删除。",
                                            group.group_name
                                        ),
                                    })
                                }
                                _ => {
                                    let step = if key.code == KeyCode::Char('J') {
                                        1
                                    } else {
                                        -1
                                    };
                                    change = Some(Change::MoveGroup {
                                        id: group.group_id,
                                        step,
                                    });
                                    *selected =
                                        selected.saturating_add_signed(step).clamp(1, groups.len());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(action) = change {
                self.commands.push(Command::Library(action));
            }
            self.live.as_mut().unwrap().modal = if close {
                None
            } else {
                Some(next.unwrap_or(modal))
            };
            return true;
        }
        if !live.ready
            || live.prompt.is_some()
            || live.picker.is_some()
            || self.help
            || self.reader.is_some()
            || self.focus != Focus::Main
        {
            return false;
        }
        match (self.route().clone(), key.code) {
            (Route::Purify | Route::Prefs, KeyCode::Char('i')) => {
                let kind = if matches!(self.route(), Route::Purify) {
                    crate::live::InputKind::Rules
                } else {
                    crate::live::InputKind::Layout
                };
                self.live.as_mut().unwrap().prompt = Some(crate::live::Input {
                    kind,
                    text: String::new(),
                });
                true
            }
            (
                Route::Prefs,
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l' | ' ') | KeyCode::Enter,
            ) => {
                let step = if matches!(key.code, KeyCode::Left | KeyCode::Char('h')) {
                    -1
                } else {
                    1
                };
                self.commands.push(Command::Library(Change::Preference {
                    index: self.demo.pref_sel,
                    step,
                }));
                true
            }
            (Route::Prefs, KeyCode::Char('R')) => {
                self.commands
                    .push(Command::Library(Change::ResetPreferences));
                true
            }
            (Route::Shelf { filter }, KeyCode::Char('m' | 'M')) => {
                let book_url = if key.code == KeyCode::Char('M') {
                    let Some(book) = self
                        .shelf_list(filter)
                        .get(self.shelf_sel)
                        .and_then(|v| self.live.as_ref().unwrap().domain.get(&v.id))
                    else {
                        return true;
                    };
                    Some(book.book_url.clone())
                } else {
                    None
                };
                self.live.as_mut().unwrap().modal = Some(Modal::Groups {
                    selected: 0,
                    book_url,
                });
                true
            }
            (Route::Purify, code) => {
                let live = self.live.as_mut().unwrap();
                let len = live.library.rules.len();
                self.demo.rule_sel = self.demo.rule_sel.min(len.saturating_sub(1));
                match code {
                    KeyCode::Char('p') => {
                        live.modal = Some(Modal::Edit {
                            editor: Editor::Preview,
                            fields: vec![live.rule_sample.clone()],
                            active: 0,
                            error: String::new(),
                        })
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.demo.rule_sel = (self.demo.rule_sel + 1).min(len.saturating_sub(1))
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.demo.rule_sel = self.demo.rule_sel.saturating_sub(1)
                    }
                    KeyCode::Home | KeyCode::Char('g') => self.demo.rule_sel = 0,
                    KeyCode::End | KeyCode::Char('G') => self.demo.rule_sel = len.saturating_sub(1),
                    KeyCode::Char('a') => {
                        live.modal = Some(rule_editor(ReplaceRule {
                            is_enabled: true,
                            ..Default::default()
                        }))
                    }
                    KeyCode::Char('e' | 'x' | ' ' | 'J' | 'K') | KeyCode::Enter if len > 0 => {
                        let rule = &live.library.rules[self.demo.rule_sel];
                        match code {
                            KeyCode::Char('e') => live.modal = Some(rule_editor(rule.clone())),
                            KeyCode::Char('x') => {
                                live.modal = Some(Modal::Confirm {
                                    change: Change::DeleteRule(rule.id),
                                    label: format!("删除规则「{}」？", rule.name),
                                })
                            }
                            KeyCode::Char('J' | 'K') => {
                                let step = if code == KeyCode::Char('J') { 1 } else { -1 };
                                self.commands
                                    .push(Command::Library(Change::MoveRule { id: rule.id, step }));
                                self.demo.rule_sel =
                                    self.demo.rule_sel.saturating_add_signed(step).min(len - 1);
                            }
                            _ => self
                                .commands
                                .push(Command::Library(Change::ToggleRule(rule.id))),
                        }
                    }
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }
}

fn rule_editor(rule: ReplaceRule) -> Modal {
    let fields = vec![
        rule.name.clone(),
        rule.pattern.clone(),
        rule.replacement.clone(),
        rule.is_regex.to_string(),
        rule.scope.clone().unwrap_or_default(),
    ];
    Modal::Edit {
        editor: Editor::Rule(rule),
        fields,
        active: 0,
        error: String::new(),
    }
}

fn editor_change(editor: &Editor, fields: &[String]) -> Result<Change> {
    match editor {
        Editor::Preview => bail!("预览文本不写入领域配置"),
        Editor::Group(id) => {
            let name = fields[0].trim();
            if name.is_empty() || name.chars().count() > 60 || ["全部", "未分组"].contains(&name)
            {
                bail!("分组名须为 1–60 字，不能为全部或未分组");
            }
            Ok(Change::SaveGroup {
                id: *id,
                name: name.into(),
            })
        }
        Editor::Rule(original) => {
            let mut rule = original.clone();
            rule.name = fields[0].trim().into();
            rule.pattern = fields[1].clone();
            rule.replacement = fields[2].clone();
            rule.is_regex = fields[3]
                .trim()
                .parse()
                .context("正则开关请输入 true 或 false")?;
            rule.scope = (!fields[4].trim().is_empty()).then(|| fields[4].trim().to_string());
            crate::purify::validate(&rule)?;
            Ok(Change::SaveRule(rule))
        }
    }
}

pub fn draw_modal(f: &mut Frame, app: &App, area: Rect) -> bool {
    let Some(live) = &app.live else {
        return false;
    };
    let Some(modal) = &live.modal else {
        return false;
    };
    let rect = center_rect(area, 86, 19);
    f.render_widget(Clear, rect);
    let title = match modal {
        Modal::Groups {
            book_url: Some(_), ..
        } => "选择书籍分组 · Enter 确认",
        Modal::Groups { .. } => "分组管理",
        Modal::Edit {
            editor: Editor::Group(_),
            ..
        } => "编辑分组",
        Modal::Edit {
            editor: Editor::Preview,
            ..
        } => "编辑试算原文",
        Modal::Edit { .. } => "编辑净化规则",
        Modal::Confirm { .. } => "确认删除",
    };
    let block = panel(vec![Span::styled(title, sb(THEME.mag))], None, true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> =
        match modal {
            Modal::Confirm { label, .. } => vec![
                Line::from(label.as_str()),
                Line::from("y 确认 · n/Esc 取消"),
            ],
            Modal::Groups { selected, .. } => {
                let rows: Vec<_> = std::iter::once("未分组".to_string())
                    .chain(live.library.groups.iter().map(|g| g.group_name.clone()))
                    .collect();
                let height = inner.height.saturating_sub(2) as usize;
                let start = list_scroll(*selected, height, rows.len());
                let mut lines = vec![Line::from(
                    "a 新建 · e 重命名 · x 删除 · J/K 排序 · Esc 返回",
                )];
                lines.extend(rows.into_iter().enumerate().skip(start).take(height).map(
                    |(i, row)| {
                        Line::styled(
                            format!("{} {row}", if i == *selected { "›" } else { " " }),
                            if i == *selected {
                                sb(THEME.accent)
                            } else {
                                s(THEME.fg)
                            },
                        )
                    },
                ));
                lines
            }
            Modal::Edit {
                editor,
                fields,
                active,
                error,
            } => {
                let labels: &[&str] = if matches!(editor, Editor::Group(_)) {
                    &["名称"]
                } else if matches!(editor, Editor::Preview) {
                    &["试算原文"]
                } else {
                    &[
                        "名称",
                        "匹配",
                        "替换（空=删除）",
                        "正则 true/false",
                        "范围（空=全部）",
                    ]
                };
                let mut lines = vec![Line::from(
                    "Tab 切换 · Ctrl+U 清空 · Ctrl+S 保存 · Esc 取消",
                )];
                let visible = inner.height.saturating_sub(4).max(1) as usize;
                let start = list_scroll(*active, visible, fields.len());
                for (i, field) in fields.iter().enumerate().skip(start).take(visible) {
                    let display = tail(field, inner.width.saturating_sub(22) as usize);
                    lines.push(Line::styled(
                        format!(
                            "{} {}：{}{}",
                            if i == *active { "›" } else { " " },
                            labels[i],
                            display,
                            if i == *active { "▋" } else { "" }
                        ),
                        if i == *active {
                            sb(THEME.accent)
                        } else {
                            s(THEME.fg)
                        },
                    ));
                }
                if matches!(editor, Editor::Rule(_)) {
                    lines.push(Line::from("范围：逗号分隔的完整书名、书籍 URL 或书源 URL"));
                    lines.push(Line::from("正则：Rust regex；捕获替换 $1 / ${name}"));
                }
                lines.push(Line::styled(error.as_str(), s(THEME.mag)));
                lines
            }
        };
    f.render_widget(Paragraph::new(lines).style(s(THEME.fg)), inner);
    true
}

fn tail(text: &str, width: usize) -> String {
    let mut count = 0;
    let mut chars = Vec::new();
    for c in text.chars().rev() {
        count += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if count > width {
            break;
        }
        chars.push(c);
    }
    chars.into_iter().rev().collect()
}

pub fn draw_rules(f: &mut Frame, app: &App, area: Rect) {
    let live = app.live.as_ref().unwrap();
    let block = panel(
        vec![Span::styled("净化规则 · 自动保存", sb(THEME.hi))],
        None,
        app.focus == Focus::Main,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![Line::from(
        "i 导入 JSON · a 新建 · e 编辑 · 空格 启停 · x 删除 · J/K 排序",
    )];
    let height = inner.height.saturating_sub(8).max(1) as usize;
    let start = list_scroll(app.demo.rule_sel, height, live.library.rules.len());
    for (i, rule) in live
        .library
        .rules
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
    {
        lines.push(Line::styled(
            format!(
                "{} {} {} [{}]",
                if i == app.demo.rule_sel { "›" } else { " " },
                if rule.is_enabled { "●" } else { "○" },
                crate::theme::truncate(&rule.name, inner.width.saturating_sub(14) as usize),
                if rule.is_regex { "正则" } else { "字面" }
            ),
            if i == app.demo.rule_sel {
                sb(THEME.accent)
            } else {
                s(THEME.fg)
            },
        ));
    }
    if live.library.rules.is_empty() {
        lines.push(Line::from("暂无规则 · 按 a 新建"));
    }
    if let Some(rule) = live.library.rules.get(app.demo.rule_sel) {
        if let Err(error) = crate::purify::validate(rule) {
            lines.push(Line::styled(
                format!("不兼容（已停用）：{error:#}"),
                s(THEME.err),
            ));
        }
        let sample = live.rule_sample.as_str();
        let compiled = crate::purify::CompiledRule::new(rule)
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        let (output, _, errors) = crate::purify::apply_report(sample, &compiled);
        for error in errors {
            lines.push(Line::styled(error, s(THEME.err)));
        }
        lines.extend([
            Line::from(""),
            Line::from(format!("匹配：{} → {}", rule.pattern, rule.replacement)),
            Line::from(format!(
                "范围：{} · 正文 {} / 标题 {}",
                rule.scope
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("全部"),
                rule.scope_content,
                rule.scope_title
            )),
            Line::from("p 编辑试算原文；选中规则试算不受启停和范围限制"),
            Line::from(format!("原文：{sample}")),
            Line::from(format!("结果：{output}")),
        ]);
    }
    f.render_widget(
        Paragraph::new(lines)
            .style(s(THEME.fg))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use reader_core::model::book::Book;

    fn rule() -> ReplaceRule {
        ReplaceRule {
            name: "去水印".into(),
            pattern: "水印".into(),
            is_enabled: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn groups_rules_and_preferences_survive_restart_without_losing_book_progress() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        let book = Book {
            name: "测试".into(),
            author: "作者".into(),
            book_url: "https://test.invalid/book".into(),
            origin: "https://test.invalid".into(),
            ..Default::default()
        };
        backend
            .books
            .save_book(NAMESPACE, book.clone())
            .await
            .unwrap();
        backend
            .save_progress(book.clone(), 3, 77, "旧章节".into())
            .await
            .unwrap();
        for name in ["待读", "收藏"] {
            backend
                .change_library(Change::SaveGroup {
                    id: 0,
                    name: name.into(),
                })
                .await
                .unwrap();
        }
        let groups = backend.library().await.unwrap().groups;
        let id = groups[0].group_id;
        backend
            .change_library(Change::AssignGroup {
                book_url: book.book_url.clone(),
                id,
            })
            .await
            .unwrap();
        // An already queued progress snapshot must not undo a later assignment.
        backend
            .save_progress(book.clone(), 4, 90, "新章节".into())
            .await
            .unwrap();
        assert!(backend
            .change_library(Change::DeleteGroup(id))
            .await
            .is_err());
        backend
            .change_library(Change::SaveGroup {
                id,
                name: "稍后阅读".into(),
            })
            .await
            .unwrap();
        backend
            .change_library(Change::MoveGroup { id, step: 1 })
            .await
            .unwrap();
        assert!(backend
            .change_library(Change::SaveGroup {
                id: 0,
                name: "稍后阅读".into()
            })
            .await
            .is_err());
        for _ in 0..2 {
            backend
                .change_library(Change::Preference { index: 0, step: 1 })
                .await
                .unwrap();
        }
        backend
            .change_library(Change::SaveRule(rule()))
            .await
            .unwrap();
        let mut invalid = rule();
        invalid.is_regex = true;
        invalid.pattern = "[".into();
        assert!(backend
            .change_library(Change::SaveRule(invalid))
            .await
            .is_err());
        let backend = Backend::open(temp.path()).await.unwrap();
        let state = backend.library().await.unwrap();
        assert_eq!(state.prefs.0[0], 2);
        assert_eq!(state.groups[1].group_name, "稍后阅读");
        assert_eq!(state.rules.len(), 1);
        let saved = &backend.shelf().await.unwrap()[0];
        assert_eq!(
            (saved.dur_chapter_index, saved.dur_chapter_pos, saved.group),
            (Some(4), Some(90), Some(id))
        );
        backend
            .change_library(Change::ToggleRule(state.rules[0].id))
            .await
            .unwrap();
        assert!(!backend.library().await.unwrap().rules[0].is_enabled);
        backend
            .change_library(Change::DeleteRule(state.rules[0].id))
            .await
            .unwrap();
        backend
            .change_library(Change::ResetPreferences)
            .await
            .unwrap();
        backend
            .change_library(Change::AssignGroup {
                book_url: book.book_url,
                id: 0,
            })
            .await
            .unwrap();
        backend
            .change_library(Change::DeleteGroup(id))
            .await
            .unwrap();
        let state = backend.library().await.unwrap();
        assert!(state.rules.is_empty());
        assert_eq!(state.prefs, Prefs::default());
        assert_eq!(state.groups.len(), 1);
        assert_eq!(backend.shelf().await.unwrap()[0].group, Some(0));
    }

    #[tokio::test]
    async fn rule_reordering_editing_and_corrupt_preferences_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        backend
            .change_library(Change::SaveRule(rule()))
            .await
            .unwrap();
        backend
            .change_library(Change::SaveRule(ReplaceRule {
                name: "第二条".into(),
                ..rule()
            }))
            .await
            .unwrap();
        let mut second = backend.library().await.unwrap().rules[1].clone();
        second.replacement = "替换文本".into();
        backend
            .change_library(Change::SaveRule(second.clone()))
            .await
            .unwrap();
        backend
            .change_library(Change::MoveRule {
                id: second.id,
                step: -1,
            })
            .await
            .unwrap();
        assert_eq!(
            backend.library().await.unwrap().rules[0].replacement,
            "替换文本"
        );
        assert!(backend
            .change_library(Change::Preference {
                index: 100,
                step: 1
            })
            .await
            .is_err());
        backend
            .documents
            .set_value(NAMESPACE, PREFERENCES, &[100, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();
        assert!(backend.library().await.is_err());
        assert_eq!(
            backend
                .documents
                .get_value(NAMESPACE, PREFERENCES)
                .await
                .unwrap()
                .unwrap()[0],
            100
        );
    }

    fn app() -> App {
        let mut app = App::new_live();
        app.live.as_mut().unwrap().ready = true;
        app.apply_live_event(crate::jobs::Event::Library(Library {
            groups: vec![BookGroup {
                group_id: 1,
                group_name: "收藏".into(),
                order_no: 0,
            }],
            rules: vec![ReplaceRule { id: 1, ..rule() }],
            prefs: Prefs::default(),
            layout: None,
        }));
        app
    }
    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    fn render(app: &mut App) {
        for (w, h) in [(120, 40), (80, 24), (40, 12), (2, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
        }
    }

    #[test]
    fn keyboard_forms_validate_and_require_confirmation_and_do_not_mutate_before_ack() {
        let mut app = app();
        app.goto_id_for_test("set:prefs");
        app.focus = Focus::Main;
        let prefs = app.demo.prefs.clone();
        key(&mut app, KeyCode::Char('l'));
        key(&mut app, KeyCode::Char('l'));
        assert_eq!(app.demo.prefs, prefs);
        assert_eq!(app.commands.len(), 2);
        app.goto_id_for_test("shelf:all");
        app.focus = Focus::Main;
        key(&mut app, KeyCode::Char('m'));
        render(&mut app);
        key(&mut app, KeyCode::Char('a'));
        render(&mut app);
        key(&mut app, KeyCode::Enter);
        assert!(
            matches!(&app.live.as_ref().unwrap().modal, Some(Modal::Edit { error, .. }) if !error.is_empty())
        );
        for c in "待读".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Enter);
        assert!(
            matches!(app.commands.last(), Some(Command::Library(Change::SaveGroup { name, .. })) if name == "待读")
        );
        app.goto_id_for_test("set:purify");
        app.focus = Focus::Main;
        render(&mut app);
        key(&mut app, KeyCode::Char('e'));
        render(&mut app);
        if let Some(Modal::Edit { fields, .. }) = &mut app.live.as_mut().unwrap().modal {
            fields[1] = "[".into();
            fields[3] = "true".into();
        }
        app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(
            matches!(&app.live.as_ref().unwrap().modal, Some(Modal::Edit { error, .. }) if error.contains("正则"))
        );
        key(&mut app, KeyCode::Esc);
        let count = app.commands.len();
        key(&mut app, KeyCode::Char('x'));
        render(&mut app);
        key(&mut app, KeyCode::Char('n'));
        assert_eq!(app.commands.len(), count);
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Char('y'));
        assert!(matches!(
            app.commands.last(),
            Some(Command::Library(Change::DeleteRule(1)))
        ));
    }

    #[test]
    fn group_assignment_empty_group_navigation_and_local_import_are_available() {
        let mut app = app();
        let book = Book {
            name: "本地书".into(),
            origin: "local-txt".into(),
            book_url: "local-txt:fixture".into(),
            group: Some(1),
            ..Default::default()
        };
        app.apply_live_event(crate::jobs::Event::Shelf(vec![book]));
        assert_eq!(app.books[0].group, "收藏");
        assert_eq!(app.books[0].kind, crate::data::Kind::Local);
        app.goto_id_for_test("shelf:local");
        app.focus = Focus::Main;
        assert!(app
            .shelf_groups(crate::app::ShelfFilter::Local)
            .contains(&"收藏".into()));
        key(&mut app, KeyCode::Char('M'));
        key(&mut app, KeyCode::Enter);
        assert!(matches!(
            app.commands.last(),
            Some(Command::Library(Change::AssignGroup { id: 0, .. }))
        ));
        key(&mut app, KeyCode::Char('o'));
        for c in "C:/我的书.TXT".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Enter);
        assert!(
            matches!(app.commands.last(), Some(Command::ImportLocal(path)) if path == &std::path::PathBuf::from("C:/我的书.TXT"))
        );
    }
}

#[cfg(test)]
mod view_regressions {
    use super::*;
    use crate::jobs::Event;

    #[test]
    fn preview_editor_uses_typed_text_without_saving_a_rule() {
        let mut app = App::new_live();
        app.live.as_mut().unwrap().ready = true;
        app.goto_id_for_test("set:purify");
        app.focus = Focus::Main;
        app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for c in "实际广告文本😀".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.live.as_ref().unwrap().rule_sample, "实际广告文本😀");
        assert!(app.live.as_ref().unwrap().modal.is_none());
        assert!(app.commands.is_empty());
    }

    #[test]
    fn group_reorder_preserves_current_group_by_id() {
        let mut app = App::new_live();
        let mut library = Library {
            groups: vec![
                BookGroup {
                    group_id: 7,
                    group_name: "甲".into(),
                    order_no: 0,
                },
                BookGroup {
                    group_id: 9,
                    group_name: "乙".into(),
                    order_no: 1,
                },
            ],
            ..Default::default()
        };
        app.apply_live_event(Event::Library(library.clone()));
        app.shelf_group = 2;
        library.groups.swap(0, 1);
        app.apply_live_event(Event::Library(library));
        assert_eq!(app.shelf_group, 3);
        assert_eq!(app.shelf_groups(crate::app::ShelfFilter::All)[3], "甲");
    }
}
