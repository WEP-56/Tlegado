//! Domain adapter. Terminal code never performs HTTP or database operations.
use anyhow::{bail, Context, Result};
use reader_core::{
    crawler::http_client::HttpClient,
    model::{
        book::Book,
        book_chapter::BookChapter,
        book_source::{book_source_from_value, BookSource},
    },
    parser::rule_engine::RuleEngine,
    service::{book_service::BookService, book_source_service::BookSourceService},
    storage::{
        cache::file_cache::FileCache,
        db::{self, repo::BookSourceRepo},
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncWriteExt;

pub const NAMESPACE: &str = "default";

#[derive(Clone)]
pub struct Backend {
    pub books: Arc<BookService>,
    pub sources: BookSourceService,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub books: Vec<Book>,
    pub sources: Vec<BookSource>,
    pub categories: Vec<Vec<reader_core::model::book_source::ExploreKind>>,
}

#[derive(Clone, Debug)]
pub struct Reading {
    pub book: Book,
    pub chapters: Vec<BookChapter>,
    pub index: usize,
    pub text: String,
}

impl Backend {
    pub async fn open(path: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(path)
            .await
            .context("无法创建数据目录")?;
        let db_path = path.join("reader.db");
        let pool = db::init_pool(db_path.to_str().context("数据目录不是有效 UTF-8 路径")?).await?;
        let storage = path.to_str().context("数据目录不是有效 UTF-8 路径")?;
        let sources = BookSourceService::new(BookSourceRepo::new(pool), storage);
        let books = Arc::new(BookService::new(
            HttpClient::new(15, None)?,
            RuleEngine::new()?,
            FileCache::new(path.join("cache")),
            storage,
        ));
        Ok(Self { books, sources })
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let books = self.shelf().await?;
        let sources = self.sources.list(NAMESPACE).await?;
        let categories = sources
            .iter()
            .map(|s| self.books.explore_kinds(s).unwrap_or_default())
            .collect();
        Ok(Snapshot {
            books,
            sources,
            categories,
        })
    }

    pub async fn shelf(&self) -> Result<Vec<Book>> {
        let mut books = self.books.get_bookshelf(NAMESPACE).await?;
        books.sort_by_key(|b| std::cmp::Reverse(b.dur_chapter_time.unwrap_or(0)));
        Ok(books)
    }

    pub async fn import(&self, path: &Path) -> Result<usize> {
        if tokio::fs::metadata(path)
            .await
            .context("无法读取书源文件")?
            .len()
            > 16 * 1024 * 1024
        {
            bail!("书源文件超过 16 MiB，请拆分后导入");
        }
        let raw = tokio::fs::read_to_string(path)
            .await
            .context("书源必须为 UTF-8 JSON 文件")?;
        let sources = parse_sources(&raw)?;
        let count = sources.len();
        self.sources.save_many(NAMESPACE, sources).await?;
        Ok(count)
    }

    pub async fn export(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(&self.sources.list(NAMESPACE).await?)?;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
            .context("无法导出：请选择不存在的新文件路径")?;
        file.write_all(&json).await?;
        file.sync_all().await?;
        Ok(())
    }

    pub async fn source(&self, key: &str) -> Result<BookSource> {
        self.sources
            .get(NAMESPACE, key)
            .await?
            .context("找不到该书籍的书源，请重新导入")
    }

    pub async fn set_sources(&self, keys: &[String], explore: bool, enabled: bool) -> Result<()> {
        let mut sources = self.sources.list(NAMESPACE).await?;
        let keys: std::collections::HashSet<_> = keys.iter().collect();
        sources.retain(|s| keys.contains(&s.book_source_url));
        for source in &mut sources {
            if explore {
                source.enabled_explore = Some(enabled);
            } else {
                source.enabled = Some(enabled);
            }
        }
        self.sources.save_many(NAMESPACE, sources).await?;
        Ok(())
    }

    pub async fn search(
        &self,
        source: &BookSource,
        keyword: &str,
        explore: Option<&str>,
    ) -> Result<Vec<Book>> {
        let results = if let Some(url) = explore {
            self.books.explore_book(NAMESPACE, source, url, 1).await?
        } else {
            self.books
                .search_book(NAMESPACE, source, keyword, 1)
                .await?
        };
        Ok(results
            .into_iter()
            .filter(|b| !b.book_url.trim().is_empty())
            .take(200)
            .map(|b| Book {
                name: b.name,
                author: b.author,
                book_url: b.book_url,
                origin: source.book_source_url.clone(),
                origin_name: Some(source.book_source_name.clone()),
                intro: b.intro,
                kind: b.kind,
                latest_chapter_title: b.last_chapter,
                cover_url: b.cover_url,
                word_count: b.word_count,
                ..Default::default()
            })
            .collect())
    }

    pub async fn read(&self, mut book: Book) -> Result<Reading> {
        if let Some(saved) = self.books.get_shelf_book(NAMESPACE, &book.book_url).await? {
            if saved.origin == book.origin {
                book = saved;
            }
        }
        let source = self.source(&book.origin).await?;
        if book.toc_url.as_deref().is_none_or(str::is_empty) {
            let info = self
                .books
                .get_book_info(NAMESPACE, &source, &book.book_url)
                .await?;
            if !info.name.is_empty() {
                book.name = info.name;
            }
            if !info.author.is_empty() {
                book.author = info.author;
            }
            if info.intro.is_some() {
                book.intro = info.intro;
            }
            book.toc_url = info
                .toc_url
                .filter(|s| !s.is_empty())
                .or_else(|| Some(book.book_url.clone()));
        }
        let chapters = self
            .books
            .get_chapter_list(NAMESPACE, &source, book.toc_url.as_deref().unwrap())
            .await?;
        if chapters.is_empty() {
            bail!("目录为空，请检查书源目录规则");
        }
        let index = (book.dur_chapter_index.unwrap_or(0).max(0) as usize).min(chapters.len() - 1);
        book.total_chapter_num = Some(chapters.len().min(i32::MAX as usize) as i32);
        let text = self.chapter(&book, &chapters[index]).await?;
        Ok(Reading {
            book,
            chapters,
            index,
            text,
        })
    }

    pub async fn chapter(&self, book: &Book, chapter: &BookChapter) -> Result<String> {
        let source = self.source(&book.origin).await?;
        let text = self
            .books
            .get_content(NAMESPACE, &book.book_url, &source, &chapter.url)
            .await?;
        if text.trim().is_empty() {
            bail!("正文为空，请检查书源正文规则");
        }
        Ok(reader_core::export::html_to_plain_text(&text).replace("\r\n", "\n"))
    }

    pub async fn save_progress(
        &self,
        mut book: Book,
        index: usize,
        position: usize,
        title: String,
    ) -> Result<()> {
        book.dur_chapter_index = Some(index.min(i32::MAX as usize) as i32);
        book.dur_chapter_pos = Some(position.min(i32::MAX as usize) as i32);
        book.dur_chapter_title = Some(title);
        book.dur_chapter_time = Some(chrono::Utc::now().timestamp_millis());
        self.books.save_book(NAMESPACE, book).await?;
        Ok(())
    }
}

pub fn parse_sources(raw: &str) -> Result<Vec<BookSource>> {
    let value: serde_json::Value =
        serde_json::from_str(raw.trim_start_matches('\u{feff}')).context("书源 JSON 格式错误")?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(_) => vec![value],
        _ => bail!("书源 JSON 必须为对象或数组"),
    };
    if items.is_empty() {
        bail!("书源列表为空");
    }
    let mut unique = BTreeMap::new();
    for (index, item) in items.into_iter().enumerate() {
        let source = book_source_from_value(item)
            .with_context(|| format!("第 {} 个书源格式错误", index + 1))?;
        if source.book_source_name.trim().is_empty() || source.book_source_url.trim().is_empty() {
            bail!(
                "第 {} 个书源缺少 bookSourceName 或 bookSourceUrl",
                index + 1
            );
        }
        unique.insert(source.book_source_url.clone(), source);
    }
    Ok(unique.into_values().collect())
}

#[derive(Default, Debug)]
pub struct Options {
    pub data_dir: PathBuf,
    pub imports: Vec<PathBuf>,
    pub demo: bool,
    pub import_only: bool,
    pub help: bool,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Self> {
        let mut result = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--data-dir") => {
                    result.data_dir = args.next().context("--data-dir 缺少路径")?.into()
                }
                Some("--import-source") => result
                    .imports
                    .push(args.next().context("--import-source 缺少路径")?.into()),
                Some("--demo") => result.demo = true,
                Some("--import-only") => result.import_only = true,
                Some("--help" | "-h") => result.help = true,
                _ => bail!("未知参数 {}，使用 --help 查看用法", arg.to_string_lossy()),
            }
        }
        if result.demo && (!result.imports.is_empty() || result.import_only) {
            bail!("--demo 不能与导入参数同时使用");
        }
        if result.import_only && result.imports.is_empty() {
            bail!("--import-only 需要 --import-source");
        }
        if result.data_dir.as_os_str().is_empty() && !result.help && !result.demo {
            result.data_dir = default_data_dir()?;
        }
        Ok(result)
    }
}

