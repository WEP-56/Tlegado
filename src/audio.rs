//! The UI publishes desired playback state; one worker owns speech and audio playback.
use crate::app::{App, ToastTone};
use anyhow::{Context, Result};
use std::{
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(target_os = "windows")]
#[path = "audio/windows.rs"]
mod platform;

mod config;
mod http;
pub mod settings;
pub use config::Config;
#[cfg(unix)]
#[path = "audio/unix.rs"]
mod platform;

#[derive(Clone, Debug, Default)]
pub struct SpeechText {
    pub text: String,
    /// One chapter-local original Unicode scalar offset per spoken character.
    pub positions: Vec<usize>,
}

#[derive(Clone, Debug)]
struct Chunk {
    text: String,
    offset: usize,
}

// Keep individual submissions small for cancellation and Unix stdin pipe bounds.
const CHUNK_CHARS: usize = 480;
fn chunks(text: &str) -> Vec<Chunk> {
    let chars: Vec<char> = text.chars().collect();
    let mut result = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let limit = (start + CHUNK_CHARS).min(chars.len());
        let end = if limit < chars.len() {
            (start + CHUNK_CHARS / 2..limit)
                .rev()
                .find(|&i| {
                    matches!(
                        chars[i],
                        '。' | '！' | '？' | '；' | '.' | '!' | '?' | ';' | '\n'
                    ) || chars[i].is_whitespace()
                })
                .map_or(limit, |i| i + 1)
        } else {
            limit
        };
        let part = &chars[start..end];
        // Skip punctuation-only text without losing the offsets of later chunks.
        if part.iter().any(|ch| ch.is_alphanumeric()) {
            result.push(Chunk {
                text: part
                    .iter()
                    .map(|ch| if ch.is_control() { ' ' } else { *ch })
                    .collect(),
                offset: start,
            });
        }
        start = end;
    }
    result
}

trait Backend {
    fn speak(&mut self, text: &str, speed: u16) -> Result<()>;
    fn pause(&mut self, paused: bool) -> Result<()>;
    fn rate(&mut self, speed: u16) -> Result<()>;
    fn finished(&mut self) -> Result<bool>;
    fn stop(&mut self) -> Result<()>;
    fn ready(&mut self) -> Result<bool> {
        Ok(true)
    }
    fn prefetch(&mut self, _text: &str, _speed: u16) {}
    fn reset(&mut self, _next: Option<&str>, _speed: u16) -> Result<()> {
        self.stop()
    }
}

impl Backend for Box<dyn Backend> {
    fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
        (**self).speak(text, speed)
    }
    fn pause(&mut self, paused: bool) -> Result<()> {
        (**self).pause(paused)
    }
    fn rate(&mut self, speed: u16) -> Result<()> {
        (**self).rate(speed)
    }
    fn finished(&mut self) -> Result<bool> {
        (**self).finished()
    }
    fn stop(&mut self) -> Result<()> {
        (**self).stop()
    }
    fn ready(&mut self) -> Result<bool> {
        (**self).ready()
    }
    fn prefetch(&mut self, text: &str, speed: u16) {
        (**self).prefetch(text, speed);
    }
    fn reset(&mut self, next: Option<&str>, speed: u16) -> Result<()> {
        (**self).reset(next, speed)
    }
}

#[derive(Clone, Default)]
struct Desired {
    generation: u64,
    text: Option<Arc<SpeechText>>,
    prefetch: Option<Arc<String>>,
    paused: bool,
    speed: u16,
    shutdown: bool,
}

#[derive(Debug)]
enum Event {
    Preparing { generation: u64 },
    Started { generation: u64, position: usize },
    Finished { generation: u64 },
    Error { generation: u64, message: String },
}
impl Event {
    fn generation(&self) -> u64 {
        match self {
            Self::Preparing { generation }
            | Self::Started { generation, .. }
            | Self::Finished { generation }
            | Self::Error { generation, .. } => *generation,
        }
    }
}

struct Engine<B: Backend> {
    backend: Option<B>,
    generation: u64,
    chunks: Vec<Chunk>,
    chunk: usize,
    speaking: bool,
    reported: bool,
    terminal: bool,
    paused: bool,
    speed: u16,
}
impl<B: Backend> Engine<B> {
    fn new() -> Self {
        Self {
            backend: None,
            generation: 0,
            chunks: Vec::new(),
            chunk: 0,
            speaking: false,
            reported: false,
            terminal: true,
            paused: false,
            speed: 100,
        }
    }

