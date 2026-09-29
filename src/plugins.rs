//! Legado JSON exchange. Import preserves unsupported fields for future compatibility.
use crate::backend::{Backend, NAMESPACE};
use anyhow::{bail, Context, Result};
use reader_core::model::replace_rule::ReplaceRule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const LAYOUT: &str = "legado_read_config.json";
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    pub foreground: Option<[u8; 3]>,
    pub background: Option<[u8; 3]>,
    pub bold: bool,
    pub indent: Option<String>,
    pub line_gap: Option<usize>,
    pub paragraph_gap: Option<usize>,
    pub original: Value,
}

pub fn parse_layout(raw: &str) -> Result<Layout> {
    let value: Value =
        serde_json::from_str(raw.trim_start_matches('﻿')).context("排版 JSON 格式错误")?;
    let value = match value {
        Value::Array(mut items) if items.len() == 1 => items.remove(0),
        value @ Value::Object(_) => value,
        _ => bail!("排版须为单个配置对象或仅含一个配置的数组"),
    };
    let object = value.as_object().context("排版须为对象")?;
    if ![
        "textColor",
        "bgStr",
        "paragraphIndent",
        "lineSpacingExtra",
        "textBold",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
    {
        bail!("未找到 Legado 排版字段，请检查文件类型");
    }
    let color = |key: &str| -> Result<Option<[u8; 3]>> {
        let Some(v) = object.get(key) else {
            return Ok(None);
        };
        let rgb = if let Some(s) = v.as_str() {
            let s = s
                .strip_prefix('#')
                .context("颜色须为 #RRGGBB 或 #AARRGGBB")?;
            if ![6, 8].contains(&s.len()) || !s.is_ascii() {
                bail!("颜色格式无效：{key}");
            }
            u32::from_str_radix(s, 16).context("颜色必须为十六进制")?
        } else if let Some(n) = v.as_i64() {
            if n < i32::MIN as i64 || n > u32::MAX as i64 {
                bail!("颜色超出范围");
            }
            n as u32
        } else {
            bail!("颜色类型无效：{key}");
        };
        Ok(Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]))
    };
    let gap = |key: &str| -> Result<Option<usize>> {
        object
            .get(key)
            .map(|v| {
                let pixels = v.as_f64().context("间距须为数值")?;
                if !(0.0..=1000.0).contains(&pixels) {
                    bail!("间距超出范围：{key}");
                }
                Ok(if pixels == 0.0 {
                    0
                } else {
                    (pixels / 16.0).round().clamp(1.0, 3.0) as usize
                })
            })
            .transpose()
    };
    let indent = object
        .get("paragraphIndent")
        .map(|v| {
            let text = v.as_str().context("段首缩进须为字符串")?;
            if text.chars().count() > 16 || text.chars().any(char::is_control) {
                bail!("段首缩进最多 16 字且不能含控制字符");
            }
            Ok(text.to_owned())
        })
        .transpose()?;
    let bold = match object.get("textBold") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(value) => value.as_i64().context("textBold 须为布尔或整数")? != 0,
    };
    let background = if object.get("bgType").and_then(Value::as_i64).unwrap_or(0) == 0 {
        color("bgStr")?
    } else {
        None
    };
    Ok(Layout {
        name: object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("导入排版")
            .to_owned(),
        foreground: color("textColor")?,
        background,
        bold,
        indent,
        line_gap: gap("lineSpacingExtra")?,
        paragraph_gap: gap("paragraphSpacing")?,
        original: value,
    })
}

async fn read_json(location: &str) -> Result<String> {
    let mut bytes = Vec::new();
    if location.starts_with("https://") || location.starts_with("http://") {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        let mut response = client.get(location).send().await?.error_for_status()?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BYTES as u64)
        {
            bail!("JSON 超过 4 MiB");
        }
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
                bail!("JSON 超过 4 MiB");
            }
            bytes.extend_from_slice(&chunk);
        }
    } else {
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(location)
            .await
            .context("无法打开 JSON 文件")?;
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > MAX_BYTES {
            bail!("JSON 超过 4 MiB");
        }
    }
    String::from_utf8(bytes).context("JSON 必须使用 UTF-8 编码")
}

pub fn parse_rules(raw: &str) -> Result<Vec<ReplaceRule>> {
    let value: Value =
        serde_json::from_str(raw.trim_start_matches('﻿')).context("净化规则 JSON 格式错误")?;
    let items = match value {
        Value::Array(items) => items,
        value @ Value::Object(_) => vec![value],
        _ => bail!("规则须为对象或数组"),
    };
    if items.is_empty() || items.len() > 500 {
        bail!("每次须导入 1–500 条规则");
    }
    let mut rules = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let mut rule: ReplaceRule =
            serde_json::from_value(item).with_context(|| format!("第 {} 条规则格式错误", i + 1))?;
        crate::purify::validate_fields(&rule).with_context(|| format!("第 {} 条规则", i + 1))?;
        if crate::purify::validate(&rule).is_err() {
            rule.is_enabled = false;
        }
        rules.push(rule);
    }
    rules.sort_by_key(|r| r.order);
    Ok(rules)
}

