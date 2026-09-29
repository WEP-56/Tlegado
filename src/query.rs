//! Query state shared by search, discovery and the source picker.
//! A source advances only after a successful response; failures are retryable.
use reader_core::model::{book::Book, book_source::BookSource, search::SearchBook};
use std::collections::{HashMap, HashSet};

pub const RESULT_LIMIT: usize = 2000;

#[derive(Clone)]
pub struct Request {
    pub source: BookSource,
    pub page: i32,
}

struct Cursor {
    source: BookSource,
    page: i32,
    pending: bool,
    exhausted: bool,
    error: Option<String>,
    seen: HashSet<String>,
}

#[derive(Default)]
pub struct Query {
    pub keyword: String,
    pub explore: Option<String>,
    pub books: Vec<Book>,
    cursors: Vec<Cursor>,
    groups: HashMap<(String, String), usize>,
    count: usize,
}

pub fn identity(book: &Book) -> Option<(String, String)> {
    let name = normalize(&book.name);
    let author = normalize(&book.author);
    let author = author
        .strip_prefix("作者：")
        .or_else(|| author.strip_prefix("作者:"))
        .or_else(|| author.strip_prefix("作者"))
        .unwrap_or(&author)
        .trim_start_matches(['：', ':'])
        .to_owned();
    (!name.is_empty() && !author.is_empty()).then_some((name, author))
}

pub fn same_book(left: &Book, right: &Book) -> bool {
    match (identity(left), identity(right)) {
        (Some(a), Some(b)) => a == b,
        _ => left.origin == right.origin && left.book_url == right.book_url,
    }
}

pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn candidate(book: &Book) -> SearchBook {
    SearchBook {
        name: book.name.clone(),
        author: book.author.clone(),
        origin: book.origin.clone(),
        book_url: book.book_url.clone(),
        cover_url: book.cover_url.clone(),
        intro: book.intro.clone(),
        kind: book.kind.clone(),
        last_chapter: book.latest_chapter_title.clone(),
        word_count: book.word_count.clone(),
        update_time: book.update_time.clone(),
        ..Default::default()
    }
}

pub fn from_candidate(hit: &SearchBook, sources: &[BookSource]) -> Book {
    Book {
        name: hit.name.clone(),
        author: hit.author.clone(),
        origin: hit.origin.clone(),
        book_url: hit.book_url.clone(),
        origin_name: sources
            .iter()
            .find(|s| s.book_source_url == hit.origin)
            .map(|s| s.book_source_name.clone()),
        cover_url: hit.cover_url.clone(),
        intro: hit.intro.clone(),
        kind: hit.kind.clone(),
        latest_chapter_title: hit.last_chapter.clone(),
        word_count: hit.word_count.clone(),
        update_time: hit.update_time.clone(),
        ..Default::default()
    }
}