    fn step(&mut self, desired: &Desired, create: &mut impl FnMut() -> Result<B>) -> Option<Event> {
        match self.update(desired, create) {
            Ok(event) => event,
            Err(error) => {
                if let Some(backend) = &mut self.backend {
                    let _ = backend.stop();
                }
                self.backend = None;
                self.speaking = false;
                self.terminal = true;
                Some(Event::Error {
                    generation: desired.generation,
                    message: format!("听书失败：{error:#}"),
                })
            }
        }
    }

    fn update(
        &mut self,
        desired: &Desired,
        create: &mut impl FnMut() -> Result<B>,
    ) -> Result<Option<Event>> {
        if self.generation != desired.generation {
            self.generation = desired.generation;
            self.terminal = true;
            let next_chunks: Vec<Chunk> = desired
                .text
                .as_ref()
                .map_or_else(Vec::new, |text| chunks(&text.text));
            if let Some(backend) = &mut self.backend {
                backend.reset(
                    next_chunks.first().map(|chunk| chunk.text.as_str()),
                    desired.speed,
                )?;
            }
            self.chunks = next_chunks;
            self.chunk = 0;
            self.speaking = false;
            self.reported = false;
            self.paused = false;
            self.terminal = desired.text.is_none();
        }
        if self.terminal {
            return Ok(None);
        }
        if self.speaking {
            let backend = self.backend.as_mut().expect("active speech engine");
            if self.paused != desired.paused {
                backend.pause(desired.paused)?;
                self.paused = desired.paused;
            }
            if self.speed != desired.speed {
                backend.rate(desired.speed)?;
                self.speed = desired.speed;
            }
            if !self.reported {
                if !backend.ready()? {
                    return Ok(None);
                }
                return Ok(Some(self.started(desired)));
            }
            if desired.paused || !backend.finished()? {
                return Ok(None);
            }
            self.speaking = false;
            self.chunk += 1;
        }
        if desired.paused {
            return Ok(None);
        }
        let Some(chunk) = self.chunks.get(self.chunk) else {
            self.terminal = true;
            return Ok(Some(Event::Finished {
                generation: self.generation,
            }));
        };
        if self.backend.is_none() {
            self.backend = Some(create()?);
        }
        self.backend
            .as_mut()
            .unwrap()
            .speak(&chunk.text, desired.speed)?;
        self.speaking = true;
        self.reported = false;
        self.paused = false;
        self.speed = desired.speed;
        if self.backend.as_mut().unwrap().ready()? {
            Ok(Some(self.started(desired)))
        } else {
            Ok(Some(Event::Preparing {
                generation: self.generation,
            }))
        }
    }

    fn started(&mut self, desired: &Desired) -> Event {
        self.reported = true;
        if !desired.paused {
            let next =
                self.chunks
                    .get(self.chunk + 1)
                    .map(|chunk| chunk.text.clone())
                    .or_else(|| {
                        desired.prefetch.as_ref().and_then(|text| {
                            chunks(text).into_iter().next().map(|chunk| chunk.text)
                        })
                    });
            if let Some(text) = next {
                self.backend
                    .as_mut()
                    .unwrap()
                    .prefetch(&text, desired.speed);
            }
        }
        let chunk = &self.chunks[self.chunk];
        let position = desired
            .text
            .as_ref()
            .and_then(|text| text.positions.get(chunk.offset))
            .copied()
            .unwrap_or(0);
        Event::Started {
            generation: self.generation,
            position,
        }
    }
}
impl<B: Backend> Drop for Engine<B> {
    fn drop(&mut self) {
        if let Some(backend) = &mut self.backend {
            let _ = backend.stop();
        }
    }
}

struct Worker {
    desired: Arc<(Mutex<Desired>, Condvar)>,
    events: mpsc::Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn start(config: Config, cache_dir: Option<std::path::PathBuf>) -> Result<Self> {
        Self::spawn(move || -> Result<Box<dyn Backend>> {
            match &config {
                Config::System { voice, .. } => {
                    Ok(Box::new(platform::SystemSpeech::with_voice(voice.clone())?))
                }
                Config::Http(config) => Ok(Box::new(http::HttpSpeech::new(
                    config.clone(),
                    cache_dir.clone(),
                )?)),
            }
        })
    }

