use super::{config::HttpConfig, Backend};
use anyhow::{bail, Context, Result};
use base64::Engine;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink, Source};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{runtime::Runtime, task::JoinHandle};

const MAX_AUDIO_BYTES: usize = 32 * 1024 * 1024;
type Audio = Arc<[u8]>;
type AudioTask = JoinHandle<Result<Audio>>;

fn decode(bytes: Audio) -> Result<Decoder<Cursor<Audio>>> {
    Decoder::try_from(Cursor::new(bytes))
        .map_err(|_| anyhow::anyhow!("TTS 返回的音频无法解码，请检查格式和接口配置"))
}
fn validate(bytes: Audio) -> Result<()> {
    let mut decoder = decode(bytes)?;
    if decoder.channels() > 8
        || decoder.sample_rate() > 192_000
        || decoder
            .total_duration()
            .is_some_and(|duration| duration > Duration::from_secs(1200))
    {
        bail!("TTS 音频参数或时长超出支持范围");
    }
    if decoder.next().is_none() {
        bail!("TTS 返回了空音频");
    }
    Ok(())
}

fn decode_response(bytes: Vec<u8>, mode: &str) -> Result<Audio> {
    if mode == "audio" {
        return Ok(bytes.into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("TTS JSON 音频响应格式无效"))?;
    let encoded = json
        .pointer("/choices/0/message/audio/data")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            json.pointer("/audio/data")
                .and_then(serde_json::Value::as_str)
        })
        .ok_or_else(|| anyhow::anyhow!("TTS JSON 响应中没有音频数据"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| anyhow::anyhow!("TTS JSON 音频 Base64 无效"))?;
    if decoded.len() > MAX_AUDIO_BYTES {
        bail!("TTS 单个音频响应不能超过 32 MiB");
    }
    Ok(decoded.into())
}

