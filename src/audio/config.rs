use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum Config {
    System {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice: Option<String>,
        /// Retain inactive HTTP settings when switching back to system speech.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        http: Option<HttpConfig>,
    },
    Http(HttpConfig),
}

impl Default for Config {
    fn default() -> Self {
        Self::System {
            voice: None,
            http: None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    pub endpoint: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default = "wav")]
    pub response_format: String,
    #[serde(default = "audio_response")]
    pub response_mode: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub header_env: BTreeMap<String, String>,
    #[serde(default)]
    pub body_template: Option<Value>,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "retries")]
    pub retries: u8,
    #[serde(default = "cache_mb")]
    pub cache_max_mb: u64,
    #[serde(default)]
    pub prefetch: bool,
}
fn wav() -> String {
    "wav".into()
}
fn audio_response() -> String {
    "audio".into()
}
fn timeout() -> u64 {
    45
}
fn retries() -> u8 {
    1
}
fn cache_mb() -> u64 {
    256
}

impl Config {
    pub fn path(data_dir: &Path, explicit: Option<&Path>) -> Option<std::path::PathBuf> {
        explicit
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("TLEGADO_TTS_CONFIG").map(Into::into))
            .or_else(|| (!data_dir.as_os_str().is_empty()).then(|| data_dir.join("tts.json")))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(self).context("无法编码听书配置")?;
        Self::parse(&bytes)?;
        if bytes.len() > 65_536 {
            bail!("听书配置文件不能超过 64 KiB");
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).context("无法创建听书配置目录")?;
        let mut file =
            tempfile::NamedTempFile::new_in(parent).context("无法创建听书配置临时文件")?;
        file.write_all(&bytes).context("无法写入听书配置")?;
        file.as_file().sync_all().context("无法同步听书配置")?;
        file.persist(path)
            .map_err(|_| anyhow::anyhow!("无法替换听书配置文件；原配置未修改"))?;
        Ok(())
    }

    pub fn load(data_dir: &Path, explicit: Option<&Path>) -> Result<Self> {
        let environment = std::env::var_os("TLEGADO_TTS_CONFIG").map(std::path::PathBuf::from);
        let default = data_dir.join("tts.json");
        let path = if let Some(path) = explicit.or(environment.as_deref()) {
            path
        } else if !data_dir.as_os_str().is_empty() && default.exists() {
            &default
        } else {
            return Ok(Self::default());
        };
        let file = std::fs::File::open(path).context("无法打开听书配置文件")?;
        let mut bytes = Vec::new();
        file.take(65_537)
            .read_to_end(&mut bytes)
            .context("无法读取听书配置文件")?;
        if bytes.len() > 65_536 {
            bail!("听书配置文件不能超过 64 KiB");
        }
        Self::parse(&bytes)
    }

    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        // Do not reflect JSON values: malformed config may contain a pasted credential.
        let config: Self = serde_json::from_slice(bytes).map_err(|error| {
            anyhow::anyhow!(
                "听书配置 JSON 无效或含未知字段（行 {}，列 {}）",
                error.line(),
                error.column()
            )
        })?;
        if let Self::Http(http) = &config {
            http.validate()?;
        }
        Ok(config)
    }
}

