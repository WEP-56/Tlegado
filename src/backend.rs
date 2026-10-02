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
    service::{
        book_group_service::BookGroupService,
        book_service::BookService,
        book_source_service::BookSourceService,
        json_document_service::JsonDocumentService,
        local_epub_book::{LocalEpubBookService, MAX_EPUB_UPLOAD_BYTES},
        local_txt_book::{LocalTxtBookService, MAX_TXT_UPLOAD_BYTES},
    },
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

pub fn is_local(book: &Book) -> bool {
    matches!(book.origin.as_str(), "local-txt" | "local-epub")
}

#[derive(Clone)]
pub struct Backend {
    pub books: Arc<BookService>,
    pub sources: BookSourceService,
    pub documents: Arc<JsonDocumentService>,
    pub groups: Arc<BookGroupService>,
    txt: LocalTxtBookService,
    epub: LocalEpubBookService,
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
        let documents = Arc::new(JsonDocumentService::new(pool.clone(), storage));
        let groups = Arc::new(BookGroupService::new(documents.clone()));
        let sources = BookSourceService::new(BookSourceRepo::new(pool), storage);
        let books = Arc::new(BookService::new(
            HttpClient::new(15, None)?,
            RuleEngine::new()?,
            FileCache::new(path.join("cache")),
            storage,
        ));
        Ok(Self {
            books,
            sources,
            documents,
            groups,
            txt: LocalTxtBookService::new(path),
            epub: LocalEpubBookService::new(path),
        })
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let books = self.shelf().await?;
        let sources = self.sources.list(NAMESPACE).await?;
        // Category scripts may perform HTTP. Load only the selected source in
        // a background job, never while importing or refreshing all sources.
        let categories = vec![Vec::new(); sources.len()];
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

