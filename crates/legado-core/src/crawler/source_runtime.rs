//! Explicit per-user, per-source state shared by native and JavaScript HTTP.
//! A task-local context survives awaits; a scoped thread-local is used only
//! during synchronous evaluation. Neither context is a process-global session.
use super::http_client::HttpClient;
use super::session_store::SessionStore;
use crate::model::book_source::BookSource;
use reqwest::{cookie::CookieStore as ReqwestCookieStore, header::HeaderValue};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};
use url::Url;

#[derive(Default)]
pub struct SourceCookies(Arc<SessionStore>);

impl SourceCookies {
    pub fn get(&self, url: &str) -> anyhow::Result<String> {
        let url = Url::parse(url)?;
        Ok(self.0.read(|data| {
            data.cookies
                .get_request_values(&url)
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ")
        }))
    }

    /// Cookie header import, not a Set-Cookie header (attributes are not accepted).
    pub fn put(&self, url: &str, header: &str, replace: bool) -> anyhow::Result<()> {
        let url = Url::parse(url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https"),
            "invalid cookie URL"
        );
        self.0
            .update(|data| Self::import(&mut data.cookies, &url, header, replace))
    }

    fn import(
        candidate: &mut cookie_store::CookieStore,
        url: &Url,
        header: &str,
        replace: bool,
    ) -> anyhow::Result<()> {
        if replace {
            Self::remove_domain(candidate, url);
        }
        for part in header.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            anyhow::ensure!(part.contains('='), "invalid cookie header");
            candidate
                .parse(&format!("{part}; Path=/"), url)
                .map_err(|_| anyhow::anyhow!("invalid cookie header"))?;
        }
        Ok(())
    }

    pub fn remove(&self, url: &str) -> anyhow::Result<()> {
        let url = Url::parse(url)?;
        self.0.update(|data| {
            Self::remove_domain(&mut data.cookies, &url);
            Ok(())
        })
    }

    fn remove_domain(store: &mut cookie_store::CookieStore, url: &Url) {
        let keys: Vec<_> = store
            .iter_any()
            .filter(|c| c.domain.matches(url))
            .filter_map(|c| {
                let domain = match &c.domain {
                    cookie_store::CookieDomain::HostOnly(v)
                    | cookie_store::CookieDomain::Suffix(v) => v.clone(),
                    _ => return None,
                };
                Some((domain, c.path.as_ref().to_string(), c.name().to_string()))
            })
            .collect();
        for (domain, path, name) in keys {
            store.remove(&domain, &path, &name);
        }
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        self.0.update(|data| {
            data.cookies.clear();
            Ok(())
        })
    }
}

impl ReqwestCookieStore for SourceCookies {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let values: Vec<_> = headers.filter_map(|h| h.to_str().ok()).collect();
        if values.is_empty() {
            return;
        }
        if self
            .0
            .update(|data| {
                for value in values {
                    let _ = data.cookies.parse(value, url);
                }
                Ok(())
            })
            .is_err()
        {
            self.0.record_callback_failure();
        }
    }
    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let text = self.get(url.as_str()).ok()?;
        if text.is_empty() {
            None
        } else {
            HeaderValue::from_str(&text).ok()
        }
    }
}

/// Disabling automatic response cookies must not disable explicitly imported cookies.
pub struct CookieProvider {
    pub jar: Arc<SourceCookies>,
    pub accept_response: bool,
}
impl ReqwestCookieStore for CookieProvider {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        if self.accept_response {
            self.jar.set_cookies(headers, url);
        }
    }
    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.jar.cookies(url)
    }
}

pub struct SourceSession {
    pub cookies: Arc<SourceCookies>,
    pub js_lib_cache: Mutex<HashMap<String, String>>,
    store: Arc<SessionStore>,
}

impl Default for SourceSession {
    fn default() -> Self {
        Self::from_store(SessionStore::default())
    }
}

impl SourceSession {
    fn from_store(store: SessionStore) -> Self {
        let store = Arc::new(store);
        Self {
            cookies: Arc::new(SourceCookies(store.clone())),
            js_lib_cache: Mutex::new(HashMap::new()),
            store,
        }
    }

    /// Files stay under the existing per-user data directory. The source URL
    /// is hashed so query parameters and filesystem separators never enter names.
    pub fn persistent(root: &std::path::Path, user_ns: &str, source_url: &str) -> Self {
        use sha2::{Digest, Sha256};
        let mut parts = std::path::Path::new(user_ns).components();
        if !matches!(parts.next(), Some(std::path::Component::Normal(_)))
            || parts.next().is_some()
            || user_ns.contains(['\\', '/', ':', '\0'])
            || user_ns.ends_with(['.', ' '])
        {
            return Self::from_store(SessionStore::unavailable());
        }
        let source_url = crate::util::text::normalize_source_url(source_url);
        let identity = hex::encode(Sha256::digest(
            serde_json::to_vec(&(user_ns, &source_url)).unwrap(),
        ));
        let name = hex::encode(Sha256::digest(source_url.as_bytes()));
        let path = root
            .join("data")
            .join(user_ns)
            .join("source_sessions")
            .join(format!("{name}.json"));
        Self::from_store(SessionStore::open(path, identity))
    }

