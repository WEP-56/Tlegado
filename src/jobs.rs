//! Tracked background reads and ordered writes. Cancelling a read suppresses
//! its result and unscheduled requests; in-flight HTTP requests finish normally
//! so the upstream serial-rate state is released safely.
use crate::backend::{Backend, Reading, Snapshot, NAMESPACE};
use anyhow::{Context, Result};
use reader_core::model::{book::Book, book_chapter::BookChapter, book_source::BookSource};
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
    Query {
        id: u64,
        sources: Vec<BookSource>,
        keyword: String,
        explore: Option<String>,
    },
    Open {
        id: u64,
        book: Book,
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
    Export(PathBuf),
    SetSources {
        keys: Vec<String>,
        explore: bool,
        enabled: bool,
    },
    DeleteSources(Vec<String>),
    Add(Book),
    Remove(Book),
    Progress {
        book: Book,
        index: usize,
        position: usize,
        title: String,
    },
    Shutdown,
}

pub enum Event {
    Snapshot(Snapshot),
    QueryPart {
        id: u64,
        source: String,
        result: Result<Vec<Book>, String>,
    },
    QueryDone(u64),
    Opened {
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
                sources,
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
                    let mut sources = sources.into_iter();
                    loop {
                        while queries.len() < 4 && generation.load(Ordering::SeqCst) == id {
                            let Some(source) = sources.next() else { break };
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
                                    .search(&source, &key, explore.as_deref())
                                    .await
                                    .map_err(|e| e.to_string());
                                if generation.load(Ordering::SeqCst) == id {
                                    let _ = tx.send(Event::QueryPart {
                                        id,
                                        source: source.book_source_name,
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
                    _ => None,
                };
                let result: Result<()> = async {
                    match command {
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
                        Command::Remove(book) => {
                            backend.books.delete_book(NAMESPACE, &book).await?;
                            let _ = tx.send(Event::Shelf(backend.shelf().await?));
                            let _ = tx.send(Event::Notice("已移出书架".into()));
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