    pub async fn import_local(&self, path: &Path) -> Result<Book> {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let limit = match extension.as_str() {
            "txt" => MAX_TXT_UPLOAD_BYTES,
            "epub" => MAX_EPUB_UPLOAD_BYTES,
            _ => bail!("仅支持 TXT 和 EPUB 文件"),
        };
        let metadata = tokio::fs::metadata(path)
            .await
            .context("无法读取本地书籍文件")?;
        if !metadata.is_file() || metadata.len() > limit as u64 {
            bail!("请选择不超过 {} MiB 的文件", limit / 1024 / 1024);
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("文件名不是有效 UTF-8")?;
        let bytes = tokio::fs::read(path).await?;
        let imported = if extension == "txt" {
            self.txt.import_txt_book(NAMESPACE, name, &bytes).await?
        } else {
            self.epub.import_epub_book(NAMESPACE, name, &bytes).await?
        };
        let book = self
            .books
            .get_shelf_book(NAMESPACE, &imported.book_url)
            .await?
            .unwrap_or(imported);
        self.books.save_book(NAMESPACE, book.clone()).await?;
        Ok(book)
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
        page: i32,
    ) -> Result<Vec<Book>> {
        if page < 1 {
            bail!("页码必须大于 0");
        }
        let results = if let Some(url) = explore {
            self.books
                .explore_book(NAMESPACE, source, url, page)
                .await?
        } else {
            self.books
                .search_book(NAMESPACE, source, keyword, page)
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
        let saved = self.books.get_shelf_book(NAMESPACE, &book.book_url).await?;
        let saved = match saved {
            Some(saved) => Some(saved),
            None if !is_local(&book) => {
                self.books
                    .find_shelf_book_by_name_author(NAMESPACE, &book.name, &book.author)
                    .await?
            }
            None => None,
        };
        if let Some(mut saved) = saved {
            if (saved.origin != book.origin || saved.book_url != book.book_url)
                && crate::query::same_book(&saved, &book)
            {
                return self.prepare_switch(&saved, book).await;
            }
            if saved.origin == book.origin {
                let candidates = saved.source_candidates.get_or_insert_with(Vec::new);
                for hit in book.source_candidates.into_iter().flatten() {
                    if !candidates
                        .iter()
                        .any(|c| c.origin == hit.origin && c.book_url == hit.book_url)
                    {
                        candidates.push(hit);
                    }
                }
                book = saved;
            }
        }
        self.read_at(book, None).await
    }

    pub async fn prepare_switch(&self, previous: &Book, mut book: Book) -> Result<Reading> {
        if !crate::query::same_book(previous, &book) {
            bail!("候选书籍的书名或作者不匹配");
        }
        book.toc_url = None;
        book.group = previous.group;
        book.custom_cover_url = previous.custom_cover_url.clone();
        book.can_update = previous.can_update;
        book.dur_chapter_index = previous.dur_chapter_index;
        book.dur_chapter_title = previous.dur_chapter_title.clone();
        book.dur_chapter_time = Some(chrono::Utc::now().timestamp_millis());
        // Text lengths differ between sources, so resume at the chapter start.
        book.dur_chapter_pos = Some(0);
        self.read_at(book, Some(previous)).await
    }

    async fn read_at(&self, mut book: Book, previous: Option<&Book>) -> Result<Reading> {
        let chapters = if book.origin == "local-txt" {
            self.txt.get_chapter_list(NAMESPACE, &book.book_url).await?
        } else if book.origin == "local-epub" {
            self.epub
                .get_chapter_list(NAMESPACE, &book.book_url)
                .await?
        } else {
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
            chapters
        };
        if chapters.is_empty() {
            bail!("目录为空，请检查书源目录规则");
        }
        let fallback =
            (book.dur_chapter_index.unwrap_or(0).max(0) as usize).min(chapters.len() - 1);
        let index = previous
            .and_then(|b| b.dur_chapter_title.as_deref())
            .filter(|title| !title.trim().is_empty())
            .and_then(|title| {
                chapters.iter().position(|c| {
                    crate::query::normalize(&c.title) == crate::query::normalize(title)
                })
            })
            .unwrap_or(fallback);
        book.dur_chapter_index = Some(index as i32);
        book.dur_chapter_title = Some(chapters[index].title.clone());
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
        if book.origin == "local-txt" {
            return Ok(self
                .txt
                .get_content(NAMESPACE, &chapter.url)
                .await?
                .replace("\r\n", "\n"));
        }
        let text = if book.origin == "local-epub" {
            self.epub.get_content(NAMESPACE, &chapter.url).await?
        } else {
            let source = self.source(&book.origin).await?;
            self.books
                .get_content(NAMESPACE, &book.book_url, &source, &chapter.url)
                .await?
        };
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
        let Some(saved) = self.books.get_shelf_book(NAMESPACE, &book.book_url).await? else {
            // Reading a preview or receiving a stale queued write must not add a book.
            return Ok(());
        };
        book.group = saved.group;
        book.dur_chapter_index = Some(index.min(i32::MAX as usize) as i32);
        book.dur_chapter_pos = Some(position.min(i32::MAX as usize) as i32);
        book.dur_chapter_title = Some(title);
        book.dur_chapter_time = Some(chrono::Utc::now().timestamp_millis());
        self.books.save_book(NAMESPACE, book).await?;
        Ok(())
    }

    pub async fn remove_book(&self, book: &Book) -> Result<()> {
        // Only the managed, hashed import paths are touched, never the original file.
        // Persist removal first: a shelf-write failure must not destroy a readable copy.
        self.books.delete_book(NAMESPACE, book).await?;
        match book.origin.as_str() {
            "local-txt" => {
                self.txt
                    .delete_book_files(NAMESPACE, &book.book_url)
                    .await
                    .context("书架已移除，但本地副本清理失败，请检查数据目录权限")?;
            }
            "local-epub" => {
                self.epub
                    .delete_book_files(NAMESPACE, &book.book_url)
                    .await
                    .context("书架已移除，但本地副本清理失败，请检查数据目录权限")?;
            }
            _ => {}
        }
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
    pub tts_config: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub imports: Vec<PathBuf>,
    pub local_imports: Vec<PathBuf>,
    pub rule_imports: Vec<String>,
    pub layout_imports: Vec<String>,
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
                Some("--tts-config") => {
                    result.tts_config = Some(args.next().context("--tts-config 缺少路径")?.into())
                }
                Some("--import-rules") => result.rule_imports.push(
                    args.next()
                        .context("--import-rules 缺少路径或 URL")?
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("路径必须是 UTF-8"))?,
                ),
                Some("--import-layout") => result.layout_imports.push(
                    args.next()
                        .context("--import-layout 缺少路径或 URL")?
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("路径必须是 UTF-8"))?,
                ),
                Some("--import-book") => result
                    .local_imports
                    .push(args.next().context("--import-book 缺少路径")?.into()),
                Some("--import-only") => result.import_only = true,
                Some("--help" | "-h") => result.help = true,
                _ => bail!("未知参数 {}，使用 --help 查看用法", arg.to_string_lossy()),
            }
        }
        if result.demo
            && (!result.imports.is_empty()
                || !result.local_imports.is_empty()
                || !result.rule_imports.is_empty()
                || !result.layout_imports.is_empty()
                || result.import_only)
        {
            bail!("--demo 不能与导入参数同时使用");
        }
        if result.import_only
            && result.imports.is_empty()
            && result.local_imports.is_empty()
            && result.rule_imports.is_empty()
            && result.layout_imports.is_empty()
        {
            bail!("--import-only 需要 --import-source、--import-book、--import-rules 或 --import-layout");
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

    #[tokio::test]
    async fn previews_and_stale_progress_never_create_shelf_entries() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        let book = Book {
            name: "试读".into(),
            origin: "https://fixture.invalid".into(),
            book_url: "https://fixture.invalid/book".into(),
            ..Default::default()
        };
        backend
            .save_progress(book.clone(), 2, 91, "第三章".into())
            .await
            .unwrap();
        assert!(backend.shelf().await.unwrap().is_empty());
        backend
            .books
            .save_book(NAMESPACE, book.clone())
            .await
            .unwrap();
        backend
            .save_progress(book.clone(), 2, 91, "第三章".into())
            .await
            .unwrap();
        assert_eq!(backend.shelf().await.unwrap()[0].dur_chapter_pos, Some(91));
        backend.remove_book(&book).await.unwrap();
        backend
            .save_progress(book, 3, 123, "过期写入".into())
            .await
            .unwrap();
        assert!(backend.shelf().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn local_delete_removes_only_managed_copy_and_can_reimport() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("原始小说.txt");
        let text = "第一章 开始\n原始文件需要保留。";
        tokio::fs::write(&original, text).await.unwrap();
        let backend = Backend::open(&temp.path().join("data")).await.unwrap();
        let book = backend.import_local(&original).await.unwrap();
        assert!(backend.read(book.clone()).await.is_ok());
        backend.remove_book(&book).await.unwrap();
        assert!(backend.shelf().await.unwrap().is_empty());
        assert_eq!(tokio::fs::read_to_string(&original).await.unwrap(), text);
        assert!(backend.read(book.clone()).await.is_err());
        backend
            .save_progress(book.clone(), 0, 11, "过期".into())
            .await
            .unwrap();
        assert!(backend.shelf().await.unwrap().is_empty());
        backend.remove_book(&book).await.unwrap();
        let imported = backend.import_local(&original).await.unwrap();
        assert!(backend.read(imported).await.is_ok());
    }

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
    async fn discovery_snapshot_does_not_execute_dynamic_category_scripts() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        let source = BookSource {
            book_source_name: "延迟加载分类".into(),
            book_source_url: "https://fixture.invalid/discovery".into(),
            explore_url: Some("@js:source.setVariable('must not run during snapshot');throw Error('category failure')".into()),
            enabled_explore: Some(true),
            ..Default::default()
        };
        backend
            .sources
            .save(NAMESPACE, source.clone())
            .await
            .unwrap();
        let snapshot = backend.snapshot().await.unwrap();
        assert_eq!(snapshot.sources.len(), 1);
        assert!(snapshot.categories[0].is_empty());
        assert_eq!(
            backend
                .books
                .source_runtime(NAMESPACE, &source)
                .session
                .variable(),
            ""
        );
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
            .books
            .save_book(
                NAMESPACE,
                Book {
                    name: "保留的书".into(),
                    book_url: "book".into(),
                    origin: "source-0".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
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
    async fn paginated_queries_and_source_switch_preserve_shelf_and_restart_progress() {
        use axum::extract::Query;
        use std::collections::HashMap;
        let router = Router::new()
            .route("/search", get(|Query(params): Query<HashMap<String, String>>| async move {
                let page = params.get("page").map(String::as_str).unwrap_or("missing");
                match page {
                    "1" => Html("<div class='book'><a href='/book-a'>换源小说</a><span class='author'>测试作者</span></div>"),
                    "2" => Html("<div class='book'><a href='/book-b'>换源小说</a><span class='author'>测试作者</span></div>"),
                    _ => Html("<div>没有更多</div>"),
                }
            }))
            .route("/book-a", get(|| async { Html("<h1>换源小说</h1><a id='toc' href='/toc-a'>目录</a>") }))
            .route("/book-b", get(|| async { Html("<h1>换源小说</h1><a id='toc' href='/toc-b'>目录</a>") }))
            .route("/broken", get(|| async { Html("<a id='toc' href='/toc-broken'>目录</a>") }))
            .route("/toc-a", get(|| async { Html("<a class='chapter' href='/a1'>第一章</a><a class='chapter' href='/a2'>第二章 归来</a>") }))
            .route("/toc-b", get(|| async { Html("<a class='chapter' href='/b0'>序章</a><a class='chapter' href='/b1'>第一章</a><a class='chapter' href='/b2'>第二章归来</a>") }))
            .route("/toc-broken", get(|| async { Html("<a class='chapter' href='/empty'>第二章归来</a>") }))
            .route("/a1", get(|| async { Html("<div id='content'>旧源第一章</div>") }))
            .route("/a2", get(|| async { Html("<div id='content'>旧源第二章</div>") }))
            .route("/b0", get(|| async { Html("<div id='content'>新源序章</div>") }))
            .route("/b1", get(|| async { Html("<div id='content'>新源第一章</div>") }))
            .route("/b2", get(|| async { Html("<div id='content'>新源第二章归来</div>") }))
            .route("/empty", get(|| async { Html("<div id='content'></div>") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let source: BookSource = serde_json::from_value(json!({
            "bookSourceName":"分页测试源", "bookSourceUrl":base, "searchUrl":"/search?key={{key}}&page={{page}}",
            "ruleSearch":{"bookList":".book", "name":"a@text", "author":".author@text", "bookUrl":"a@href"},
            "ruleExplore":{"bookList":".book", "name":"a@text", "author":".author@text", "bookUrl":"a@href"},
            "ruleBookInfo":{"name":"h1@text", "tocUrl":"#toc@href"},
            "ruleToc":{"chapterList":"a.chapter", "chapterName":"text", "chapterUrl":"href"},
            "ruleContent":{"content":"#content@text"}
        })).unwrap();
        let mut second_source = source.clone();
        second_source.book_source_url = format!("{base}/source-b");
        second_source.book_source_name = "第二源".into();
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(temp.path()).await.unwrap();
        backend
            .sources
            .save_many(NAMESPACE, vec![source.clone(), second_source.clone()])
            .await
            .unwrap();
        assert!(backend.search(&source, "书", None, 0).await.is_err());
        let a = backend
            .search(&source, "书", None, 1)
            .await
            .unwrap()
            .remove(0);
        let b = backend
            .search(&second_source, "书", None, 2)
            .await
            .unwrap()
            .remove(0);
        assert!(a.book_url.ends_with("/book-a"));
        assert!(b.book_url.ends_with("/book-b"));
        assert!(backend
            .search(&source, "书", None, 3)
            .await
            .unwrap()
            .is_empty());
        let explore = backend
            .search(&source, "", Some("/search?page={{page}}"), 2)
            .await
            .unwrap();
        assert!(explore[0].book_url.ends_with("/book-b"));
        let mut old = backend.read(a).await.unwrap().book;
        old.group = Some(8);
        backend
            .books
            .save_book(NAMESPACE, old.clone())
            .await
            .unwrap();
        backend
            .save_progress(old.clone(), 1, 5, "第二章 归来".into())
            .await
            .unwrap();
        old = backend.shelf().await.unwrap().remove(0);
        let mut broken = b.clone();
        broken.book_url = format!("{base}/broken");
        assert!(backend.prepare_switch(&old, broken).await.is_err());
        assert_eq!(backend.shelf().await.unwrap()[0].book_url, old.book_url);
        let prepared = backend.prepare_switch(&old, b.clone()).await.unwrap();
        assert_eq!(
            backend.read(b.clone()).await.unwrap().index,
            2,
            "opening an alternate search hit must also resume progress"
        );
        assert_eq!(prepared.index, 2, "chapter title wins over old index");
        assert_eq!(prepared.book.dur_chapter_pos, Some(0));
        assert_eq!(prepared.book.group, Some(8));
        assert!(prepared.text.contains("新源第二章"));
        assert_eq!(
            backend.shelf().await.unwrap()[0].book_url,
            old.book_url,
            "preparation is read-only"
        );
        let mut unmatched = old.clone();
        unmatched.dur_chapter_title = Some("不存在".into());
        unmatched.dur_chapter_index = Some(99);
        assert_eq!(
            backend
                .prepare_switch(&unmatched, b.clone())
                .await
                .unwrap()
                .index,
            2
        );
        let mut wrong = b;
        wrong.author = "另一作者".into();
        assert!(backend.prepare_switch(&old, wrong).await.is_err());
        let saved = backend
            .books
            .replace_book_source(NAMESPACE, &old, prepared.book)
            .await
            .unwrap();
        assert_eq!(backend.shelf().await.unwrap().len(), 1);
        assert_eq!(saved.source_candidates.as_ref().unwrap().len(), 2);
        assert!(backend
            .books
            .replace_book_source(NAMESPACE, &old, saved.clone())
            .await
            .is_err());
        server.abort();
        let _ = server.await;
        drop(backend);
        let reopened = Backend::open(temp.path()).await.unwrap();
        let shelf = reopened.shelf().await.unwrap();
        assert_eq!(shelf.len(), 1);
        assert_eq!(shelf[0].origin, second_source.book_source_url);
        assert_eq!(shelf[0].dur_chapter_index, Some(2));
        assert_eq!(shelf[0].dur_chapter_pos, Some(0));
        assert_eq!(shelf[0].group, Some(8));
        assert_eq!(shelf[0].source_candidates.as_ref().unwrap().len(), 2);
        let restored = reopened.read(shelf[0].clone()).await.unwrap();
        assert_eq!(restored.index, 2);
        assert!(restored.text.contains("新源第二章"));
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
        assert!(snapshot.categories[0].is_empty());
        let categories = backend.books.explore_kinds(&snapshot.sources[0]).unwrap();
        assert_eq!(categories[0].title, "推荐");
        let source = snapshot.sources.remove(0);
        let explore = backend
            .search(&source, "", categories[0].url.as_deref(), 1)
            .await
            .unwrap();
        assert_eq!(explore.len(), 1);
        let results = backend.search(&source, "测试", None, 1).await.unwrap();
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
            .books
            .save_book(NAMESPACE, reading.book.clone())
            .await
            .unwrap();
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

#[cfg(test)]
mod local_tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn epub() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let files = [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#,
            ),
            (
                "OPS/book.opf",
                r#"<package version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>离线测试</dc:title><dc:creator>作者</dc:creator></metadata><manifest><item id="c1" href="one.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#,
            ),
            (
                "OPS/one.xhtml",
                "<html><head><title>起点</title></head><body><p>首章正文。</p></body></html>",
            ),
            (
                "OPS/two.xhtml",
                "<html><head><title>后续</title></head><body><p>第二章中文正文。</p></body></html>",
            ),
        ];
        for (name, content) in files {
            zip.start_file(
                name,
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[tokio::test]
    async fn epub_removal_preserves_original_and_clears_managed_book() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("原文件.epub");
        let bytes = epub();
        tokio::fs::write(&original, &bytes).await.unwrap();
        let backend = Backend::open(&temp.path().join("data")).await.unwrap();
        let book = backend.import_local(&original).await.unwrap();
        backend.remove_book(&book).await.unwrap();
        assert!(backend.shelf().await.unwrap().is_empty());
        assert!(backend.read(book).await.is_err());
        assert_eq!(tokio::fs::read(&original).await.unwrap(), bytes);
    }

    #[tokio::test]
    async fn txt_epub_import_reimport_and_restart_work_without_original_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library");
        let backend = Backend::open(&path).await.unwrap();
        for (name, content) in [
            (
                "含空格 书.TXT",
                "第一章 开始
正文 <原文> & 内容。
第二章 继续
保存阅读位置的正文。"
                    .as_bytes()
                    .to_vec(),
            ),
            ("本地 EPUB.epub", epub()),
        ] {
            let original = temp.path().join(name);
            tokio::fs::write(&original, content).await.unwrap();
            let book = backend.import_local(&original).await.unwrap();
            assert!(is_local(&book));
            let reading = backend.read(book.clone()).await.unwrap();
            assert_eq!(reading.chapters.len(), 2);
            if name.ends_with("TXT") {
                assert!(reading.text.contains("<原文>"));
            } else {
                assert!(reading.text.contains("首章正文"));
                assert!(!reading.text.contains("<p>"));
            }
            let second = backend.chapter(&book, &reading.chapters[1]).await.unwrap();
            assert!(second.contains("正文"));
            backend
                .save_progress(book.clone(), 1, 5, reading.chapters[1].title.clone())
                .await
                .unwrap();
            let again = backend.import_local(&original).await.unwrap();
            assert_eq!(again.book_url, book.book_url);
            assert_eq!(again.dur_chapter_pos, Some(5));
            tokio::fs::remove_file(&original).await.unwrap();
            let reopened = Backend::open(&path)
                .await
                .unwrap()
                .read(book)
                .await
                .unwrap();
            assert_eq!(reopened.index, 1);
            assert_eq!(reopened.book.dur_chapter_pos, Some(5));
            assert_eq!(reopened.text, second);
        }
        assert_eq!(backend.shelf().await.unwrap().len(), 2);
        assert!(backend.sources.list(NAMESPACE).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_local_imports_never_create_shelf_entries() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Backend::open(&temp.path().join("data")).await.unwrap();
        for (name, content) in [
            ("bad.epub", b"not zip".as_slice()),
            ("empty.txt", b"  "),
            ("ignored.pdf", b"pdf"),
        ] {
            let path = temp.path().join(name);
            tokio::fs::write(&path, content).await.unwrap();
            assert!(backend.import_local(&path).await.is_err());
        }
        let huge = temp.path().join("too-large.txt");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_TXT_UPLOAD_BYTES as u64 + 1)
            .unwrap();
        assert!(backend
            .import_local(&huge)
            .await
            .unwrap_err()
            .to_string()
            .contains("MiB"));
        assert!(backend.shelf().await.unwrap().is_empty());
    }

    #[test]
    fn cli_local_imports_accept_multiple_paths_and_reject_missing_path_or_demo() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(std::ffi::OsString::from));
        let options = parse(&[
            "--import-book",
            "a b.txt",
            "--import-book",
            "book.epub",
            "--import-only",
            "--data-dir",
            "data",
        ])
        .unwrap();
        assert_eq!(options.local_imports.len(), 2);
        assert!(options.import_only);
        assert!(parse(&["--import-book"]).is_err());
        assert!(parse(&["--demo", "--import-book", "book.txt"]).is_err());
    }
}
