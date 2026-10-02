//! TUI form for speech configuration. Drafts never affect the running backend.
use super::config::{Config, HttpConfig};
use crate::app::{App, Focus, Route};
use crate::theme::{panel, s, sb, truncate, THEME};
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::path::PathBuf;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Backend,
    SystemVoice,
    Endpoint,
    Model,
    Voice,
    Key,
    KeyEnv,
    Format,
    ResponseMode,
    Timeout,
    Retries,
    Cache,
    Prefetch,
    Headers,
    HeaderEnv,
    Body,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Backend => "听书后端",
            Self::SystemVoice => "系统声音（留空自动选择）",
            Self::Endpoint => "接口地址（完整 POST URL）",
            Self::Model => "模型",
            Self::Voice => "声音",
            Self::Key => "API 密钥（遮罩显示，本地明文保存）",
            Self::KeyEnv => "密钥环境变量（与 API 密钥二选一）",
            Self::Format => "音频格式",
            Self::ResponseMode => "响应模式",
            Self::Timeout => "请求超时 / 秒（1–120）",
            Self::Retries => "额外重试次数（0–2）",
            Self::Cache => "缓存预算 / MiB（0–1024，0 关闭）",
            Self::Prefetch => "预取下一片段",
            Self::Headers => "静态请求头 / JSON 对象",
            Self::HeaderEnv => "请求头 → 环境变量 / JSON 对象",
            Self::Body => "自定义请求体 / JSON（留空使用兼容请求）",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Backend => "Enter 或左右键切换系统语音 / HTTP；Ctrl+S 保存并立即生效。",
            Self::SystemVoice => "填写已安装的声音名。Windows 使用完整 SAPI 名称；留空沿用环境变量或自动选择。",
            Self::Endpoint => "填写服务商完整语音合成地址，包括路径；响应须为音频字节。",
            Self::Key => "密钥输入始终遮罩。保存到下方配置文件，未加密；不愿落盘可使用密钥环境变量。Ctrl+U 清空。",
            Self::KeyEnv => "这里只填写变量名，如 TLEGADO_TTS_API_KEY；也可直接在上一项填写密钥。",
            Self::Prefetch => "预取可能产生未收听内容的费用；默认关闭。",
            Self::Headers => "例如 {\"accept\":\"audio/wav\"}。认证值请使用密钥或请求头环境变量。",
            Self::HeaderEnv => "例如 {\"x-api-key\":\"MY_TTS_KEY\"}，变量值会原样作为请求头发送。",
            Self::Body => "粘贴 JSON 对象，支持 {{text}}、{{model}}、{{voice}}、{{format}}、{{speed}}；必须含 {{text}}。",
            Self::Model | Self::Voice => "标准兼容请求必须填写；使用自定义请求体时可留空，只有服务商实际需要的字段才需填写。",
            Self::Format => "Enter 或左右键切换 wav / mp3 / flac / ogg。",
            Self::ResponseMode => "标准接口选直接音频；MiMo-V2.5-TTS 选 JSON Base64。",
            Self::Timeout | Self::Retries => "只有 HTTP 429/5xx 自动重试；不会自动重试鉴权失败或超时。",
            Self::Cache => "缓存目录为数据目录下 tts-cache；仅缓存通过解码校验的音频。",
        }
    }
}

struct Edit {
    field: Field,
    chars: Vec<char>,
    cursor: usize,
}
impl Edit {
    fn insert(&mut self, text: &str) {
        let available = 32_768usize.saturating_sub(self.chars.len());
        let chars: Vec<_> = text
            .chars()
            .filter_map(|ch| {
                if matches!(ch, '\n' | '\r' | '\t') {
                    Some(' ')
                } else if ch.is_control() {
                    None
                } else {
                    Some(ch)
                }
            })
            .take(available)
            .collect();
        let count = chars.len();
        self.chars.splice(self.cursor..self.cursor, chars);
        self.cursor += count;
    }
    fn visible(&self, width: usize) -> String {
        let mut chars = self.chars.clone();
        if self.field == Field::Key {
            chars.fill('•');
        }
        chars.insert(self.cursor, '▋');
        let mut start = self.cursor;
        let mut used = 1;
        while start > 0 {
            let next = chars[start - 1].width().unwrap_or(0);
            if used + next > width.saturating_sub(1) {
                break;
            }
            used += next;
            start -= 1;
        }
        truncate(&chars[start..].iter().collect::<String>(), width)
    }
}

