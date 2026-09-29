use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReplaceRule {
    pub id: i64,
    pub name: String,
    pub group: Option<String>,
    pub pattern: String,
    pub replacement: String,
    pub scope: Option<String>,
    #[serde(rename = "isEnabled")]
    pub is_enabled: bool,
    #[serde(rename = "isRegex")]
    pub is_regex: bool,
    pub order: i32,
    pub scope_content: bool,
    pub scope_title: bool,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Default for ReplaceRule {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            group: None,
            pattern: String::new(),
            replacement: String::new(),
            scope: None,
            is_enabled: false,
            is_regex: false,
            order: 0,
            scope_content: true,
            scope_title: false,
            extra: Default::default(),
        }
    }
}
