//! A single transaction covers cookies, login headers and source state.
use cookie_store::{Cookie, CookieStore};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, io::Write, path::PathBuf, sync::Mutex};

#[derive(Clone, Default)]
pub(super) struct SessionData {
    pub cookies: CookieStore,
    pub cache: HashMap<String, String>,
    pub variable: String,
    pub login_headers: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    identity: String,
    // CookieStore's default Serialize omits session cookies. Legado retains
    // these too; keep their original attributes and drop expired entries.
    cookies: Vec<Cookie<'static>>,
    cache: HashMap<String, String>,
    variable: String,
    login_headers: HashMap<String, String>,
}

#[derive(Default)]
pub(super) struct SessionStore {
    data: Mutex<SessionData>,
    file: Option<(PathBuf, String)>,
    load_error: Option<&'static str>,
    callback_error: Mutex<bool>,
}

impl SessionStore {
    pub fn open(path: PathBuf, identity: String) -> Self {
        let loaded = (|| -> anyhow::Result<SessionData> {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(SessionData::default())
                }
                Err(e) => return Err(e.into()),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(
                snapshot.version == 1 && snapshot.identity == identity,
                "invalid source session version or identity"
            );
            for (name, value) in &snapshot.login_headers {
                reqwest::header::HeaderName::from_bytes(name.as_bytes())?;
                reqwest::header::HeaderValue::from_str(value)?;
            }
            let cookies = CookieStore::from_cookies(
                snapshot.cookies.into_iter().map(Ok::<_, anyhow::Error>),
                false,
            )?;
            Ok(SessionData {
                cookies,
                cache: snapshot.cache,
                variable: snapshot.variable,
                login_headers: snapshot.login_headers,
            })
        })();
        let (data, load_error) = match loaded {
            Ok(data) => (data, None),
            Err(_) => (
                SessionData::default(),
                Some("source session could not be loaded; original file preserved"),
            ),
        };
        Self {
            data: Mutex::new(data),
            file: Some((path, identity)),
            load_error,
            callback_error: Mutex::new(false),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            load_error: Some("invalid source session namespace"),
            ..Default::default()
        }
    }

    pub fn check(&self) -> anyhow::Result<()> {
        if let Some(error) = self.load_error {
            anyhow::bail!(error);
        }
        let mut failed = self
            .callback_error
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if std::mem::take(&mut *failed) {
            anyhow::bail!("could not persist response cookies");
        }
        Ok(())
    }

    pub fn read<T>(&self, f: impl FnOnce(&SessionData) -> T) -> T {
        f(&self.data.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn update<T>(
        &self,
        f: impl FnOnce(&mut SessionData) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        if let Some(error) = self.load_error {
            anyhow::bail!(error);
        }
        let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = data.clone();
        let result = f(&mut next)?;
        if let Some((path, identity)) = &self.file {
            let snapshot = Snapshot {
                version: 1,
                identity: identity.clone(),
                cookies: next.cookies.iter_unexpired().cloned().collect(),
                cache: next.cache.clone(),
                variable: next.variable.clone(),
                login_headers: next.login_headers.clone(),
            };
            let bytes = serde_json::to_vec(&snapshot)?;
            atomic_write(path, &bytes).map_err(|_| {
                anyhow::anyhow!("could not persist source session; previous state preserved")
            })?;
        }
        *data = next;
        Ok(result)
    }

    // reqwest's CookieStore callback cannot return Result. Report its failure
    // at the enclosing request boundary without exposing cookie values.
    pub fn record_callback_failure(&self) {
        *self
            .callback_error
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = true;
    }
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing session directory"))?;
    let mut dirs = fs::DirBuilder::new();
    dirs.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        dirs.mode(0o700);
    }
    dirs.create(parent)?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