impl Backend {
    pub async fn import_rules(&self, location: &str) -> Result<String> {
        let imported = parse_rules(&read_json(location).await?)?;
        let count = imported.len();
        let unsupported = imported
            .iter()
            .filter(|r| crate::purify::validate(r).is_err())
            .count();
        let mut rules = self.library().await?.rules;
        for mut rule in imported {
            // External IDs belong to the exporting library. Match logical identity.
            if let Some(old) = rules
                .iter_mut()
                .find(|r| r.name == rule.name && r.group == rule.group && r.scope == rule.scope)
            {
                rule.id = old.id;
                rule.order = old.order;
                *old = rule;
            } else {
                rule.id = rules
                    .iter()
                    .map(|r| r.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .context("规则 ID 已耗尽")?;
                rule.order = rules.len() as i32;
                rules.push(rule);
            }
        }
        if rules.len() > 500 {
            bail!("合并后超过 500 条规则，请先删除不需要的规则");
        }
        self.documents
            .write_list(NAMESPACE, "replace_rules.json", &rules)
            .await?;
        Ok(format!(
            "已导入 {count} 条规则；{unsupported} 条不兼容规则保留并停用（规则页查看原因）"
        ))
    }

    pub async fn import_layout(&self, location: &str) -> Result<String> {
        let layout = parse_layout(&read_json(location).await?)?;
        self.documents.set_value(NAMESPACE, LAYOUT, &layout).await?;
        Ok(format!(
            "已导入排版「{}」：颜色/粗体/缩进/间距；像素间距近似为终端行，其他字段仅保留",
            layout.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Change;

    #[test]
    fn layout_maps_argb_spacing_and_preserves_unsupported_fields() {
        let layout = parse_layout(r##"{"name":"番茄小说","bgStr":"#ffded9c5","textColor":"#ff5b4928","paragraphIndent":"　　","lineSpacingExtra":14,"paragraphSpacing":8,"textBold":0,"textSize":17,"pageAnim":0}"##).unwrap();
        assert_eq!(layout.background, Some([222, 217, 197]));
        assert_eq!(layout.foreground, Some([91, 73, 40]));
        assert_eq!(layout.indent.as_deref(), Some("　　"));
        assert_eq!(layout.line_gap, Some(1));
        assert_eq!(layout.paragraph_gap, Some(1));
        assert_eq!(layout.original["textSize"], 17);
        assert!(parse_layout(r#"{"textColor":"red"}"#).is_err());
        assert!(parse_layout("[{},{}]").is_err());
        assert!(parse_layout(r#"{"textBold":0,"lineSpacingExtra":-1}"#).is_err());
        assert!(parse_layout("{}").is_err());
    }

    #[test]
    fn imports_keep_scopes_unknown_fields_and_disabled_unsupported_patterns() {
        let rules = parse_rules(r#"[{"name":"标题","pattern":"广告","replacement":"","isEnabled":true,"scopeContent":false,"scopeTitle":true,"futureField":42},{"name":"不支持","isEnabled":true,"isRegex":true,"pattern":"(?<=a+)b"}]"#).unwrap();
        assert!(rules[0].is_enabled);
        assert!(!rules[0].scope_content && rules[0].scope_title);
        assert_eq!(rules[0].extra["futureField"], 42);
        assert!(!rules[1].is_enabled);
        assert_eq!(rules[1].pattern, "(?<=a+)b");
        assert!(parse_rules(r#"[{"name":"正常","pattern":"x"},{}]"#).is_err());
    }

    #[tokio::test]
    async fn imports_merge_atomically_reload_and_reset_layout() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(&temp.path().join("data")).await.unwrap();
        let path = temp.path().join("规则.json");
        tokio::fs::write(
            &path,
            "\u{feff}[{\"id\":17,\"name\":\"去水印\",\"pattern\":\"水印\",\"isEnabled\":true}]",
        )
        .await
        .unwrap();
        backend.import_rules(path.to_str().unwrap()).await.unwrap();
        backend.import_rules(path.to_str().unwrap()).await.unwrap();
        assert_eq!(backend.library().await.unwrap().rules.len(), 1);
        tokio::fs::write(&path, r#"[{"name":"新增","pattern":"测试"},{}]"#)
            .await
            .unwrap();
        assert!(backend.import_rules(path.to_str().unwrap()).await.is_err());
        assert_eq!(backend.library().await.unwrap().rules[0].name, "去水印");
        tokio::fs::write(
            &path,
            r##"{"name":"测试排版","textColor":"#010203","paragraphIndent":"  "}"##,
        )
        .await
        .unwrap();
        backend.import_layout(path.to_str().unwrap()).await.unwrap();
        let reopened = Backend::open(&temp.path().join("data")).await.unwrap();
        assert_eq!(
            reopened.library().await.unwrap().layout.unwrap().foreground,
            Some([1, 2, 3])
        );
        reopened
            .change_library(Change::Preference { index: 0, step: 1 })
            .await
            .unwrap();
        let layout = reopened.library().await.unwrap().layout.unwrap();
        assert!(layout.foreground.is_none());
        assert_eq!(layout.indent.as_deref(), Some("  "));
        reopened
            .change_library(Change::ResetPreferences)
            .await
            .unwrap();
        assert!(reopened.library().await.unwrap().layout.is_none());
    }

    #[tokio::test]
    async fn http_import_limits_and_error_responses_are_checked() {
        use axum::{http::StatusCode, routing::get, Router};
        let router = Router::new()
            .route(
                "/rules",
                get(|| async { r#"{"name":"网络规则","pattern":"广告","isEnabled":true}"# }),
            )
            .route("/large", get(|| async { "x".repeat(MAX_BYTES + 1) }))
            .route("/error", get(|| async { StatusCode::NOT_FOUND }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        backend
            .import_rules(&format!("{base}/rules"))
            .await
            .unwrap();
        assert!(backend
            .import_rules(&format!("{base}/large"))
            .await
            .is_err());
        assert!(backend
            .import_rules(&format!("{base}/error"))
            .await
            .is_err());
        assert_eq!(backend.library().await.unwrap().rules.len(), 1);
        task.abort();
    }
}