fn default_data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("TLEGADO_DATA_DIR").filter(|p| !p.is_empty()) {
        return Ok(path.into());
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .context("找不到用户目录，请通过 --data-dir 指定数据目录")?;
    Ok(PathBuf::from(home).join(".tlegado"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{response::Html, routing::get, Router};
    use serde_json::json;

    #[test]
    fn imports_validate_entire_batch_and_keep_legacy_compatibility() {
        let raw = json!([
            {"bookSourceName":"旧源", "bookSourceUrl":"https://fixture.invalid", "ruleSearchUrl":"/search?key=searchKey", "ruleSearchList":".book", "ruleSearchName":"a@text"},
            {"bookSourceName":"新源", "bookSourceUrl":"https://fixture.invalid", "searchUrl":"/search?key={{key}}"}
        ]).to_string();
        let sources = parse_sources(&format!("\u{feff}{raw}")).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].book_source_name, "新源");
        let old = parse_sources(r#"{"bookSourceName":"旧源","bookSourceUrl":"https://fixture.invalid","ruleSearchUrl":"/s?key=searchKey","ruleSearchList":".book"}"#).unwrap();
        assert!(old[0].search_url.as_ref().unwrap().contains("{{key}}"));
        assert!(parse_sources(r#"[{"bookSourceName":"正常","bookSourceUrl":"x"},{}]"#).is_err());
        assert!(parse_sources("[]").is_err());
        assert!(parse_sources("null").is_err());
    }

    #[tokio::test]
    async fn batch_source_changes_persist_and_preserve_books() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        let sources = (0..3)
            .map(|i| BookSource {
                book_source_name: format!("源{i}"),
                book_source_url: format!("source-{i}"),
                enabled: Some(true),
                enabled_explore: Some(true),
                ..Default::default()
            })
            .collect();
        backend.sources.save_many(NAMESPACE, sources).await.unwrap();
        backend
            .save_progress(
                Book {
                    name: "保留的书".into(),
                    book_url: "book".into(),
                    origin: "source-0".into(),
                    ..Default::default()
                },
                0,
                12,
                "正文".into(),
            )
            .await
            .unwrap();
        let keys = vec!["source-0".into(), "source-2".into()];
        backend.set_sources(&keys, false, false).await.unwrap();
        backend.set_sources(&keys, true, false).await.unwrap();
        drop(backend);
        let reopened = Backend::open(temp.path()).await.unwrap();
        assert!(!reopened.source("source-0").await.unwrap().is_enabled());
        assert_eq!(
            reopened.source("source-2").await.unwrap().enabled_explore,
            Some(false)
        );
        assert!(reopened.source("source-1").await.unwrap().is_enabled());
        reopened
            .sources
            .delete_many(NAMESPACE, &keys)
            .await
            .unwrap();
        assert_eq!(reopened.sources.list(NAMESPACE).await.unwrap().len(), 1);
        assert_eq!(reopened.shelf().await.unwrap()[0].dur_chapter_pos, Some(12));
        drop(reopened);
        let reopened = Backend::open(temp.path()).await.unwrap();
        assert!(reopened.source("source-0").await.is_err());
        assert_eq!(reopened.sources.list(NAMESPACE).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_search_read_restart_and_offline_cache() {
        let router = Router::new()
            .route("/search", get(|| async { Html(r#"<div class="book"><a href="/book">测试小说</a><span class="author">测试作者</span></div>"#) }))
            .route("/book", get(|| async { Html(r#"<h1>测试小说</h1><span id="author">测试作者</span><a id="toc" href="/toc">目录</a>"#) }))
            .route("/toc", get(|| async { Html(r#"<a class="chapter" href="/c1">第一章</a><a class="chapter" href="/c2">第二章</a>"#) }))
            .route("/c1", get(|| async { Html("<div id='content'>第一章的真实测试正文。中文与 English。</div>") }))
            .route("/c2", get(|| async { Html("<div id='content'>第二章的真实测试正文。离线缓存。</div>") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap()
        });
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("含 空格书源.json");
        let source = json!({
            "bookSourceName":"本地测试源", "bookSourceUrl":base, "searchUrl":"/search?key={{key}}",
            "exploreUrl":"推荐::/search",
            "ruleSearch":{"bookList":".book", "name":"a@text", "author":".author@text", "bookUrl":"a@href"},
            "ruleBookInfo":{"name":"h1@text", "author":"#author@text", "tocUrl":"#toc@href"},
            "ruleToc":{"chapterList":"a.chapter", "chapterName":"text", "chapterUrl":"href"},
            "ruleContent":{"content":"#content@text"}
        });
        tokio::fs::write(&source_path, source.to_string())
            .await
            .unwrap();
        let data = temp.path().join("数据目录");
        let backend = Backend::open(&data).await.unwrap();
        assert!(backend.snapshot().await.unwrap().books.is_empty());
        assert_eq!(backend.import(&source_path).await.unwrap(), 1);
        let mut snapshot = backend.snapshot().await.unwrap();
        assert_eq!(snapshot.categories[0][0].title, "推荐");
        let source = snapshot.sources.remove(0);
        let explore = backend
            .search(&source, "", snapshot.categories[0][0].url.as_deref())
            .await
            .unwrap();
        assert_eq!(explore.len(), 1);
        let results = backend.search(&source, "测试", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].origin, base);
        let reading = backend.read(results[0].clone()).await.unwrap();
        assert_eq!(reading.chapters.len(), 2);
        assert!(reading.text.contains("第一章的真实测试正文"));
        let text = backend
            .chapter(&reading.book, &reading.chapters[1])
            .await
            .unwrap();
        assert!(text.contains("第二章"));
        backend
            .save_progress(
                reading.book.clone(),
                1,
                8,
                reading.chapters[1].title.clone(),
            )
            .await
            .unwrap();
        let export = temp.path().join("导出.json");
        backend.export(&export).await.unwrap();
        assert!(
            backend.export(&export).await.is_err(),
            "never overwrite an existing export"
        );
        let before = tokio::fs::read(&export).await.unwrap();
        assert_eq!(
            parse_sources(std::str::from_utf8(&before).unwrap())
                .unwrap()
                .len(),
            1
        );
        // Invalid batches do not modify already imported source data.
        tokio::fs::write(
            &source_path,
            r#"[{"bookSourceName":"新","bookSourceUrl":"new"},{}]"#,
        )
        .await
        .unwrap();
        assert!(backend.import(&source_path).await.is_err());
        assert_eq!(backend.snapshot().await.unwrap().sources.len(), 1);
        stop.send(()).unwrap();
        server.await.unwrap();
        drop(backend);
        let reopened = Backend::open(&data).await.unwrap();
        let books = reopened.snapshot().await.unwrap().books;
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].dur_chapter_index, Some(1));
        assert_eq!(books[0].dur_chapter_pos, Some(8));
        let cached = reopened.read(books[0].clone()).await.unwrap();
        assert_eq!(cached.index, 1);
        assert!(cached.text.contains("离线缓存"));
    }
}