    fn spawn<B: Backend + 'static>(
        mut create: impl FnMut() -> Result<B> + Send + 'static,
    ) -> Result<Self> {
        let desired = Arc::new((Mutex::new(Desired::default()), Condvar::new()));
        let shared = desired.clone();
        let (events, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("speech-playback".into())
            .spawn(move || {
                // Platform objects are constructed and dropped on this same thread.
                let mut engine = Engine::new();
                loop {
                    let (lock, wake) = &*shared;
                    let state = lock.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if state.shutdown {
                        break;
                    }
                    let mut initialize = || {
                        let backend = create()?;
                        let current = lock.lock().unwrap_or_else(|e| e.into_inner());
                        if current.shutdown || current.generation != state.generation {
                            anyhow::bail!("已取消语音初始化");
                        }
                        Ok(backend)
                    };
                    if let Some(event) = engine.step(&state, &mut initialize) {
                        if events.send(event).is_err() {
                            break;
                        }
                    }
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    // Do not sleep over a command delivered during synthesis initialization.
                    if guard.generation == state.generation
                        && guard.paused == state.paused
                        && guard.speed == state.speed
                        && !guard.shutdown
                    {
                        let _ = wake.wait_timeout(guard, Duration::from_millis(20));
                    }
                }
            })
            .context("无法启动语音线程")?;
        Ok(Self {
            desired,
            events: receiver,
            thread: Some(thread),
        })
    }

    fn send(&self, desired: Desired) {
        *self.desired.0.lock().unwrap_or_else(|e| e.into_inner()) = desired;
        self.desired.1.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct FakeState {
        log: Mutex<Vec<String>>,
        done: AtomicBool,
    }
    struct Fake(Arc<FakeState>);
    impl Backend for Fake {
        fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
            self.0
                .log
                .lock()
                .unwrap()
                .push(format!("speak:{text}:{speed}"));
            self.0.done.store(false, Ordering::SeqCst);
            Ok(())
        }
        fn pause(&mut self, paused: bool) -> Result<()> {
            self.0.log.lock().unwrap().push(format!("pause:{paused}"));
            Ok(())
        }
        fn rate(&mut self, speed: u16) -> Result<()> {
            self.0.log.lock().unwrap().push(format!("rate:{speed}"));
            Ok(())
        }
        fn finished(&mut self) -> Result<bool> {
            Ok(self.0.done.load(Ordering::SeqCst))
        }
        fn stop(&mut self) -> Result<()> {
            self.0.log.lock().unwrap().push("stop".into());
            Ok(())
        }
    }
    fn desired(generation: u64, text: &str) -> Desired {
        Desired {
            generation,
            text: Some(Arc::new(SpeechText {
                text: text.into(),
                positions: (50..50 + text.chars().count()).collect(),
            })),
            speed: 100,
            ..Default::default()
        }
    }

    #[test]
    fn chunks_preserve_unicode_and_skip_punctuation_without_using_terminal_lines() {
        let text = "中文😀 English。下一句很长。".repeat(100);
        let result = chunks(&text);
        assert_eq!(
            result.iter().map(|c| c.text.as_str()).collect::<String>(),
            text
        );
        for chunk in &result {
            assert!(chunk.text.chars().count() <= CHUNK_CHARS);
            assert!(text
                .chars()
                .skip(chunk.offset)
                .collect::<String>()
                .starts_with(&chunk.text));
        }
        assert!(chunks("…… ——\n！？\r\n　").is_empty());
        assert_eq!(chunks("文本\0尾部")[0].text, "文本 尾部");
    }

    #[test]
    fn delayed_audio_reports_preparing_until_ready_without_advancing_while_paused() {
        struct Delayed {
            fake: Fake,
            ready: Arc<AtomicBool>,
        }
        impl Backend for Delayed {
            fn speak(&mut self, text: &str, speed: u16) -> Result<()> {
                self.fake.speak(text, speed)
            }
            fn pause(&mut self, paused: bool) -> Result<()> {
                self.fake.pause(paused)
            }
            fn rate(&mut self, speed: u16) -> Result<()> {
                self.fake.rate(speed)
            }
            fn finished(&mut self) -> Result<bool> {
                self.fake.finished()
            }
            fn stop(&mut self) -> Result<()> {
                self.fake.stop()
            }
            fn ready(&mut self) -> Result<bool> {
                Ok(self.ready.load(Ordering::SeqCst))
            }
        }
        let state = Arc::new(FakeState::default());
        let ready = Arc::new(AtomicBool::new(false));
        let mut create = || {
            Ok(Delayed {
                fake: Fake(state.clone()),
                ready: ready.clone(),
            })
        };
        let mut engine = Engine::new();
        let mut request = desired(1, "网络段落");
        assert!(matches!(
            engine.step(&request, &mut create),
            Some(Event::Preparing { generation: 1 })
        ));
        request.paused = true;
        assert!(engine.step(&request, &mut create).is_none());
        ready.store(true, Ordering::SeqCst);
        assert!(matches!(
            engine.step(&request, &mut create),
            Some(Event::Started { generation: 1, .. })
        ));
        state.done.store(true, Ordering::SeqCst);
        assert!(engine.step(&request, &mut create).is_none());
        request.paused = false;
        assert!(matches!(
            engine.step(&request, &mut create),
            Some(Event::Finished { generation: 1 })
        ));
    }

    #[test]
    fn worker_pauses_resumes_changes_speed_and_finishes_exactly_once() {
        let state = Arc::new(FakeState::default());
        let mut create = || Ok(Fake(state.clone()));
        let mut engine = Engine::new();
        let mut request = desired(1, "第一段");
        assert!(matches!(
            engine.step(&request, &mut create),
            Some(Event::Started {
                generation: 1,
                position: 50
            })
        ));
        request.paused = true;
        assert!(engine.step(&request, &mut create).is_none());
        state.done.store(true, Ordering::SeqCst);
        request.speed = 150;
        assert!(engine.step(&request, &mut create).is_none());
        request.paused = false;
        assert!(matches!(
            engine.step(&request, &mut create),
            Some(Event::Finished { generation: 1 })
        ));
        assert!(engine.step(&request, &mut create).is_none());
        assert_eq!(
            *state.log.lock().unwrap(),
            ["speak:第一段:100", "pause:true", "rate:150", "pause:false"]
        );
    }

    #[test]
    fn navigation_purges_previous_speech_and_respects_paused_new_selection() {
        let state = Arc::new(FakeState::default());
        let mut create = || Ok(Fake(state.clone()));
        let mut engine = Engine::new();
        engine.step(&desired(1, "旧段"), &mut create);
        let mut next = desired(2, "新段");
        next.paused = true;
        assert!(engine.step(&next, &mut create).is_none());
        assert_eq!(*state.log.lock().unwrap(), ["speak:旧段:100", "stop"]);
        next.paused = false;
        assert!(matches!(
            engine.step(&next, &mut create),
            Some(Event::Started { generation: 2, .. })
        ));
        engine.step(
            &Desired {
                generation: 3,
                ..Default::default()
            },
            &mut create,
        );
        assert_eq!(state.log.lock().unwrap().last().unwrap(), "stop");
        assert!(engine
            .step(
                &Desired {
                    generation: 3,
                    ..Default::default()
                },
                &mut create
            )
            .is_none());
    }

    #[test]
    fn initialization_failure_waits_for_explicit_retry_and_empty_text_needs_no_device() {
        let mut engine = Engine::<Fake>::new();
        let mut attempts = 0;
        let mut create = || {
            attempts += 1;
            anyhow::bail!("没有语音设备")
        };
        assert!(matches!(
            engine.step(&desired(1, "……"), &mut create),
            Some(Event::Finished { .. })
        ));
        assert!(matches!(
            engine.step(&desired(2, "正文"), &mut create),
            Some(Event::Error { generation: 2, .. })
        ));
        assert!(engine.step(&desired(2, "正文"), &mut create).is_none());
        assert!(matches!(
            engine.step(&desired(3, "正文"), &mut create),
            Some(Event::Error { generation: 3, .. })
        ));
        assert_eq!(attempts, 2);
    }

    fn controller() -> (Controller, mpsc::Sender<Event>, App) {
        let (sender, events) = mpsc::channel();
        let worker = Worker {
            desired: Arc::new((Mutex::new(Desired::default()), Condvar::new())),
            events,
            thread: None,
        };
        let mut app = App::new();
        let mut book = app.books[0].clone();
        book.read = 0;
        book.total = 1;
        let mut reader = crate::reader::Reader::new(book, false);
        reader.set_real(vec!["章名".into()], "甲段\n乙段\n丙段".into(), 0);
        reader.start_aloud();
        app.reader = Some(reader);
        (
            Controller {
                prefetch: false,
                worker,
                key: None,
                state: Desired::default(),
            },
            sender,
            app,
        )
    }

    #[test]
    fn stale_completion_cannot_advance_manual_selection_and_pause_defers_completion() {
        let (mut controller, sender, mut app) = controller();
        controller.sync(&mut app);
        let old = controller.state.generation;
        app.reader.as_mut().unwrap().next_aloud_segment();
        sender.send(Event::Finished { generation: old }).unwrap();
        controller.sync(&mut app);
        assert_eq!(app.reader.as_ref().unwrap().speech_text().text, "乙段");
        app.reader.as_mut().unwrap().toggle_aloud();
        sender
            .send(Event::Finished {
                generation: controller.state.generation,
            })
            .unwrap();
        controller.sync(&mut app);
        assert_eq!(app.reader.as_ref().unwrap().speech_text().text, "乙段");
        app.reader.as_mut().unwrap().toggle_aloud();
        controller.sync(&mut app);
        assert_eq!(app.reader.as_ref().unwrap().speech_text().text, "丙段");
        sender
            .send(Event::Finished {
                generation: controller.state.generation,
            })
            .unwrap();
        controller.sync(&mut app);
        assert!(!app.reader.as_ref().unwrap().aloud_active());
        assert!(controller.state.text.is_none());
    }

    #[test]
    fn error_is_visible_retry_uses_new_generation_and_stop_cancels_audio() {
        let (mut controller, sender, mut app) = controller();
        controller.sync(&mut app);
        let old = controller.state.generation;
        sender
            .send(Event::Error {
                generation: old,
                message: "缺少中文声音".into(),
            })
            .unwrap();
        controller.sync(&mut app);
        assert_eq!(
            app.reader.as_ref().unwrap().aloud_error(),
            Some("缺少中文声音")
        );
        assert!(!app.reader.as_ref().unwrap().aloud_playing());
        app.reader.as_mut().unwrap().toggle_aloud();
        controller.sync(&mut app);
        assert!(controller.state.generation > old);
        assert!(app.reader.as_ref().unwrap().aloud_error().is_none());
        app.reader.as_mut().unwrap().stop_aloud();
        sender.send(Event::Finished { generation: old }).unwrap();
        controller.sync(&mut app);
        assert!(controller.state.text.is_none());
        assert!(!app.reader.as_ref().unwrap().aloud_active());
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.desired
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .shutdown = true;
        self.desired.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    book: String,
    chapter: u32,
    revision: u64,
}

pub struct Controller {
    prefetch: bool,
    worker: Worker,
    key: Option<Key>,
    state: Desired,
}
impl Controller {
    pub fn new(config: Config, cache_dir: Option<std::path::PathBuf>) -> Result<Self> {
        let prefetch = matches!(&config, Config::Http(http) if http.prefetch);
        Ok(Self {
            prefetch,
            worker: Worker::start(config, cache_dir)?,
            key: None,
            state: Desired::default(),
        })
    }

    fn current_key(app: &App) -> Option<Key> {
        app.reader
            .as_ref()
            .filter(|r| r.aloud_active() && !r.aloud_loading())
            .map(|r| Key {
                book: r.book.id.clone(),
                chapter: r.chapter,
                revision: r.aloud_revision(),
            })
    }

    fn publish(&mut self, app: &mut App) {
        let key = Self::current_key(app);
        if key != self.key {
            self.state.generation = self.state.generation.wrapping_add(1);
            self.key = key;
            self.state.prefetch = if self.prefetch && self.key.is_some() {
                app.reader
                    .as_ref()
                    .unwrap()
                    .next_speech_text()
                    .map(Arc::new)
            } else {
                None
            };
            self.state.text = if self.key.is_some() {
                let reader = app.reader.as_mut().unwrap();
                reader.set_aloud_preparing(true);
                Some(Arc::new(reader.speech_text()))
            } else {
                None
            };
        }
        if let Some(reader) = app.reader.as_ref().filter(|_| self.key.is_some()) {
            self.state.paused = !reader.aloud_playing();
            self.state.speed = reader.aloud_speed_percent();
        }
        self.worker.send(self.state.clone());
    }

    fn apply(&mut self, app: &mut App, event: Event) {
        if self.key.is_none()
            || Self::current_key(app) != self.key
            || event.generation() != self.state.generation
        {
            return;
        }
        let reader = app.reader.as_mut().unwrap();
        match event {
            Event::Preparing { .. } => reader.set_aloud_preparing(true),
            Event::Started { position, .. } => reader.set_aloud_progress(position),
            Event::Finished { .. } => {
                reader.set_aloud_preparing(false);
                // A completion arriving while paused is retained until resume.
                if reader.aloud_playing() {
                    reader.next_aloud_segment();
                } else {
                    reader.set_aloud_finished();
                }
            }
            Event::Error { message, .. } => {
                reader.fail_aloud(message.clone());
                app.toast(message, ToastTone::Err);
            }
        }
    }

    pub fn sync(&mut self, app: &mut App) {
        // Publish manual navigation first so queued completions cannot advance a new selection.
        self.publish(app);
        for _ in 0..64 {
            match self.worker.events.try_recv() {
                Ok(event) => self.apply(app, event),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(reader) = app.reader.as_mut().filter(|r| r.aloud_playing()) {
                        reader.fail_aloud("语音线程已结束，请重启程序后重试".into());
                    }
                    break;
                }
            }
        }
        self.publish(app);
    }
}