    pub fn check(&self) -> anyhow::Result<()> {
        self.store.check()
    }
    pub fn variable(&self) -> String {
        self.store.read(|data| data.variable.clone())
    }
    pub fn set_variable(&self, value: String) -> anyhow::Result<()> {
        self.store.update(|data| {
            data.variable = value;
            Ok(())
        })
    }
    pub fn cache_get(&self, key: &str) -> Option<String> {
        self.store.read(|data| data.cache.get(key).cloned())
    }
    pub fn cache_put(&self, key: String, value: String) -> anyhow::Result<()> {
        self.store.update(|data| {
            data.cache.insert(key, value);
            Ok(())
        })
    }
    pub fn login_headers(&self) -> HashMap<String, String> {
        self.store.read(|data| data.login_headers.clone())
    }

    pub fn put_login_headers(&self, url: &str, text: &str) -> anyhow::Result<()> {
        let mut headers: HashMap<String, String> =
            serde_json::from_str(text).map_err(|_| anyhow::anyhow!("invalid login headers"))?;
        for (name, value) in &headers {
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| anyhow::anyhow!("invalid login header name"))?;
            HeaderValue::from_str(value)
                .map_err(|_| anyhow::anyhow!("invalid login header value"))?;
        }
        self.store.update(|data| {
            for (name, value) in &mut headers {
                if name.eq_ignore_ascii_case("cookie") {
                    let url = Url::parse(url)?;
                    anyhow::ensure!(
                        matches!(url.scheme(), "http" | "https"),
                        "invalid cookie URL"
                    );
                    SourceCookies::import(&mut data.cookies, &url, value, false)?;
                    // Keep only a marker: getters rebuild Cookie from the live jar.
                    value.clear();
                }
            }
            data.login_headers = headers;
            Ok(())
        })?;
        self.js_lib_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }

    pub fn remove_login_headers(&self, url: &str) -> anyhow::Result<()> {
        let url = Url::parse(url)?;
        self.store.update(|data| {
            data.login_headers.clear();
            SourceCookies::remove_domain(&mut data.cookies, &url);
            Ok(())
        })?;
        self.js_lib_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }

    pub fn clear_auth(&self) -> anyhow::Result<()> {
        self.store.update(|data| {
            data.cookies.clear();
            data.login_headers.clear();
            Ok(())
        })?;
        self.js_lib_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }
}

#[derive(Clone)]
pub struct SourceRuntime {
    pub source: BookSource,
    pub session: Arc<SourceSession>,
    pub http: HttpClient,
    /// Scratch rule variables live for one operation, not the lifetime of a source.
    pub variables: Arc<Mutex<HashMap<String, String>>>,
}

tokio::task_local! { static TASK_RUNTIME: SourceRuntime; }
thread_local! { static SYNC_RUNTIME: RefCell<Option<SourceRuntime>> = const { RefCell::new(None) }; }
thread_local! { static HEADER_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(crate) fn in_header() -> bool {
    HEADER_DEPTH.with(|n| n.get() > 0)
}
pub(crate) fn with_header<T>(f: impl FnOnce() -> T) -> T {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            HEADER_DEPTH.with(|n| n.set(n.get() - 1));
        }
    }
    HEADER_DEPTH.with(|n| n.set(n.get() + 1));
    let _restore = Restore;
    f()
}

impl SourceRuntime {
    pub fn new(source: BookSource, session: Arc<SourceSession>, http: HttpClient) -> Self {
        Self {
            source,
            session,
            http,
            variables: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn current() -> Option<Self> {
        SYNC_RUNTIME
            .with(|r| r.borrow().clone())
            .or_else(|| TASK_RUNTIME.try_with(Clone::clone).ok())
    }
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        TASK_RUNTIME.scope(self.clone(), future).await
    }
    pub fn enter<T>(&self, f: impl FnOnce() -> T) -> T {
        struct Restore(Option<SourceRuntime>);
        impl Drop for Restore {
            fn drop(&mut self) {
                SYNC_RUNTIME.with(|r| {
                    r.replace(self.0.take());
                });
            }
        }
        let _restore = Restore(SYNC_RUNTIME.with(|r| r.replace(Some(self.clone()))));
        f()
    }
    pub fn cookies(&self) -> Arc<CookieProvider> {
        Arc::new(CookieProvider {
            jar: self.session.cookies.clone(),
            accept_response: self.source.enabled_cookie_jar == Some(true),
        })
    }
}