impl HttpConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if self.api_key.is_some() && self.api_key_env.is_some() {
            bail!("API 密钥与密钥环境变量只能填写一项");
        }
        if let Some(key) = &self.api_key {
            if key.trim().is_empty()
                || reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")).is_err()
            {
                bail!("API 密钥为空或含无效请求头字符");
            }
        }
        for name in self.api_key_env.iter().chain(self.header_env.values()) {
            if name.is_empty()
                || name.len() > 128
                || !name.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte.is_ascii_alphabetic()
                        || (index > 0 && byte.is_ascii_digit())
                })
            {
                bail!("TTS 密钥字段须填写环境变量名称（字母、数字、下划线），不要填写密钥本身");
            }
        }
        let url = reqwest::Url::parse(&self.endpoint)
            .map_err(|_| anyhow::anyhow!("TTS endpoint 必须是完整 HTTP(S) URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            bail!("TTS endpoint 必须是无用户名、密码和片段的 HTTP(S) URL");
        }
        if !matches!(
            self.response_format.as_str(),
            "wav" | "mp3" | "flac" | "ogg"
        ) {
            bail!("TTS response_format 支持 wav/mp3/flac/ogg；不支持裸 PCM 或 JSON/base64 音频");
        }
        if !matches!(self.response_mode.as_str(), "audio" | "json_base64") {
            bail!("TTS response_mode 只支持 audio 或 json_base64");
        }
        if !(1..=120).contains(&self.timeout_seconds)
            || self.retries > 2
            || self.cache_max_mb > 1024
        {
            bail!("TTS timeout_seconds 须为 1–120，retries 为 0–2，cache_max_mb 为 0–1024");
        }
        if let Some(body) = &self.body_template {
            fn contains_text(value: &Value) -> bool {
                match value {
                    Value::String(text) => text.contains("{{text}}"),
                    Value::Array(items) => items.iter().any(contains_text),
                    Value::Object(items) => items.values().any(contains_text),
                    _ => false,
                }
            }
            if !body.is_object() || !contains_text(body) {
                bail!("TTS body_template 必须是包含 {{text}} 占位符的 JSON 对象");
            }
        } else if self.model.trim().is_empty() || self.voice.trim().is_empty() {
            bail!("兼容模式必须配置 model 和 voice");
        }
        for name in self.headers.keys().chain(self.header_env.keys()) {
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| anyhow::anyhow!("TTS 配置含无效请求头名称"))?;
            if ["host", "content-length", "transfer-encoding", "connection"]
                .iter()
                .any(|blocked| name.eq_ignore_ascii_case(blocked))
            {
                bail!("TTS 配置不能覆盖 HTTP 传输控制请求头");
            }
        }
        if self.headers.keys().any(|name| {
            [
                "authorization",
                "proxy-authorization",
                "api-key",
                "x-api-key",
                "cookie",
            ]
            .iter()
            .any(|secret| name.eq_ignore_ascii_case(secret))
        }) {
            bail!("认证请求头请使用 api_key_env 或 header_env，不能把密钥直接写进 headers");
        }
        for value in self.headers.values() {
            reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| anyhow::anyhow!("TTS 请求头值无效"))?;
        }
        Ok(())
    }

    pub(super) fn request_headers(&self) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        for (name, value) in &self.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(value).map_err(|_| anyhow::anyhow!("TTS 请求头值无效"))?,
            );
        }
        let credential = match &self.api_key_env {
            Some(name) => Some(secret(name)?),
            None => self.api_key.clone(),
        };
        if let Some(value) = credential {
            let mut value = HeaderValue::from_str(&format!("Bearer {value}"))
                .map_err(|_| anyhow::anyhow!("TTS 密钥不适合作为请求头"))?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        for (header, environment) in &self.header_env {
            let mut value = HeaderValue::from_str(&secret(environment)?)
                .map_err(|_| anyhow::anyhow!("TTS 密钥请求头值无效"))?;
            value.set_sensitive(true);
            headers.insert(HeaderName::from_bytes(header.as_bytes())?, value);
        }
        Ok(headers)
    }

    pub(super) fn body(&self, text: &str, speed: u16) -> Value {
        let Some(template) = &self.body_template else {
            return serde_json::json!({"model": self.model, "voice": self.voice, "input": text, "response_format": self.response_format, "speed": f64::from(speed) / 100.0});
        };
        fn render(value: &Value, replacements: &[(&str, Value)]) -> Value {
            match value {
                Value::String(string) => {
                    if let Some((_, value)) = replacements.iter().find(|(token, _)| string == token)
                    {
                        return value.clone();
                    }
                    let mut result = string.clone();
                    // Replace original template tokens once; do not interpret tokens in book text.
                    let mut output = String::new();
                    while let Some((index, token, replacement)) = replacements
                        .iter()
                        .filter_map(|(token, value)| {
                            result.find(token).map(|index| (index, *token, value))
                        })
                        .min_by_key(|(index, _, _)| *index)
                    {
                        output.push_str(&result[..index]);
                        output.push_str(replacement.as_str().unwrap_or(""));
                        if !replacement.is_string() {
                            output.push_str(&replacement.to_string());
                        }
                        result = result[index + token.len()..].to_string();
                    }
                    output.push_str(&result);
                    Value::String(output)
                }
                Value::Array(values) => {
                    Value::Array(values.iter().map(|v| render(v, replacements)).collect())
                }
                Value::Object(values) => Value::Object(
                    values
                        .iter()
                        .map(|(k, v)| (k.clone(), render(v, replacements)))
                        .collect(),
                ),
                _ => value.clone(),
            }
        }
        render(
            template,
            &[
                ("{{text}}", text.into()),
                ("{{model}}", self.model.clone().into()),
                ("{{voice}}", self.voice.clone().into()),
                ("{{format}}", self.response_format.clone().into()),
                ("{{speed}}", (f64::from(speed) / 100.0).into()),
            ],
        )
    }
}

