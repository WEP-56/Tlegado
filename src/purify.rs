//! Replacement text retains a monotonic map to original Unicode scalar offsets.
use anyhow::{bail, Context, Result};
use fancy_regex::{Regex, RegexBuilder};
use reader_core::model::{book::Book, replace_rule::ReplaceRule};
use std::time::{Duration, Instant};

pub fn validate_fields(rule: &ReplaceRule) -> Result<()> {
    if rule.name.trim().is_empty() || rule.name.chars().count() > 100 {
        bail!("规则名须为 1–100 字");
    }
    if rule.pattern.is_empty() || rule.pattern.len() > 16_384 || rule.replacement.len() > 16_384 {
        bail!("匹配不能为空，匹配和替换各不超过 16 KiB");
    }
    Ok(())
}

pub fn validate(rule: &ReplaceRule) -> Result<()> {
    CompiledRule::new(rule).map(|_| ())
}

fn js_runtime() -> Result<rquickjs::Runtime> {
    let runtime = rquickjs::Runtime::new()?;
    runtime.set_memory_limit(8 * 1024 * 1024);
    runtime.set_max_stack_size(256 * 1024);
    let deadline = Instant::now() + Duration::from_millis(30);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    Ok(runtime)
}

#[derive(Clone)]
pub struct CompiledRule {
    regex: Regex,
    replacement: String,
    literal: bool,
    script: Option<String>,
    name: String,
}

impl CompiledRule {
    pub fn new(rule: &ReplaceRule) -> Result<Self> {
        validate_fields(rule)?;
        let script = rule.replacement.strip_prefix("@js:").map(str::to_owned);
        if let Some(script) = &script {
            let runtime = js_runtime()?;
            let context = rquickjs::Context::full(&runtime)?;
            context
                .with(|ctx| {
                    ctx.eval::<(), _>(format!(
                        "new Function({}); void 0",
                        serde_json::to_string(script).unwrap()
                    ))
                })
                .context("JavaScript 语法无效")?;
        }
        Ok(Self {
            regex: RegexBuilder::new(&if rule.is_regex {
                java_whitespace(&rule.pattern)
            } else {
                regex::escape(&rule.pattern)
            })
            .backtrack_limit(20_000)
            .build()
            .context("正则表达式不兼容（不支持可变长度后顾等语法）")?,
            replacement: rule.replacement.clone(),
            literal: !rule.is_regex,
            script,
            name: rule.name.clone(),
        })
    }
}