#[derive(Clone)]
struct Fetcher {
    config: HttpConfig,
    client: reqwest::Client,
    cache_dir: Option<PathBuf>,
    cache_lock: Arc<tokio::sync::Mutex<()>>,
}
impl Fetcher {
    fn new(config: HttpConfig, cache_dir: Option<PathBuf>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .default_headers(config.request_headers()?)
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(config.timeout_seconds.min(10)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| anyhow::anyhow!("无法初始化 TTS HTTP 客户端"))?;
        Ok(Self {
            config,
            client,
            cache_dir,
            cache_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    fn key(&self, text: &str, speed: u16) -> String {
        // Credentials are never included. Environment variable names separate account configs.
        let input = serde_json::to_vec(&(
            &self.config.endpoint,
            self.config.body(text, speed),
            &self.config.headers,
            &self.config.header_env,
            &self.config.api_key_env,
        ))
        .expect("JSON request serializes");
        format!("{:x}", Sha256::digest(input))
    }

    async fn cached(&self, key: &str) -> Option<Audio> {
        if self.config.cache_max_mb == 0 {
            return None;
        }
        let path = self.cache_dir.as_ref()?.join(format!("{key}.audio"));
        let metadata = tokio::fs::symlink_metadata(&path).await.ok()?;
        if !metadata.is_file() {
            return None;
        }
        if metadata.len() > MAX_AUDIO_BYTES as u64 {
            let _ = tokio::fs::remove_file(&path).await;
            return None;
        }
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(&path)
            .await
            .ok()?
            .take(MAX_AUDIO_BYTES as u64 + 1);
        let mut bounded = Vec::new();
        file.read_to_end(&mut bounded).await.ok()?;
        let bytes: Audio = bounded.into();
        if bytes.len() <= MAX_AUDIO_BYTES && validate(bytes.clone()).is_ok() {
            Some(bytes)
        } else {
            let _ = tokio::fs::remove_file(path).await;
            None
        }
    }

    async fn store(&self, key: &str, bytes: &Audio) {
        let limit = self.config.cache_max_mb * 1024 * 1024;
        let Some(dir) = &self.cache_dir else {
            return;
        };
        if limit == 0 || bytes.len() as u64 > limit {
            return;
        }
        let _guard = self.cache_lock.lock().await;
        // Cache failures do not prevent playback. Tempfile guards remove interrupted writes.
        let persist = || -> Result<()> {
            std::fs::create_dir_all(dir)?;
            let mut entries = Vec::new();
            let mut size = 0u64;
            for entry in std::fs::read_dir(dir)?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(hash) = name.strip_suffix(".audio") else {
                    continue;
                };
                if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                    continue;
                }
                let meta = entry.path().symlink_metadata()?;
                if !meta.is_file() {
                    continue;
                }
                size = size.saturating_add(meta.len());
                entries.push((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    entry.path(),
                    meta.len(),
                ));
            }
            entries.sort_by_key(|(time, _, _)| *time);
            for (_, path, len) in entries {
                if size + bytes.len() as u64 <= limit {
                    break;
                }
                if std::fs::remove_file(path).is_ok() {
                    size = size.saturating_sub(len);
                }
            }
            if size + bytes.len() as u64 > limit {
                return Ok(());
            }
            let mut temp = tempfile::NamedTempFile::new_in(dir)?;
            temp.write_all(bytes)?;
            temp.flush()?;
            let _ = temp.persist_noclobber(dir.join(format!("{key}.audio")));
            Ok(())
        };
        let _ = persist();
    }

    async fn fetch(&self, text: &str, speed: u16) -> Result<Audio> {
        let key = self.key(text, speed);
        if let Some(bytes) = self.cached(&key).await {
            return Ok(bytes);
        }
        let body = serde_json::to_vec(&self.config.body(text, speed))?;
        for attempt in 0..=self.config.retries {
            let mut response = self
                .client
                .post(&self.config.endpoint)
                .body(body.clone())
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        anyhow::anyhow!("TTS 请求超时，按 p 重试")
                    } else {
                        anyhow::anyhow!("TTS 网络请求失败，请检查地址、网络和 TLS 配置")
                    }
                })?;
            let status = response.status();
            if !status.is_success() {
                if (status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    && attempt < self.config.retries
                {
                    let delay = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|seconds| Duration::from_secs(seconds.min(5)))
                        .unwrap_or(Duration::from_millis(250 * (u64::from(attempt) + 1)));
                    drop(response);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                // Never reflect a provider response or reqwest URL: either can contain secrets.
                bail!(
                    "TTS 接口返回 HTTP {}，请检查密钥、额度和请求配置",
                    status.as_u16()
                );
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if self.config.response_mode == "audio"
                && !content_type.is_empty()
                && !content_type.starts_with("audio/")
                && !matches!(
                    content_type.as_str(),
                    "application/octet-stream" | "application/ogg"
                )
            {
                bail!("TTS 接口返回了 JSON、文本或其他非音频内容，请检查响应模式");
            }
            if response
                .content_length()
                .is_some_and(|len| len > MAX_AUDIO_BYTES as u64)
            {
                bail!("TTS 单个音频响应不能超过 32 MiB");
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| anyhow::anyhow!("TTS 音频下载失败或超时"))?
            {
                if bytes.len().saturating_add(chunk.len()) > MAX_AUDIO_BYTES {
                    bail!("TTS 单个音频响应不能超过 32 MiB");
                }
                bytes.extend_from_slice(&chunk);
            }
            let bytes = decode_response(bytes, &self.config.response_mode)?;
            validate(bytes.clone())?;
            self.store(&key, &bytes).await;
            return Ok(bytes);
        }
        unreachable!("bounded retries always return")
    }
}

pub(super) trait Player {
    fn play(&mut self, audio: Audio, paused: bool) -> Result<()>;
    fn pause(&mut self, paused: bool);
    fn finished(&self) -> Result<bool>;
    fn stop(&mut self);
}
pub(super) struct RodioPlayer {
    sink: Option<Sink>,
    stream: OutputStream,
    failed: Arc<AtomicBool>,
}
impl RodioPlayer {
    fn new() -> Result<Self> {
        let failed = Arc::new(AtomicBool::new(false));
        let error_flag = failed.clone();
        let mut stream = OutputStreamBuilder::from_default_device()
            .context("找不到默认音频输出设备")?
            .with_error_callback(move |_| {
                error_flag.store(true, Ordering::SeqCst);
            })
            .open_stream()
            .context("无法打开默认音频输出设备")?;
        stream.log_on_drop(false);
        Ok(Self {
            sink: None,
            stream,
            failed,
        })
    }
}
impl Player for RodioPlayer {
    fn play(&mut self, audio: Audio, paused: bool) -> Result<()> {
        self.stop();
        if self.failed.load(Ordering::SeqCst) {
            bail!("音频设备已断开，按 p 重新初始化");
        }
        let sink = Sink::connect_new(self.stream.mixer());
        if paused {
            sink.pause();
        }
        sink.append(decode(audio)?);
        self.sink = Some(sink);
        Ok(())
    }
    fn pause(&mut self, paused: bool) {
        if let Some(sink) = &self.sink {
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }
    fn finished(&self) -> Result<bool> {
        if self.failed.load(Ordering::SeqCst) {
            bail!("音频设备已断开，按 p 重新初始化");
        }
        Ok(self.sink.as_ref().is_none_or(Sink::empty))
    }
    fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
    }
}