pub struct Settings {
    saved: Config,
    http: HttpConfig,
    system_voice: String,
    network: bool,
    pub path: Option<PathBuf>,
    pub pending: Option<Config>,
    selected: usize,
    edit: Option<Edit>,
    dirty: bool,
    pub message: String,
    pub error: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self::new(Config::default(), None)
    }
}
impl Settings {
    pub fn new(config: Config, path: Option<PathBuf>) -> Self {
        let http = match &config {
            Config::Http(http) => http.clone(),
            Config::System {
                http: Some(http), ..
            } => http.clone(),
            _ => serde_json::from_value(serde_json::json!({"endpoint":""})).expect("HTTP defaults"),
        };
        let system_voice = match &config {
            Config::System { voice, .. } => voice.clone().unwrap_or_default(),
            _ => String::new(),
        };
        Self {
            network: matches!(&config, Config::Http(_)),
            saved: config,
            http,
            system_voice,
            path,
            pending: None,
            selected: 0,
            edit: None,
            dirty: false,
            message: String::new(),
            error: false,
        }
    }
    fn fields(&self) -> Vec<Field> {
        if self.network {
            vec![
                Field::Backend,
                Field::Endpoint,
                Field::Model,
                Field::Voice,
                Field::Key,
                Field::KeyEnv,
                Field::Format,
                Field::ResponseMode,
                Field::Timeout,
                Field::Retries,
                Field::Cache,
                Field::Prefetch,
                Field::Headers,
                Field::HeaderEnv,
                Field::Body,
            ]
        } else {
            vec![Field::Backend, Field::SystemVoice]
        }
    }
    fn value(&self, field: Field) -> String {
        match field {
            Field::Backend => if self.network {
                "HTTP 网络语音"
            } else {
                "系统语音"
            }
            .into(),
            Field::SystemVoice => self.system_voice.clone(),
            Field::Endpoint => self.http.endpoint.clone(),
            Field::Model => self.http.model.clone(),
            Field::Voice => self.http.voice.clone(),
            Field::Key => self.http.api_key.clone().unwrap_or_default(),
            Field::KeyEnv => self.http.api_key_env.clone().unwrap_or_default(),
            Field::Format => self.http.response_format.clone(),
            Field::ResponseMode => self.http.response_mode.clone(),
            Field::Timeout => self.http.timeout_seconds.to_string(),
            Field::Retries => self.http.retries.to_string(),
            Field::Cache => self.http.cache_max_mb.to_string(),
            Field::Prefetch => if self.http.prefetch {
                "开启"
            } else {
                "关闭"
            }
            .into(),
            Field::Headers => serde_json::to_string(&self.http.headers).unwrap(),
            Field::HeaderEnv => serde_json::to_string(&self.http.header_env).unwrap(),
            Field::Body => self
                .http
                .body_template
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        }
    }
    fn set(&mut self, field: Field, text: &str) -> Result<()> {
        let text = text.trim();
        let optional = || (!text.is_empty()).then(|| text.to_owned());
        match field {
            Field::SystemVoice => self.system_voice = text.into(),
            Field::Endpoint => self.http.endpoint = text.into(),
            Field::Model => self.http.model = text.into(),
            Field::Voice => self.http.voice = text.into(),
            Field::Key => self.http.api_key = optional(),
            Field::KeyEnv => self.http.api_key_env = optional(),
            Field::ResponseMode => {
                if !matches!(text, "audio" | "json_base64") {
                    bail!("响应模式只能是 audio 或 json_base64");
                }
                self.http.response_mode = text.into();
            }
            Field::Timeout => {
                self.http.timeout_seconds = text
                    .parse()
                    .ok()
                    .filter(|v| (1..=120).contains(v))
                    .context("超时须为 1–120 的整数")?
            }
            Field::Retries => {
                self.http.retries = text
                    .parse()
                    .ok()
                    .filter(|v| *v <= 2)
                    .context("重试次数须为 0–2 的整数")?
            }
            Field::Cache => {
                self.http.cache_max_mb = text
                    .parse()
                    .ok()
                    .filter(|v| *v <= 1024)
                    .context("缓存预算须为 0–1024 的整数")?
            }
            Field::Headers | Field::HeaderEnv => {
                let value = serde_json::from_str(if text.is_empty() { "{}" } else { text })
                    .map_err(|_| anyhow::anyhow!("请求头须为字符串到字符串的 JSON 对象"))?;
                if field == Field::Headers {
                    self.http.headers = value;
                } else {
                    self.http.header_env = value;
                }
            }
            Field::Body => {
                let text = text
                    .strip_prefix("```json")
                    .or_else(|| text.strip_prefix("```JSON"))
                    .or_else(|| text.strip_prefix("```"))
                    .and_then(|body| body.strip_suffix("```"))
                    .map(str::trim)
                    .unwrap_or(text);
                self.http.body_template = if text.is_empty() {
                    None
                } else {
                    let value: serde_json::Value = serde_json::from_str(text)
                        .map_err(|_| anyhow::anyhow!("请求体 JSON 格式无效"))?;
                    if !value.is_object() {
                        bail!("请求体须为 JSON 对象");
                    }
                    Some(value)
                }
            }
            _ => {}
        }
        self.dirty = true;
        Ok(())
    }
    fn commit_edit(&mut self) -> Result<()> {
        if let Some(edit) = &self.edit {
            let field = edit.field;
            let text: String = edit.chars.iter().collect();
            self.set(field, &text)?;
            self.edit = None;
            if self.error {
                self.error = false;
                self.message.clear();
            }
        }
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        self.commit_edit()?;
        let config = if self.network {
            self.http.validate()?;
            Config::Http(self.http.clone())
        } else {
            Config::System {
                voice: (!self.system_voice.is_empty()).then(|| self.system_voice.clone()),
                http: (!self.http.endpoint.is_empty()
                    || self.http.api_key.is_some()
                    || !self.http.model.is_empty())
                .then(|| self.http.clone()),
            }
        };
        if let Some(path) = &self.path {
            config.save(path)?;
        }
        self.saved = config.clone();
        self.pending = Some(config);
        self.error = false;
        self.dirty = false;
        self.message = if self.path.is_some() {
            "已保存，正在应用听书配置"
        } else {
            "已应用到本次运行（演示模式未指定数据目录）"
        }
        .into();
        Ok(())
    }
    fn activate(&mut self, direction: isize) {
        let field = self.fields()[self.selected];
        match field {
            Field::Backend => {
                self.network = !self.network;
                self.dirty = true;
            }
            Field::Prefetch => {
                self.http.prefetch = !self.http.prefetch;
                self.dirty = true;
            }
            Field::Format => {
                let formats = ["wav", "mp3", "flac", "ogg"];
                let current = formats
                    .iter()
                    .position(|v| *v == self.http.response_format)
                    .unwrap_or(0);
                self.http.response_format =
                    formats[(current as isize + direction).rem_euclid(4) as usize].into();
                self.dirty = true;
            }
            Field::ResponseMode => {
                self.http.response_mode = if self.http.response_mode == "json_base64" {
                    "audio".into()
                } else {
                    "json_base64".into()
                };
                self.dirty = true;
            }
            _ => {
                let chars: Vec<_> = self.value(field).chars().collect();
                self.edit = Some(Edit {
                    field,
                    cursor: chars.len(),
                    chars,
                });
            }
        }
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(edit) = &mut self.edit {
            edit.insert(text);
        }
    }
    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }
    fn key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save()?;
            return Ok(false);
        }
        if let Some(edit) = &mut self.edit {
            match key.code {
                KeyCode::Esc => self.edit = None,
                KeyCode::Enter => self.commit_edit()?,
                KeyCode::Tab | KeyCode::BackTab => {
                    self.commit_edit()?;
                    let count = self.fields().len();
                    self.selected = (self.selected
                        + if key.code == KeyCode::BackTab {
                            count - 1
                        } else {
                            1
                        })
                        % count;
                }
                KeyCode::Left => edit.cursor = edit.cursor.saturating_sub(1),
                KeyCode::Right => edit.cursor = (edit.cursor + 1).min(edit.chars.len()),
                KeyCode::Home => edit.cursor = 0,
                KeyCode::End => edit.cursor = edit.chars.len(),
                KeyCode::Backspace if edit.cursor > 0 => {
                    edit.cursor -= 1;
                    edit.chars.remove(edit.cursor);
                }
                KeyCode::Delete if edit.cursor < edit.chars.len() => {
                    edit.chars.remove(edit.cursor);
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    edit.chars.clear();
                    edit.cursor = 0;
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    edit.insert(&ch.to_string())
                }
                _ => {}
            }
            return Ok(false);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Ok(false);
        }
        let count = self.fields().len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Tab => return Ok(true),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(count - 1)
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = count - 1,
            KeyCode::Enter
            | KeyCode::Char('e')
            | KeyCode::Right
            | KeyCode::Char('l')
            | KeyCode::Char(' ') => self.activate(1),
            KeyCode::Left | KeyCode::Char('h') => self.activate(-1),
            KeyCode::Char('R') => {
                let message = "已撤销未保存修改".to_string();
                let pending = self.pending.take();
                *self = Self::new(self.saved.clone(), self.path.clone());
                self.pending = pending;
                self.message = message;
            }
            _ => {}
        }
        Ok(false)
    }
}