fn secret(name: &str) -> Result<String> {
    // Only variable names can appear in errors, never their values.
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .with_context(|| format!("请设置 TTS 密钥环境变量 {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_config_and_cli_paths_are_loaded_without_changing_demo_defaults() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("voice config.json");
        std::fs::write(&file, br#"{"backend":"system"}"#).unwrap();
        assert!(matches!(
            Config::load(root.path(), Some(&file)).unwrap(),
            Config::System { .. }
        ));
        let options = crate::backend::Options::parse([
            "--demo".into(),
            "--tts-config".into(),
            file.clone().into_os_string(),
        ])
        .unwrap();
        assert_eq!(options.tts_config, Some(file));
        assert!(options.demo && options.data_dir.as_os_str().is_empty());
        assert!(crate::backend::Options::parse(["--tts-config".into()]).is_err());
    }
    #[test]
    fn config_defaults_templates_and_validation_are_explicit() {
        let Config::Http(config) = Config::parse(br#"{"backend":"http","endpoint":"http://localhost:8000/speech","model":"custom","voice":"voice"}"#).unwrap() else { panic!() };
        assert_eq!(config.timeout_seconds, 45);
        assert!(!config.prefetch);
        assert_eq!(
            config.body("引号\"换行\n{{voice}}", 125)["input"],
            "引号\"换行\n{{voice}}"
        );
        assert_eq!(config.body("正文", 125)["speed"], 1.25);
        let mut template = config.clone();
        template.body_template = Some(
            serde_json::json!({"nested": {"text": "前缀{{text}}后缀", "rate": "{{speed}}", "voice": "{{voice}}"}}),
        );
        let body = template.body("{{voice}}\"\n文本", 150);
        assert_eq!(body["nested"]["text"], "前缀{{voice}}\"\n文本后缀");
        assert_eq!(body["nested"]["rate"], 1.5);
        for json in [
            r#"{"backend":"http","endpoint":"file:///tmp/x","model":"m","voice":"v"}"#,
            r#"{"backend":"http","endpoint":"https://user:password@example.com","model":"m","voice":"v"}"#,
            r#"{"backend":"http","endpoint":"https://example.com","model":"m","voice":"v","response_format":"pcm"}"#,
            r#"{"backend":"http","endpoint":"https://example.com","model":"m","voice":"v","retries":3}"#,
            r#"{"backend":"http","endpoint":"https://example.com","model":"m","voice":"v","headers":{"Authorization":"SECRET"}}"#,
            r#"{"backend":"http","endpoint":"https://example.com","model":"m","voice":"v","api_key":"SECRET","api_key_env":"KEY"}"#,
        ] {
            let error = Config::parse(json.as_bytes())
                .err()
                .expect("invalid config must fail")
                .to_string();
            assert!(!error.contains("SECRET") && !error.contains("password"));
        }
        assert!(matches!(
            Config::parse(br#"{"backend":"system"}"#).unwrap(),
            Config::System { .. }
        ));
        let custom = Config::parse(
            br#"{"backend":"http","endpoint":"https://example.com/v1/text-to-speech/id","header_env":{"xi-api-key":"ELEVEN_KEY"},"body_template":{"text":"{{text}}"}}"#,
        )
        .unwrap();
        assert!(
            matches!(custom, Config::Http(ref http) if http.model.is_empty() && http.voice.is_empty())
        );
    }

    #[test]
    fn credentials_are_environment_only_and_marked_sensitive() {
        const NAME: &str = "TLEGADO_UNIT_HTTP_FAKE_SECRET_73522";
        std::env::set_var(NAME, "fake-secret-for-tests");
        let Config::Http(config) = Config::parse(format!(r#"{{"backend":"http","endpoint":"https://example.com","model":"m","voice":"v","api_key_env":"{NAME}","header_env":{{"x-api-key":"{NAME}"}}}}"#).as_bytes()).unwrap() else { panic!() };
        let headers = config.request_headers().unwrap();
        std::env::remove_var(NAME);
        assert_eq!(headers["authorization"], "Bearer fake-secret-for-tests");
        assert!(headers["authorization"].is_sensitive() && headers["x-api-key"].is_sensitive());
        assert!(!format!("{headers:?}").contains("fake-secret-for-tests"));
        assert!(config.request_headers().is_err());
    }
}