pub(super) struct HttpSpeech<P: Player = RodioPlayer> {
    player: P,
    fetcher: Fetcher,
    runtime: Option<Runtime>,
    pending: Option<AudioTask>,
    prefetch: Option<(String, AudioTask)>,
    paused: bool,
}
impl HttpSpeech {
    pub(super) fn new(config: HttpConfig, cache_dir: Option<PathBuf>) -> Result<Self> {
        // Validate credentials before opening an output device or sending text to the provider.
        config.request_headers()?;
        Self::with_player(config, cache_dir, RodioPlayer::new()?)
    }
}
impl<P: Player> HttpSpeech<P> {
    fn with_player(config: HttpConfig, cache_dir: Option<PathBuf>, player: P) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("无法启动 TTS 下载线程")?;
        let fetcher = {
            let _enter = runtime.enter();
            Fetcher::new(config, cache_dir)?
        };
        Ok(Self {
            player,
            fetcher,
            runtime: Some(runtime),
            pending: None,
            prefetch: None,
            paused: false,
        })
    }
    fn task(&self, text: &str, speed: u16) -> AudioTask {
        let fetcher = self.fetcher.clone();
        let text = text.to_string();
        self.runtime
            .as_ref()
            .unwrap()
            .spawn(async move { fetcher.fetch(&text, speed).await })
    }
}
impl<P: Player> Backend for HttpSpeech<P> {
    fn reset(&mut self, next: Option<&str>, speed: u16) -> Result<()> {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
        self.player.stop();
        let reusable = next.is_some_and(|text| {
            self.prefetch
                .as_ref()
                .is_some_and(|(key, _)| key == &self.fetcher.key(text, speed))
        });
        if !reusable {
            if let Some((_, task)) = self.prefetch.take() {
                task.abort();
            }
        }
        self.paused = false;
        Ok(())
    }
    fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
        self.player.stop();
        if let Some(task) = self.pending.take() {
            task.abort();
        }
        self.paused = false;
        let key = self.fetcher.key(text, speed);
        self.pending = Some(match self.prefetch.take() {
            Some((cached_key, task)) if key == cached_key => task,
            other => {
                if let Some((_, task)) = other {
                    task.abort();
                }
                self.task(text, speed)
            }
        });
        Ok(())
    }
    fn ready(&mut self) -> Result<bool> {
        if let Some(task) = self.pending.as_ref() {
            if !task.is_finished() {
                return Ok(false);
            }
            let task = self.pending.take().unwrap();
            let audio = self
                .runtime
                .as_ref()
                .unwrap()
                .block_on(task)
                .map_err(|_| anyhow::anyhow!("TTS 下载任务已取消或异常结束"))??;
            self.player.play(audio, self.paused)?;
        }
        Ok(true)
    }
    fn prefetch(&mut self, text: &str, speed: u16) {
        if !self.fetcher.config.prefetch {
            return;
        }
        let key = self.fetcher.key(text, speed);
        if self
            .prefetch
            .as_ref()
            .is_some_and(|(previous, _)| previous == &key)
        {
            return;
        }
        if let Some((_, task)) = self.prefetch.take() {
            task.abort();
        }
        self.prefetch = Some((key, self.task(text, speed)));
    }
    fn pause(&mut self, paused: bool) -> Result<()> {
        self.paused = paused;
        self.player.pause(paused);
        Ok(())
    }
    // Preserve pitch: provider-side speed changes take effect on the next synthesis chunk.
    fn rate(&mut self, _speed: u16) -> Result<()> {
        Ok(())
    }
    fn finished(&mut self) -> Result<bool> {
        if self.pending.is_some() {
            Ok(false)
        } else {
            self.player.finished()
        }
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
        if let Some((_, task)) = self.prefetch.take() {
            task.abort();
        }
        self.player.stop();
        Ok(())
    }
}
impl<P: Player> Drop for HttpSpeech<P> {
    fn drop(&mut self) {
        let _ = self.stop();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mimo_json_base64_response_is_decoded_before_audio_validation() {
        let wav = wav();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&wav);
        let response = serde_json::to_vec(&serde_json::json!({
            "choices": [{"message": {"audio": {"data": encoded}}}]
        }))
        .unwrap();
        let audio = decode_response(response, "json_base64").unwrap();
        validate(audio).unwrap();
    }
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        Json, Router,
    };
    use std::sync::{atomic::AtomicUsize, Mutex};
    use std::time::Instant;

    fn wav() -> Vec<u8> {
        let rate = 16_000u32;
        let samples = 4_000u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples * 2).to_le_bytes());
        for index in 0..samples {
            let value = ((f64::from(index) * std::f64::consts::TAU * 440.0 / f64::from(rate)).sin()
                * 1_200.0) as i16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    #[derive(Default)]
    struct ServerState {
        calls: AtomicUsize,
        requests: Mutex<Vec<(HeaderMap, serde_json::Value)>>,
    }
    struct Server {
        endpoint: String,
        state: Arc<ServerState>,
        task: JoinHandle<()>,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn handler(
        State(state): State<Arc<ServerState>>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> Response {
        let count = state.calls.fetch_add(1, Ordering::SeqCst);
        state.requests.lock().unwrap().push((headers, body.clone()));
        match body["input"].as_str().unwrap_or("") {
            "retry" if count < 2 => {
                (StatusCode::SERVICE_UNAVAILABLE, "provider-secret").into_response()
            }
            "unauthorized" => (StatusCode::UNAUTHORIZED, "provider-secret").into_response(),
            "json" => (
                [("content-type", "application/json")],
                r#"{"error":"provider-secret"}"#,
            )
                .into_response(),
            "bad" => (
                [("content-type", "application/octet-stream")],
                "provider-secret",
            )
                .into_response(),
            "redirect" => (
                StatusCode::TEMPORARY_REDIRECT,
                [("location", "http://127.0.0.1:1/secret")],
            )
                .into_response(),
            "timeout" => {
                tokio::time::sleep(Duration::from_secs(3)).await;
                ([("content-type", "audio/wav")], wav()).into_response()
            }
            "slow" => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                ([("content-type", "audio/wav")], wav()).into_response()
            }
            _ => ([("content-type", "audio/wav")], wav()).into_response(),
        }
    }
    async fn server() -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/speech", listener.local_addr().unwrap());
        let state = Arc::new(ServerState::default());
        let app = Router::new()
            .route("/speech", post(handler))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Server {
            endpoint,
            state,
            task,
        }
    }
    fn config(endpoint: &str) -> HttpConfig {
        serde_json::from_value(serde_json::json!({"endpoint":endpoint,"model":"fixture","voice":"voice","prefetch":true})).unwrap()
    }

    #[tokio::test]
    async fn request_decoding_cache_identity_and_corruption_recovery() {
        let server = server().await;
        let cache = tempfile::tempdir().unwrap();
        let fetcher = Fetcher::new(config(&server.endpoint), Some(cache.path().into())).unwrap();
        let text = "中文\"\n{{voice}}";
        let bytes = fetcher.fetch(text, 125).await.unwrap();
        validate(bytes.clone()).unwrap();
        assert_eq!(fetcher.fetch(text, 125).await.unwrap(), bytes);
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
        let request = server.state.requests.lock().unwrap()[0].clone();
        assert_eq!(request.0["content-type"], "application/json");
        assert_eq!(request.1["input"], text);
        assert_eq!(request.1["speed"], 1.25);
        let key = fetcher.key(text, 125);
        assert_eq!(key.len(), 64);
        let mut other = fetcher.clone();
        other.config.voice = "other".into();
        assert_ne!(key, other.key(text, 125));
        assert_ne!(key, fetcher.key(text, 150));
        tokio::fs::write(cache.path().join(format!("{key}.audio")), b"corrupt")
            .await
            .unwrap();
        fetcher.fetch(text, 125).await.unwrap();
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 2);
        let mut decoder = decode(bytes).unwrap();
        assert_eq!(decoder.sample_rate(), 16_000);
        assert_eq!(decoder.channels(), 1);
        assert!(decoder.any(|sample| sample.abs() > 0.001));
    }

    #[tokio::test]
    async fn errors_are_bounded_redacted_and_not_cached() {
        let server = server().await;
        let cache = tempfile::tempdir().unwrap();
        let mut cfg = config(&server.endpoint);
        cfg.timeout_seconds = 1;
        let fetcher = Fetcher::new(cfg, Some(cache.path().into())).unwrap();
        for input in ["unauthorized", "json", "bad", "redirect", "timeout"] {
            let before = server.state.calls.load(Ordering::SeqCst);
            let error = fetcher.fetch(input, 100).await.unwrap_err().to_string();
            assert!(!error.contains("provider-secret") && !error.contains(&server.endpoint));
            assert_eq!(server.state.calls.load(Ordering::SeqCst), before + 1);
        }
        assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn retries_are_limited_to_retryable_status_codes() {
        let server = server().await;
        let mut cfg = config(&server.endpoint);
        cfg.retries = 2;
        let fetcher = Fetcher::new(cfg, None).unwrap();
        fetcher.fetch("retry", 100).await.unwrap();
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn oversized_content_length_is_rejected_before_reading_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/speech", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request).await;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_AUDIO_BYTES + 1).as_bytes()).await.unwrap();
        });
        let fetcher = Fetcher::new(config(&endpoint), None).unwrap();
        assert!(fetcher
            .fetch("body", 100)
            .await
            .unwrap_err()
            .to_string()
            .contains("32 MiB"));
        task.await.unwrap();
    }

    #[tokio::test]
    async fn cache_budget_evicts_only_owned_audio_files() {
        let server = server().await;
        let cache = tempfile::tempdir().unwrap();
        let mut cfg = config(&server.endpoint);
        cfg.cache_max_mb = 1;
        let old = cache.path().join(format!("{}.audio", "a".repeat(64)));
        std::fs::write(&old, vec![0; 1024 * 1024]).unwrap();
        std::fs::write(cache.path().join("keep.txt"), b"user file").unwrap();
        let fetcher = Fetcher::new(cfg, Some(cache.path().into())).unwrap();
        fetcher.fetch("new", 100).await.unwrap();
        assert!(!old.exists());
        assert!(cache.path().join("keep.txt").exists());
    }

    #[derive(Default)]
    struct PlayerState {
        plays: usize,
        paused: bool,
        stopped: bool,
    }
    struct FakePlayer(Arc<Mutex<PlayerState>>);
    impl Player for FakePlayer {
        fn play(&mut self, bytes: Audio, paused: bool) -> Result<()> {
            validate(bytes)?;
            let mut state = self.0.lock().unwrap();
            state.plays += 1;
            state.paused = paused;
            state.stopped = false;
            Ok(())
        }
        fn pause(&mut self, paused: bool) {
            self.0.lock().unwrap().paused = paused;
        }
        fn finished(&self) -> Result<bool> {
            Ok(false)
        }
        fn stop(&mut self) {
            self.0.lock().unwrap().stopped = true;
        }
    }
    fn wait_ready<P: Player>(speech: &mut HttpSpeech<P>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !speech.ready().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn pause_during_download_and_cancellation_never_start_unwanted_audio() {
        let runtime = Runtime::new().unwrap();
        let server = runtime.block_on(server());
        let state = Arc::new(Mutex::new(PlayerState::default()));
        let mut speech =
            HttpSpeech::with_player(config(&server.endpoint), None, FakePlayer(state.clone()))
                .unwrap();
        speech.speak("slow", 100).unwrap();
        assert!(!speech.ready().unwrap());
        speech.pause(true).unwrap();
        wait_ready(&mut speech);
        assert!(state.lock().unwrap().paused);
        speech.pause(false).unwrap();
        assert!(!state.lock().unwrap().paused);
        speech.speak("timeout", 100).unwrap();
        let start = Instant::now();
        speech.stop().unwrap();
        assert!(start.elapsed() < Duration::from_millis(100));
        assert!(speech.pending.is_none() && state.lock().unwrap().stopped);
        speech.speak("replacement", 150).unwrap();
        wait_ready(&mut speech);
        assert_eq!(state.lock().unwrap().plays, 2);
        speech.stop().unwrap();
    }
    #[test]
    fn prefetch_reuses_download_without_playing_early() {
        let runtime = Runtime::new().unwrap();
        let server = runtime.block_on(server());
        let state = Arc::new(Mutex::new(PlayerState::default()));
        let mut speech =
            HttpSpeech::with_player(config(&server.endpoint), None, FakePlayer(state.clone()))
                .unwrap();
        speech.prefetch("next", 100);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !speech.prefetch.as_ref().unwrap().1.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(state.lock().unwrap().plays, 0);
        // A new paragraph is a new generation, but an exact prefetched request is reusable.
        speech.reset(Some("next"), 100).unwrap();
        speech.speak("next", 100).unwrap();
        wait_ready(&mut speech);
        assert_eq!(state.lock().unwrap().plays, 1);
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[ignore = "opens default output device and plays a short tone from a local HTTP fixture"]
    fn http_audio_device_smoke() {
        let runtime = Runtime::new().unwrap();
        let server = runtime.block_on(server());
        let mut speech = HttpSpeech::new(config(&server.endpoint), None).unwrap();
        speech.speak("local test", 100).unwrap();
        speech.pause(true).unwrap();
        wait_ready(&mut speech);
        std::thread::sleep(Duration::from_millis(100));
        assert!(!speech.finished().unwrap());
        speech.pause(false).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !speech.finished().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        speech.stop().unwrap();
    }
}