impl App {
    pub fn tts_settings_key(&mut self, key: KeyEvent) -> bool {
        if self.reader.is_some()
            || self.help
            || self.focus != Focus::Main
            || !matches!(self.route(), Route::Tts)
        {
            return false;
        }
        if !self.tts.editing()
            && (key.code == KeyCode::Char('?')
                || (key.code == KeyCode::Char('b')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return false;
        }
        match self.tts.key(key) {
            Ok(true) => {
                self.focus = Focus::Sidebar;
                self.sidebar_hidden = false;
                if self.tts.dirty {
                    self.tts.message = "修改尚未保存；返回后 Ctrl+S 保存或 R 撤销".into();
                }
            }
            Ok(false) => {}
            Err(error) => {
                self.tts.error = true;
                self.tts.message = error.to_string();
            }
        }
        true
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let settings = &app.tts;
    let focused = app.focus == Focus::Main;
    let block = panel(
        vec![Span::styled("听书设置", sb(THEME.hi))],
        Some(Span::styled(
            if settings.dirty || settings.edit.is_some() {
                "未保存"
            } else {
                "已保存"
            },
            s(THEME.accent),
        )),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 6 || inner.height < 3 {
        return;
    }
    let compact = inner.height < 16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if compact { 1 } else { 3 }),
        Constraint::Length(1),
        Constraint::Length(if compact { 1 } else { 2 }),
        Constraint::Length(if compact { 1 } else { 2 }),
    ])
    .split(inner);
    let fields = settings.fields();
    let items: Vec<_> = fields
        .iter()
        .map(|field| {
            let value = if let Some(edit) = settings.edit.as_ref().filter(|e| e.field == *field) {
                edit.visible(usize::from(parts[0].width.saturating_sub(4)))
            } else if *field == Field::Key && settings.http.api_key.is_some() {
                "••••••••（已设置）".into()
            } else {
                let text = settings.value(*field);
                if text.is_empty() {
                    "（未填写）".into()
                } else {
                    text.chars()
                        .map(|ch| if ch.is_control() { ' ' } else { ch })
                        .collect()
                }
            };
            ListItem::new(vec![
                Line::styled(field.label(), s(THEME.mute)),
                Line::from(value),
            ])
        })
        .collect();
    let list = List::new(items)
        .style(s(THEME.fg))
        .highlight_style(sb(THEME.hi).bg(THEME.bg3))
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(Some(settings.selected));
    f.render_stateful_widget(list, parts[0], &mut state);
    f.render_widget(
        Paragraph::new(fields[settings.selected].hint())
            .wrap(Wrap { trim: false })
            .style(s(THEME.mute)),
        parts[1],
    );
    let location = settings
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "仅本次运行（未指定数据目录）".into());
    f.render_widget(
        Paragraph::new(format!("保存位置：{location}")).style(s(THEME.dim)),
        parts[2],
    );
    f.render_widget(
        Paragraph::new(settings.message.as_str())
            .wrap(Wrap { trim: false })
            .style(s(if settings.error { THEME.err } else { THEME.ok })),
        parts[3],
    );
    let help = if settings.edit.is_some() {
        "Enter 确认字段 · Esc 取消字段 · Tab 下一项 · Ctrl+U 清空 · Ctrl+S 保存"
    } else {
        "↑↓ 选择 · Enter 编辑/切换 · Ctrl+S 保存生效 · R 撤销 · Esc 返回侧栏"
    };
    f.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: false })
            .style(s(THEME.accent)),
        parts[4],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn app() -> App {
        let mut app = App::new();
        app.nav_idx = app.nav.iter().position(|n| n.id == "set:tts").unwrap();
        app.focus = Focus::Main;
        app
    }
    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    fn valid(settings: &mut Settings) {
        settings.network = true;
        settings.http.endpoint = "http://localhost:8000/speech".into();
        settings.http.model = "test-model".into();
        settings.http.voice = "test-voice".into();
    }
    fn edit(app: &mut App, field: Field) {
        app.tts.selected = app.tts.fields().iter().position(|f| *f == field).unwrap();
        key(app, KeyCode::Enter);
    }

    #[test]
    fn editing_captures_shortcuts_paste_and_unicode_cursor_without_navigation() {
        let mut app = app();
        key(&mut app, KeyCode::Enter); // HTTP backend
        edit(&mut app, Field::Endpoint);
        app.tts.paste("https://example.test/v1/speech");
        for ch in ['?', '/', 'q', 'j', 'k', 'p'] {
            key(&mut app, KeyCode::Char(ch));
        }
        assert!(matches!(app.route(), Route::Tts));
        assert!(!app.help && !app.search_input_mode && !app.should_quit);
        assert!(app.reader.is_none());
        key(&mut app, KeyCode::Esc);
        assert!(app.tts.http.endpoint.is_empty());
        edit(&mut app, Field::Voice);
        app.tts.paste("中文声音");
        key(&mut app, KeyCode::Left);
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Char('语'));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.tts.http.voice, "中文语音");
        assert!(app.tts.pending.is_none());
    }

    #[test]
    fn saving_commits_editor_persists_and_enqueues_new_backend() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested dir/tts.json");
        let mut app = app();
        app.tts.path = Some(path.clone());
        valid(&mut app.tts);
        edit(&mut app, Field::Key);
        app.tts.paste("fake-tui-key");
        app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!app.tts.error && !app.tts.dirty && !app.tts.editing());
        assert!(matches!(&app.tts.pending, Some(Config::Http(_))));
        let Config::Http(http) = Config::load(root.path(), Some(&path)).unwrap() else {
            panic!()
        };
        assert_eq!(http.api_key.as_deref(), Some("fake-tui-key"));
        let headers = http.request_headers().unwrap();
        assert_eq!(headers["authorization"], "Bearer fake-tui-key");
        assert!(headers["authorization"].is_sensitive());
        assert!(!format!("{headers:?}").contains("fake-tui-key"));
        // A second save exercises atomic replacement on Windows as well as creation.
        app.tts.http.voice = "another-voice".into();
        app.tts.save().unwrap();
        let Config::Http(http) = Config::load(root.path(), Some(&path)).unwrap() else {
            panic!()
        };
        assert_eq!(http.voice, "another-voice");
    }

    #[test]
    fn invalid_input_and_io_failure_keep_previous_config_and_draft() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tts.json");
        Config::default().save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut app = app();
        app.tts.path = Some(path.clone());
        valid(&mut app.tts);
        app.tts.http.api_key = Some("fake-secret".into());
        app.tts.http.api_key_env = Some("ENV_KEY".into());
        app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(app.tts.error && app.tts.pending.is_none());
        assert!(!app.tts.message.contains("fake-secret"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        app.tts.http.api_key_env = None;
        edit(&mut app, Field::Timeout);
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        app.tts.paste("999");
        key(&mut app, KeyCode::Enter);
        assert!(app.tts.error && app.tts.editing());
        assert_eq!(app.tts.http.timeout_seconds, 45);
        key(&mut app, KeyCode::Esc);
        app.tts.path = Some(root.path().to_path_buf()); // cannot overwrite a directory
        assert!(app.tts.save().is_err());
        assert!(app.tts.pending.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn backend_switch_preserves_http_settings_across_restart_and_reset_discards_draft() {
        let mut settings = Settings::default();
        valid(&mut settings);
        settings.http.body_template = Some(serde_json::json!({"text":"{{text}}"}));
        settings
            .http
            .header_env
            .insert("x-api-key".into(), "MY_KEY".into());
        settings.activate(1); // system
        settings.save().unwrap();
        let bytes = serde_json::to_vec(&settings.saved).unwrap();
        let mut reloaded = Settings::new(Config::parse(&bytes).unwrap(), None);
        reloaded.activate(1); // HTTP
        assert_eq!(reloaded.http.model, "test-model");
        assert_eq!(reloaded.http.header_env["x-api-key"], "MY_KEY");
        assert!(reloaded.http.body_template.is_some());
        reloaded
            .key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE))
            .unwrap();
        assert!(!reloaded.network && !reloaded.dirty);
        assert_eq!(reloaded.http.model, "test-model");
    }

    #[test]
    fn json_paste_roundtrips_and_errors_do_not_echo_values() {
        let mut app = app();
        valid(&mut app.tts);
        edit(&mut app, Field::Body);
        app.tts
            .paste("{\n\"text\":\"{{text}}\",\n\"rate\":\"{{speed}}\"\n}");
        key(&mut app, KeyCode::Enter);
        assert!(!app.tts.error);
        assert_eq!(app.tts.http.body("中文\"\n", 125)["rate"], 1.25);
        edit(&mut app, Field::Headers);
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        app.tts.paste("{ fake-secret");
        key(&mut app, KeyCode::Enter);
        assert!(app.tts.error && app.tts.editing());
        assert!(!app.tts.message.contains("fake-secret"));
    }

    fn rendered(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for row in buffer.content.chunks(width as usize) {
            let mut x = 0;
            while x < row.len() {
                let symbol = row[x].symbol();
                text.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1);
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn settings_render_masks_secrets_and_scrolls_selected_editor_into_view() {
        let mut app = app();
        valid(&mut app.tts);
        app.tts.http.api_key = Some("NEVER_RENDER_FAKE_SECRET".into());
        edit(&mut app, Field::Key);
        for (width, height) in [(50, 18), (80, 24), (120, 36)] {
            let text = rendered(&mut app, width, height);
            assert!(!text.contains("NEVER_RENDER_FAKE_SECRET"));
            assert!(text.contains('•') && text.contains('▋'));
        }
        key(&mut app, KeyCode::Enter);
        edit(&mut app, Field::Body);
        app.tts.paste(&"中文".repeat(200));
        for (width, height) in [(50, 18), (80, 24), (120, 36)] {
            let text = rendered(&mut app, width, height);
            assert!(
                text.contains("自定义请求体") && text.contains('▋'),
                "{width}x{height}:\n{text}"
            );
            assert!(!text.contains("NEVER_RENDER_FAKE_SECRET"));
        }
        for (width, height) in [(10, 5), (25, 10)] {
            rendered(&mut app, width, height);
        }
    }

    #[test]
    fn demo_save_is_memory_only_and_return_preserves_unsaved_draft() {
        let mut app = app();
        app.sidebar_hidden = true;
        edit(&mut app, Field::SystemVoice);
        app.tts.paste("系统声音测试");
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Esc);
        assert!(app.focus == Focus::Sidebar && !app.sidebar_hidden);
        assert!(app.tts.dirty && app.tts.message.contains("尚未保存"));
        app.focus = Focus::Main;
        app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(app.tts.path.is_none());
        assert!(
            matches!(&app.tts.pending, Some(Config::System { voice: Some(v), .. }) if v == "系统声音测试")
        );
        assert!(app.tts.message.contains("本次运行"));
    }
}
