//! Tracked background reads and ordered writes. Cancelling a read suppresses
//! its result and unscheduled requests; in-flight HTTP requests finish normally
//! so the upstream serial-rate state is released safely.
use crate::backend::{Backend, Reading, Snapshot, NAMESPACE};
use crate::query::Request;
use anyhow::{Context, Result};
use reader_core::model::{book::Book, book_chapter::BookChapter};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc as async_mpsc, Semaphore},
    task::{JoinHandle, JoinSet},
};

pub enum Command {
    ImportRules(String),
    ImportLayout(String),
    Query {
        id: u64,
        requests: Vec<Request>,
        keyword: String,
        explore: Option<String>,
    },
    Open {
        id: u64,
        book: Book,
    },
    Switch {
        id: u64,
        previous: Box<Book>,
        book: Book,
    },
    CommitSwitch {
        id: u64,
        previous: Book,
        reading: Box<Reading>,
    },
    Chapter {
        id: u64,
        book: Book,
        chapter: BookChapter,
        index: usize,
        end: bool,
    },
    Cancel,
    Import(PathBuf),
    ImportLocal(PathBuf),
    Library(crate::library::Change),
    Export(PathBuf),
    SetSources {
        keys: Vec<String>,
        explore: bool,
        enabled: bool,
    },
    DeleteSources(Vec<String>),
    Add(Book),
    Progress {
        book: Book,
        index: usize,
        position: usize,
        title: String,
    },
    Shutdown,
}

pub enum Event {
    Library(crate::library::Library),
    Snapshot(Snapshot),
    QueryPart {
        id: u64,
        source: String,
        page: i32,
        result: Result<Vec<Book>, String>,
    },
    QueryDone(u64),
    Opened {
        id: u64,
        result: Result<Box<Reading>, String>,
    },
    Switched {
        id: u64,
        result: Result<Box<Reading>, String>,
    },
    Chapter {
        id: u64,
        index: usize,
        end: bool,
        result: Result<String, String>,
    },
    Shelf(Vec<Book>),
    Notice(String),
    Error(String),
    Fatal(String),
}

pub struct Bridge {
    tx: async_mpsc::UnboundedSender<Command>,
    rx: mpsc::Receiver<Event>,
    runtime: Option<Runtime>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl Bridge {
    pub fn start(path: PathBuf, imports: Vec<PathBuf>) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?;
        let (tx, commands) = async_mpsc::unbounded_channel();
        let (events, rx) = mpsc::channel();
        let worker = runtime.spawn(async move {
            let result = run(path, imports, commands, events.clone()).await;
            if let Err(error) = &result {
                let _ = events.send(Event::Fatal(format!("{error:#}")));
            }
            result
        });
        Ok(Self {
            tx,
            rx,
            runtime: Some(runtime),
            worker: Some(worker),
        })
    }
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx
            .send(command)
            .map_err(|_| anyhow::anyhow!("后台任务已退出"))
    }
    pub fn poll(&self) -> impl Iterator<Item = Event> + '_ {
        self.rx.try_iter()
    }
    pub fn finish(mut self) -> Result<()> {
        let _ = self.tx.send(Command::Shutdown);
        let runtime = self.runtime.take().unwrap();
        let result = runtime
            .block_on(self.worker.take().unwrap())
            .context("后台任务异常退出")?;
        runtime.shutdown_timeout(Duration::from_secs(2));
        result
    }
}