impl Query {
    pub fn new(sources: Vec<BookSource>, keyword: String, explore: Option<String>) -> Self {
        Self {
            keyword,
            explore,
            cursors: sources
                .into_iter()
                .map(|source| Cursor {
                    source,
                    page: 1,
                    pending: false,
                    exhausted: false,
                    error: None,
                    seen: HashSet::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    pub fn requests(&mut self, retry_only: bool) -> Vec<Request> {
        if self.count >= RESULT_LIMIT {
            return vec![];
        }
        self.cursors
            .iter_mut()
            .filter(|c| !c.pending && !c.exhausted && (!retry_only || c.error.is_some()))
            .map(|c| {
                c.pending = true;
                c.error = None;
                Request {
                    source: c.source.clone(),
                    page: c.page,
                }
            })
            .collect()
    }

    pub fn accept(&mut self, source: &str, page: i32, result: Result<Vec<Book>, String>) -> bool {
        let Some(cursor) = self
            .cursors
            .iter_mut()
            .find(|c| c.source.book_source_url == source && c.page == page && c.pending)
        else {
            return false;
        };
        cursor.pending = false;
        match result {
            Err(error) => {
                cursor.error = Some(format!(
                    "{} 第 {page} 页：{error}",
                    cursor.source.book_source_name
                ))
            }
            Ok(books) => {
                let mut fresh = Vec::new();
                for book in books {
                    if book.book_url.trim().is_empty() || self.count + fresh.len() >= RESULT_LIMIT {
                        continue;
                    }
                    if cursor.seen.insert(book.book_url.clone()) {
                        fresh.push(book);
                    }
                }
                // Many sources ignore {{page}} and return their first page again.
                cursor.exhausted = fresh.is_empty() || page == i32::MAX;
                cursor.page = page.saturating_add(1);
                for mut book in fresh {
                    self.count += 1;
                    let key = identity(&book);
                    if let Some(&index) = key.as_ref().and_then(|key| self.groups.get(key)) {
                        let group = &mut self.books[index];
                        let hits = group.source_candidates.get_or_insert_with(Vec::new);
                        if !hits
                            .iter()
                            .any(|h| h.origin == book.origin && h.book_url == book.book_url)
                        {
                            hits.push(candidate(&book));
                        }
                    } else {
                        if let Some(key) = key {
                            self.groups.insert(key, self.books.len());
                        }
                        book.source_candidates = Some(vec![candidate(&book)]);
                        self.books.push(book);
                    }
                }
            }
        }
        true
    }

    pub fn finish(&mut self) {
        for c in &mut self.cursors {
            if c.pending {
                c.pending = false;
                c.error = Some(format!(
                    "{} 第 {} 页任务未完成，可按 r 重试",
                    c.source.book_source_name, c.page
                ));
            }
        }
    }

    pub fn cancel(&mut self) {
        self.finish();
    }

    pub fn failures(&self) -> usize {
        self.cursors.iter().filter(|c| c.error.is_some()).count()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.cursors.iter().rev().find_map(|c| c.error.as_deref())
    }
    pub fn has_more(&self) -> bool {
        self.count < RESULT_LIMIT && self.cursors.iter().any(|c| !c.exhausted)
    }
    pub fn status(&self) -> String {
        let next = if self.count >= RESULT_LIMIT {
            "已达 2000 条候选上限"
        } else if self.has_more() {
            "n 继续加载 · r 重试失败页"
        } else {
            "已加载完毕"
        };
        format!(
            "{} 本 / {} 条候选 · {} 个书源失败 · {next}",
            self.books.len(),
            self.count,
            self.failures()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> Query {
        Query::new(
            ["a", "b"]
                .into_iter()
                .map(|key| BookSource {
                    book_source_url: key.into(),
                    book_source_name: key.into(),
                    ..Default::default()
                })
                .collect(),
            "测试".into(),
            None,
        )
    }

    fn book(source: &str, url: &str, name: &str, author: &str) -> Book {
        Book {
            origin: source.into(),
            book_url: url.into(),
            name: name.into(),
            author: author.into(),
            ..Default::default()
        }
    }

    #[test]
    fn independent_pages_retry_only_failed_sources_and_stop_repeated_pages() {
        let mut q = query();
        assert_eq!(q.requests(false).len(), 2);
        let first = book("a", "a/1", "同书", "测试作者");
        assert!(q.accept("a", 1, Ok(vec![first.clone()])));
        assert!(q.accept("b", 1, Err("timeout".into())));
        assert_eq!(q.failures(), 1);
        let retry = q.requests(true);
        assert_eq!(
            (
                retry.len(),
                retry[0].source.book_source_url.as_str(),
                retry[0].page
            ),
            (1, "b", 1)
        );
        q.accept("b", 1, Ok(vec![book("b", "b/1", "同书", "测试作者")]));
        assert_eq!(q.books.len(), 1);
        assert_eq!(q.books[0].source_candidates.as_ref().unwrap().len(), 2);
        assert_eq!(q.failures(), 0);
        assert!(q.requests(false).iter().all(|r| r.page == 2));
        // Ignore duplicate/wrong-page responses while the second page is pending.
        assert!(!q.accept("a", 1, Ok(vec![first.clone()])));
        q.accept("a", 2, Ok(vec![first]));
        q.accept("b", 2, Ok(vec![]));
        assert!(!q.has_more());
        assert!(q.requests(false).is_empty());
    }

    #[test]
    fn aggregation_normalizes_known_authors_but_does_not_merge_missing_or_different_authors() {
        let mut q = query();
        q.requests(false);
        q.accept(
            "a",
            1,
            Ok(vec![
                book("a", "a/1", "Book A", "作者： Writer"),
                book("a", "a/2", "孤本", ""),
            ]),
        );
        q.accept(
            "b",
            1,
            Ok(vec![
                book("b", "b/1", "booka", "writer"),
                book("b", "b/2", "孤本", ""),
                book("b", "b/3", "Book A", "other"),
            ]),
        );
        assert_eq!(q.books.len(), 4);
        assert_eq!(q.count, 5);
        assert_eq!(q.books[0].book_url, "a/1");
    }

    #[test]
    fn cancel_and_unfinished_tasks_keep_their_page_and_successful_results() {
        let mut q = query();
        q.requests(false);
        q.accept("a", 1, Ok(vec![book("a", "a/1", "书", "人")]));
        q.cancel();
        assert!(!q.accept("b", 1, Ok(vec![])));
        let next = q.requests(false);
        assert_eq!(next.iter().map(|r| r.page).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(q.books.len(), 1);
        q.finish();
        assert_eq!(q.failures(), 2);
        assert_eq!(q.requests(true).len(), 2);
    }

    #[test]
    fn limit_counts_candidates_not_just_merged_rows() {
        let mut q = query();
        q.requests(false);
        q.accept(
            "a",
            1,
            Ok((0..RESULT_LIMIT + 2)
                .map(|i| book("a", &format!("a/{i}"), "同书", "人"))
                .collect()),
        );
        assert_eq!(q.count, RESULT_LIMIT);
        assert_eq!(q.books.len(), 1);
        assert_eq!(
            q.books[0].source_candidates.as_ref().unwrap().len(),
            RESULT_LIMIT
        );
        assert!(!q.has_more());
        assert!(q.requests(false).is_empty());
        assert!(q.status().contains("2000"));
    }
}