// fancy-regex uses \h for hexadecimal digits; Legado/Java uses horizontal space.
fn java_whitespace(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('h') => out.push_str(
                    r"[\t\x20\u{00a0}\u{1680}\u{180e}\u{2000}-\u{200a}\u{202f}\u{205f}\u{3000}]",
                ),
                Some('H') => out.push_str(
                    r"[^\t\x20\u{00a0}\u{1680}\u{180e}\u{2000}-\u{200a}\u{202f}\u{205f}\u{3000}]",
                ),
                Some(next) => {
                    out.push(ch);
                    out.push(next);
                }
                None => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn for_book(rules: &[ReplaceRule], book: &Book) -> Vec<CompiledRule> {
    for_scope(rules, book, false)
}
pub fn for_title(rules: &[ReplaceRule], book: &Book) -> Vec<CompiledRule> {
    for_scope(rules, book, true)
}
fn for_scope(rules: &[ReplaceRule], book: &Book, title: bool) -> Vec<CompiledRule> {
    rules
        .iter()
        .filter(|r| {
            r.is_enabled
                && if title {
                    r.scope_title
                } else {
                    r.scope_content
                }
                && r.scope.as_deref().is_none_or(|scope| {
                    scope.trim().is_empty()
                        || scope
                            .split([',', '，', ';', '\n'])
                            .map(str::trim)
                            .any(|s| s == book.name || s == book.book_url || s == book.origin)
                })
        })
        .filter_map(|r| CompiledRule::new(r).ok())
        .collect()
}

pub fn apply(text: &str, rules: &[CompiledRule]) -> (String, Vec<usize>) {
    let (text, map, _) = apply_report(text, rules);
    (text, map)
}

pub fn apply_report(text: &str, rules: &[CompiledRule]) -> (String, Vec<usize>, Vec<String>) {
    let mut text = text.to_owned();
    let mut positions: Vec<usize> = (0..=text.chars().count()).collect();
    let mut errors = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(150);
    for rule in rules {
        if Instant::now() > deadline {
            errors.push("净化达到时间上限，余下规则未执行".into());
            break;
        }
        let runtime = if rule.script.is_some() {
            js_runtime().ok()
        } else {
            None
        };
        let context = runtime
            .as_ref()
            .and_then(|r| rquickjs::Context::full(r).ok());
        if rule.script.is_some() && context.is_none() {
            errors.push(format!("{}：JS 初始化失败", rule.name));
            continue;
        }
        let mut output = String::new();
        let mut mapped = Vec::new();
        let (mut byte, mut scalar) = (0, 0);
        let limit = text
            .len()
            .saturating_mul(4)
            .saturating_add(1_048_576)
            .min(64 * 1024 * 1024);
        let mut overflow = false;
        for capture in rule.regex.captures_iter(&text) {
            let capture = match capture {
                Ok(capture) if Instant::now() <= deadline => capture,
                _ => {
                    overflow = true;
                    break;
                }
            };
            let hit = capture.get(0).unwrap();
            let prefix = &text[byte..hit.start()];
            let len = prefix.chars().count();
            output.push_str(prefix);
            mapped.extend_from_slice(&positions[scalar..scalar + len]);
            scalar += len;
            let mut replacement = String::new();
            if let (Some(script), Some(context)) = (&rule.script, &context) {
                let result = context.with(|ctx| -> rquickjs::Result<String> {
                    ctx.globals().set("result", hit.as_str())?;
                    ctx.eval::<rquickjs::Coerced<String>, _>(format!(
                        "(function(result) {{ return eval({}); }})(result)",
                        serde_json::to_string(script).unwrap()
                    ))
                    .map(|s| s.0)
                });
                match result {
                    Ok(value) => replacement = value,
                    Err(_) => {
                        overflow = true;
                        break;
                    }
                }
            } else if rule.literal {
                replacement.push_str(&rule.replacement);
            } else {
                capture.expand(&rule.replacement, &mut replacement);
            }
            if output.len().saturating_add(replacement.len()) > limit {
                overflow = true;
                break;
            }
            output.push_str(&replacement);
            mapped.extend(std::iter::repeat_n(
                positions[scalar],
                replacement.chars().count(),
            ));
            scalar += hit.as_str().chars().count();
            byte = hit.end();
        }
        // Reject an explosive expansion as a whole; never truncate chapter text.
        if overflow || output.len().saturating_add(text.len() - byte) > limit {
            errors.push(format!("{}：执行失败或超出限额，本条未应用", rule.name));
            continue;
        }
        output.push_str(&text[byte..]);
        mapped.extend_from_slice(&positions[scalar..]);
        text = output;
        positions = mapped;
    }
    (text, positions, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule(pattern: &str, replacement: &str, regex: bool) -> ReplaceRule {
        ReplaceRule {
            name: "测试".into(),
            pattern: pattern.into(),
            replacement: replacement.into(),
            is_regex: regex,
            is_enabled: true,
            ..Default::default()
        }
    }
    #[test]
    fn lookarounds_js_and_horizontal_spaces_preserve_unicode_positions() {
        let compiled = CompiledRule::new(&rule(
            r"(?<=甲)[０-９](?=乙)",
            "@js:String.fromCharCode(result.charCodeAt(0)-65248)",
            true,
        ))
        .unwrap();
        let (text, map, errors) = apply_report("😀甲３乙３", &[compiled]);
        assert_eq!(text, "😀甲3乙３");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(map, vec![0, 1, 2, 3, 4, 5]);
        let spaces = CompiledRule::new(&rule(r"[\h]+", "_", true)).unwrap();
        assert_eq!(apply("A 1\t中　F", &[spaces]).0, "A_1_中_F");
        let escaped = CompiledRule::new(&rule(r"\\h", "X", true)).unwrap();
        assert_eq!(apply(r"\h", &[escaped]).0, "X");
    }

    #[test]
    fn javascript_failures_and_loops_leave_entire_rule_unapplied() {
        let lexical =
            CompiledRule::new(&rule("字", "@js:let text = result; text + '!'", false)).unwrap();
        assert_eq!(apply("字字", &[lexical]).0, "字!字!");
        for script in ["@js:throw new Error('bad')", "@js:while(true){}"] {
            let compiled = CompiledRule::new(&rule("字", script, false)).unwrap();
            let (text, map, errors) = apply_report("原文字字", &[compiled]);
            assert_eq!(text, "原文字字");
            assert_eq!(map, vec![0, 1, 2, 3, 4]);
            assert_eq!(errors.len(), 1);
        }
    }

    #[test]
    fn title_only_rules_do_not_change_content() {
        let mut title = rule("广告", "", false);
        title.scope_content = false;
        title.scope_title = true;
        let book = Book::default();
        assert!(for_book(&[title.clone()], &book).is_empty());
        assert_eq!(apply("广告第一章", &for_title(&[title], &book)).0, "第一章");
        let legacy: ReplaceRule =
            serde_json::from_str(r#"{"name":"旧规则","pattern":"水印"}"#).unwrap();
        assert!(legacy.scope_content && !legacy.scope_title);
    }
    #[test]
    fn chained_literal_and_regex_replacements_keep_original_unicode_offsets() {
        let rules = [
            rule("广告", "", false),
            rule(r"(正文)([0-9]+)", "$1·$2", true),
        ]
        .iter()
        .map(|r| CompiledRule::new(r).unwrap())
        .collect::<Vec<_>>();
        let original = "序😀广告正文12尾";
        let (text, map) = apply(original, &rules);
        assert_eq!(text, "序😀正文·12尾");
        assert_eq!(map.len(), text.chars().count() + 1);
        assert_eq!(map[2], 4);
        assert_eq!(map[7], 8);
        assert_eq!(*map.last().unwrap(), original.chars().count());
        assert!(map.windows(2).all(|w| w[0] <= w[1]));
    }
    #[test]
    fn literal_dollars_disabled_rules_and_exact_scopes_are_respected() {
        let mut r = rule("水印", "$1", false);
        r.scope = Some("一本书,https://source.invalid".into());
        let mut b = Book {
            name: "一本书".into(),
            ..Default::default()
        };
        assert_eq!(apply("水印正文", &for_book(&[r.clone()], &b)).0, "$1正文");
        b.name = "一本书续集".into();
        assert!(for_book(&[r.clone()], &b).is_empty());
        b.origin = "https://source.invalid".into();
        assert_eq!(for_book(&[r.clone()], &b).len(), 1);
        r.is_enabled = false;
        assert!(for_book(&[r], &b).is_empty());
    }
    #[test]
    fn zero_width_matches_and_full_deletion_have_valid_maps() {
        for (pattern, replacement, expected) in [("^|$", "界", "界中😀界"), ("(?s).*", "", "")]
        {
            let (text, map) = apply(
                "中😀",
                &[CompiledRule::new(&rule(pattern, replacement, true)).unwrap()],
            );
            assert_eq!(text, expected);
            assert_eq!(map.len(), text.chars().count() + 1);
            assert_eq!(*map.last().unwrap(), 2);
        }
        assert!(validate(&rule("[", "", true)).is_err());
        assert!(validate(&rule("", "", false)).is_err());
    }
}