async fn run(
    path: PathBuf,
    imports: Vec<PathBuf>,
    mut commands: async_mpsc::UnboundedReceiver<Command>,
    tx: mpsc::Sender<Event>,
) -> Result<()> {
    let backend = Backend::open(&path).await.context("初始化本地存储失败")?;
    for path in imports {
        match backend.import(&path).await {
            Ok(n) => {
                let _ = tx.send(Event::Notice(format!("已导入 {n} 个书源")));
            }
            Err(e) => {
                let _ = tx.send(Event::Error(format!("导入失败：{e:#}")));
            }
        }
    }
    let _ = tx.send(Event::Library(backend.library().await?));
    let _ = tx.send(Event::Snapshot(backend.snapshot().await?));
    let query_generation = Arc::new(AtomicU64::new(0));
    let read_generation = Arc::new(AtomicU64::new(0));
    let permits = Arc::new(Semaphore::new(4));
    let mut tasks = JoinSet::new();
    let mut write_errors = std::collections::HashMap::new();
    loop {
        let command = tokio::select! {
            command = commands.recv() => match command { Some(c) => c, None => break },
            result = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = result { let _ = tx.send(Event::Error(format!("后台读取任务失败：{error}"))); }
                continue;
            }
        };
        match command {
            Command::Query {
                id,
                requests,
                keyword,
                explore,
            } => {
                query_generation.store(id, Ordering::SeqCst);
                let (b, tx, generation, permits) = (
                    backend.clone(),
                    tx.clone(),
                    query_generation.clone(),
                    permits.clone(),
                );
                tasks.spawn(async move {
                    let mut queries = JoinSet::new();
                    // Only four source futures are scheduled at a time.
                    let mut sources = requests.into_iter();
                    loop {
                        while queries.len() < 4 && generation.load(Ordering::SeqCst) == id {
                            let Some(Request { source, page }) = sources.next() else {
                                break;
                            };
                            let (b, tx, permits, generation, key, explore) = (
                                b.clone(),
                                tx.clone(),
                                permits.clone(),
                                generation.clone(),
                                keyword.clone(),
                                explore.clone(),
                            );
                            queries.spawn(async move {
                                let _permit = permits.acquire_owned().await.unwrap();
                                if generation.load(Ordering::SeqCst) != id {
                                    return;
                                }
                                let result = b
                                    .search(&source, &key, explore.as_deref(), page)
                                    .await
                                    .map_err(|e| e.to_string());
                                if generation.load(Ordering::SeqCst) == id {
                                    let _ = tx.send(Event::QueryPart {
                                        id,
                                        source: source.book_source_url,
                                        page,
                                        result,
                                    });
                                }
                            });
                        }
                        match queries.join_next().await {
                            Some(Err(e)) => {
                                let _ = tx.send(Event::QueryPart {
                                    id,
                                    source: "后台任务".into(),
                                    page: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                            Some(Ok(())) => {}
                            None => break,
                        }
                    }
                    let _ = tx.send(Event::QueryDone(id));
                });
            }
            Command::Open { id, book } => {
                read_generation.store(id, Ordering::SeqCst);
                let (b, tx, generation, permits) = (
                    backend.clone(),
                    tx.clone(),
                    read_generation.clone(),
                    permits.clone(),
                );
                tasks.spawn(async move {
                    let _permit = permits.acquire_owned().await.unwrap();
                    if generation.load(Ordering::SeqCst) != id {
                        return;
                    }
                    let result = b.read(book).await.map(Box::new).map_err(|e| e.to_string());
                    if generation.load(Ordering::SeqCst) == id {
                        let _ = tx.send(Event::Opened { id, result });
                    }
                });
            }
            Command::Switch { id, previous, book } => {
                read_generation.store(id, Ordering::SeqCst);
                let (b, tx, generation, permits) = (
                    backend.clone(),
                    tx.clone(),
                    read_generation.clone(),
                    permits.clone(),
                );
                tasks.spawn(async move {
                    let _permit = permits.acquire_owned().await.unwrap();
                    if generation.load(Ordering::SeqCst) != id {
                        return;
                    }
                    let result = b
                        .prepare_switch(&previous, book)
                        .await
                        .map(Box::new)
                        .map_err(|e| e.to_string());
                    if generation.load(Ordering::SeqCst) == id {
                        let _ = tx.send(Event::Opened { id, result });
                    }
                });
            }
            Command::Chapter {
                id,
                book,
                chapter,
                index,
                end,
            } => {
                read_generation.store(id, Ordering::SeqCst);
                let (b, tx, generation, permits) = (
                    backend.clone(),
                    tx.clone(),
                    read_generation.clone(),
                    permits.clone(),
                );
                tasks.spawn(async move {
                    let _permit = permits.acquire_owned().await.unwrap();
                    if generation.load(Ordering::SeqCst) != id {
                        return;
                    }
                    let result = b.chapter(&book, &chapter).await.map_err(|e| e.to_string());
                    if generation.load(Ordering::SeqCst) == id {
                        let _ = tx.send(Event::Chapter {
                            id,
                            index,
                            end,
                            result,
                        });
                    }
                });
            }
            Command::Cancel => {
                query_generation.store(0, Ordering::SeqCst);
                read_generation.store(0, Ordering::SeqCst);
            }
            Command::Shutdown => break,
            command => {
                let progress_key = match &command {
                    Command::Progress { book, .. } => Some(book.book_url.clone()),
                    Command::Library(
                        crate::library::Change::Preference { .. }
                        | crate::library::Change::ResetPreferences,
                    ) => Some("tui_preferences".into()),
                    _ => None,
                };
                let result: Result<()> = async {
                    match command {
                        Command::ImportRules(location) => {
                            let message = backend.import_rules(&location).await?;
                            let _ = tx.send(Event::Library(backend.library().await?));
                            let _ = tx.send(Event::Notice(message));
                        }
                        Command::ImportLayout(location) => {
                            let message = backend.import_layout(&location).await?;
                            let _ = tx.send(Event::Library(backend.library().await?));
                            let _ = tx.send(Event::Notice(message));
                        }
                        Command::CommitSwitch {
                            id,
                            previous,
                            mut reading,
                        } => {
                            let result = backend
                                .books
                                .replace_book_source(NAMESPACE, &previous, reading.book.clone())
                                .await;
                            let result = match result {
                                Ok(book) => {
                                    reading.book = book;
                                    Ok(reading)
                                }
                                Err(error) => {
                                    write_errors.insert(
                                        previous.book_url.clone(),
                                        format!("换源保存失败：{error}"),
                                    );
                                    Err(error.to_string())
                                }
                            };
                            let success = result.is_ok();
                            let _ = tx.send(Event::Switched { id, result });
                            if success {
                                write_errors.remove(&previous.book_url);
                                let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            }
                        }
                        Command::Import(path) => {
                            let count = backend.import(&path).await?;
                            let _ = tx.send(Event::Snapshot(backend.snapshot().await?));
                            let _ = tx.send(Event::Notice(format!("已导入 {count} 个书源")));
                        }
                        Command::Export(path) => {
                            backend.export(&path).await?;
                            let _ =
                                tx.send(Event::Notice(format!("书源已导出到 {}", path.display())));
                        }
                        Command::ImportLocal(path) => {
                            let book = backend.import_local(&path).await?;
                            let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            let _ = tx.send(Event::Notice(format!(
                                "已导入《{}》，可从书架打开",
                                book.name
                            )));
                        }
                        Command::Library(change) => {
                            let removal = match &change {
                                crate::library::Change::RemoveBook(book) => {
                                    Some(crate::backend::is_local(book))
                                }
                                _ => None,
                            };
                            let result = backend.change_library(change).await;
                            if removal.is_some() {
                                let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            }
                            result?;
                            let _ = tx.send(Event::Library(backend.library().await?));
                            let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            let _ = tx.send(Event::Notice(
                                match removal {
                                    Some(true) => "已删除本地书及导入副本，原文件保留",
                                    Some(false) => "已移出书架",
                                    None => "设置已保存",
                                }
                                .into(),
                            ));
                        }
                        Command::SetSources {
                            keys,
                            explore,
                            enabled,
                        } => {
                            backend.set_sources(&keys, explore, enabled).await?;
                            let _ = tx.send(Event::Snapshot(backend.snapshot().await?));
                            let _ = tx.send(Event::Notice(format!("已更新 {} 个书源", keys.len())));
                        }
                        Command::DeleteSources(keys) => {
                            backend.sources.delete_many(NAMESPACE, &keys).await?;
                            let _ = tx.send(Event::Snapshot(backend.snapshot().await?));
                            let _ = tx.send(Event::Notice(format!("已删除 {} 个书源", keys.len())));
                        }
                        Command::Add(book) => {
                            backend.books.save_book(NAMESPACE, book).await?;
                            let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            let _ = tx.send(Event::Notice("已加入书架".into()));
                        }
                        Command::Progress {
                            book,
                            index,
                            position,
                            title,
                        } => {
                            backend.save_progress(book, index, position, title).await?;
                            let _ = tx.send(Event::Shelf(backend.shelf().await?));
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    let message = format!("保存/导入失败：{error:#}");
                    if let Some(key) = progress_key {
                        write_errors.insert(key, message.clone());
                    }
                    let _ = tx.send(Event::Error(message));
                } else if let Some(key) = progress_key {
                    write_errors.remove(&key);
                }
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if !write_errors.is_empty() {
        anyhow::bail!(write_errors.into_values().collect::<Vec<_>>().join("; "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flushes_source_commit_and_reports_a_failed_commit() {
        let temp = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(temp.path().to_owned(), vec![]).unwrap();
        let original = Book {
            name: "换源持久化".into(),
            author: "测试作者".into(),
            origin: "https://a.invalid".into(),
            book_url: "https://a.invalid/book".into(),
            ..Default::default()
        };
        let replacement = Book {
            origin: "https://b.invalid".into(),
            book_url: "https://b.invalid/book".into(),
            dur_chapter_index: Some(3),
            dur_chapter_pos: Some(0),
            ..original.clone()
        };
        bridge.send(Command::Add(original.clone())).unwrap();
        bridge
            .send(Command::Progress {
                book: original.clone(),
                index: 2,
                position: 15,
                title: "旧章".into(),
            })
            .unwrap();
        bridge
            .send(Command::CommitSwitch {
                id: 9,
                previous: original.clone(),
                reading: Box::new(Reading {
                    book: replacement.clone(),
                    index: 3,
                    chapters: vec![],
                    text: "已加载的新正文".into(),
                }),
            })
            .unwrap();
        bridge.finish().unwrap();
        Runtime::new().unwrap().block_on(async {
            let backend = Backend::open(temp.path()).await.unwrap();
            let shelf = backend.shelf().await.unwrap();
            assert_eq!(shelf.len(), 1);
            assert_eq!(shelf[0].book_url, replacement.book_url);
            assert_eq!(shelf[0].dur_chapter_index, Some(3));
            assert_eq!(shelf[0].source_candidates.as_ref().unwrap().len(), 2);
        });
        let bridge = Bridge::start(temp.path().to_owned(), vec![]).unwrap();
        bridge
            .send(Command::CommitSwitch {
                id: 10,
                previous: original,
                reading: Box::new(Reading {
                    book: replacement,
                    index: 3,
                    chapters: vec![],
                    text: "正文".into(),
                }),
            })
            .unwrap();
        assert!(
            bridge.finish().is_err(),
            "exit must report an unacknowledged failed commit"
        );
    }

    #[test]
    fn bridge_passes_each_sources_page_and_returns_stable_source_ids() {
        use axum::{extract::Query, response::Html, routing::get, Router};
        use reader_core::model::book_source::BookSource;
        use std::collections::HashMap;
        let runtime = Runtime::new().unwrap();
        let (base, server) = runtime.block_on(async {
            let router = Router::new().route(
                "/search",
                get(|Query(params): Query<HashMap<String, String>>| async move {
                    let page = params.get("page").unwrap();
                    Html(format!("<a href='/book/{page}'>第{page}页</a>"))
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            (
                base,
                tokio::spawn(async move {
                    axum::serve(listener, router).await.unwrap();
                }),
            )
        });
        let temp = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(temp.path().to_owned(), vec![]).unwrap();
        let requests = [1, 3].into_iter().map(|page| {
            let source: BookSource = serde_json::from_value(serde_json::json!({
                "bookSourceName":"同名书源", "bookSourceUrl":format!("{base}/source-{page}"), "searchUrl":"/search?page={{page}}",
                "ruleSearch":{"bookList":"a", "name":"text", "bookUrl":"href"}
            })).unwrap();
            Request { source, page }
        }).collect();
        bridge
            .send(Command::Query {
                id: 7,
                requests,
                keyword: "书".into(),
                explore: None,
            })
            .unwrap();
        let mut pages = vec![];
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut done = false;
        while !done && std::time::Instant::now() < deadline {
            for event in bridge.poll() {
                match event {
                    Event::QueryPart {
                        id,
                        source,
                        page,
                        result,
                    } => {
                        assert_eq!(id, 7);
                        assert_eq!(source, format!("{base}/source-{page}"));
                        assert!(result.unwrap()[0]
                            .book_url
                            .ends_with(&format!("/book/{page}")));
                        pages.push(page);
                    }
                    Event::QueryDone(7) => done = true,
                    Event::Fatal(error) => panic!("{error}"),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        bridge.finish().unwrap();
        server.abort();
        assert!(done, "query timed out");
        pages.sort_unstable();
        assert_eq!(pages, vec![1, 3]);
    }

    #[test]
    fn shutdown_flushes_queued_progress_in_order() {
        let temp = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(temp.path().to_owned(), vec![]).unwrap();
        let book = Book {
            name: "退出保存测试".into(),
            author: "作者".into(),
            origin: "https://fixture.invalid".into(),
            book_url: "https://fixture.invalid/book".into(),
            ..Default::default()
        };
        bridge.send(Command::Add(book.clone())).unwrap();
        for position in [3, 41, 87] {
            bridge
                .send(Command::Progress {
                    book: book.clone(),
                    index: 2,
                    position,
                    title: "第三章".into(),
                })
                .unwrap();
        }
        bridge.finish().unwrap();
        Runtime::new().unwrap().block_on(async {
            let backend = Backend::open(temp.path()).await.unwrap();
            let books = backend.shelf().await.unwrap();
            assert_eq!(books.len(), 1);
            assert_eq!(books[0].dur_chapter_pos, Some(87));
            assert_eq!(books[0].dur_chapter_index, Some(2));
        });
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::library::Change;
    use reader_core::model::replace_rule::ReplaceRule;

    #[test]
    fn shutdown_drains_local_import_and_ordered_configuration_changes() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("书.txt");
        std::fs::write(
            &original,
            "第一章
正文内容",
        )
        .unwrap();
        let data = temp.path().join("data");
        let bridge = Bridge::start(data.clone(), vec![]).unwrap();
        bridge.send(Command::ImportLocal(original)).unwrap();
        for _ in 0..3 {
            bridge
                .send(Command::Library(Change::Preference { index: 0, step: 1 }))
                .unwrap();
        }
        bridge
            .send(Command::Library(Change::SaveGroup {
                id: 0,
                name: "收藏".into(),
            }))
            .unwrap();
        bridge
            .send(Command::Library(Change::SaveRule(ReplaceRule {
                name: "去尾".into(),
                pattern: "求票".into(),
                is_enabled: true,
                ..Default::default()
            })))
            .unwrap();
        bridge.finish().unwrap();
        Runtime::new().unwrap().block_on(async {
            let backend = Backend::open(&data).await.unwrap();
            assert_eq!(backend.shelf().await.unwrap().len(), 1);
            let library = backend.library().await.unwrap();
            assert_eq!(library.prefs.0[0], 3);
            assert_eq!(library.groups[0].group_name, "收藏");
            assert_eq!(library.rules[0].pattern, "求票");
        });
    }

    #[test]
    fn failed_preference_write_is_reported_at_exit_and_valid_retry_clears_it() {
        let temp = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(temp.path().into(), vec![]).unwrap();
        bridge
            .send(Command::Library(Change::Preference {
                index: usize::MAX,
                step: 1,
            }))
            .unwrap();
        assert!(bridge.finish().is_err());
        let bridge = Bridge::start(temp.path().into(), vec![]).unwrap();
        bridge
            .send(Command::Library(Change::Preference {
                index: usize::MAX,
                step: 1,
            }))
            .unwrap();
        bridge
            .send(Command::Library(Change::ResetPreferences))
            .unwrap();
        bridge.finish().unwrap();
    }
}
