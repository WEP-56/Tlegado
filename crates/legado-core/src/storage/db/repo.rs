use crate::error::error::AppError;
use crate::model::book_source::BookSource;
use crate::util::time::now_ts;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct BookSourceRepo {
    pool: SqlitePool,
}

impl BookSourceRepo {
    /// Import a validated batch atomically; never leave half an import saved.
    pub async fn upsert_many(&self, user_ns: &str, sources: &[BookSource]) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        for source in sources {
            let json = serde_json::to_string(source).map_err(|e| AppError::BadRequest(e.to_string()))?;
            sqlx::query("INSERT INTO book_sources (user_ns, book_source_url, book_source_name, json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(user_ns, book_source_url) DO UPDATE SET book_source_name=excluded.book_source_name, json=excluded.json, updated_at=excluded.updated_at")
                .bind(user_ns).bind(&source.book_source_url).bind(&source.book_source_name)
                .bind(json).bind(now_ts()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(
        &self,
        user_ns: &str,
        source: &BookSource,
        json: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO book_sources (user_ns, book_source_url, book_source_name, json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(user_ns, book_source_url) DO UPDATE SET book_source_name=excluded.book_source_name, json=excluded.json, updated_at=excluded.updated_at"
        )
        .bind(user_ns)
        .bind(&source.book_source_url)
        .bind(&source.book_source_name)
        .bind(json)
        .bind(now_ts())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, user_ns: &str, book_source_url: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM book_sources WHERE user_ns=?1 AND book_source_url=?2")
            .bind(user_ns)
            .bind(book_source_url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_all(&self, user_ns: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM book_sources WHERE user_ns=?1")
            .bind(user_ns)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_many(&self, user_ns: &str, keys: &[String]) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        for key in keys {
            sqlx::query("DELETE FROM book_sources WHERE user_ns=?1 AND book_source_url=?2")
                .bind(user_ns).bind(key).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(
        &self,
        user_ns: &str,
        book_source_url: &str,
    ) -> Result<Option<String>, AppError> {
        let row =
            sqlx::query("SELECT json FROM book_sources WHERE user_ns=?1 AND book_source_url=?2")
                .bind(user_ns)
                .bind(book_source_url)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| r.get::<String, _>("json")))
    }

    pub async fn list(&self, user_ns: &str) -> Result<Vec<String>, AppError> {
        let rows =
            sqlx::query("SELECT json FROM book_sources WHERE user_ns=?1 ORDER BY updated_at DESC")
                .bind(user_ns)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.get::<String, _>("json"))
            .collect())
    }

    pub async fn copy_to(&self, from_ns: &str, to_ns: &str) -> Result<i64, AppError> {
        let rows = sqlx::query("SELECT book_source_url, book_source_name, json, updated_at FROM book_sources WHERE user_ns=?1")
            .bind(from_ns)
            .fetch_all(&self.pool)
            .await?;
        let count = rows.len() as i64;
        for row in rows {
            let url: String = row.get("book_source_url");
            let name: String = row.get("book_source_name");
            let json: String = row.get("json");
            let updated_at: i64 = row.get("updated_at");
            sqlx::query(
                "INSERT INTO book_sources (user_ns, book_source_url, book_source_name, json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(user_ns, book_source_url) DO UPDATE SET book_source_name=excluded.book_source_name, json=excluded.json, updated_at=excluded.updated_at"
            )
            .bind(to_ns)
            .bind(&url)
            .bind(&name)
            .bind(&json)
            .bind(updated_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_delete_batch_rolls_back_and_respects_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let pool = super::super::init_pool(temp.path().join("sources.db").to_str().unwrap()).await.unwrap();
        let repo = BookSourceRepo::new(pool.clone());
        let sources = ["accept", "reject"].map(|url| BookSource { book_source_url: url.into(), book_source_name: url.into(), ..Default::default() });
        repo.upsert_many("default", &sources).await.unwrap();
        repo.upsert_many("other", &sources).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_delete BEFORE DELETE ON book_sources WHEN OLD.book_source_url = 'reject' BEGIN SELECT RAISE(ABORT, 'test rejection'); END").execute(&pool).await.unwrap();
        assert!(repo.delete_many("default", &["accept".into(), "reject".into()]).await.is_err());
        assert_eq!(repo.list("default").await.unwrap().len(), 2);
        sqlx::query("DROP TRIGGER reject_delete").execute(&pool).await.unwrap();
        repo.delete_many("default", &["accept".into(), "reject".into()]).await.unwrap();
        assert!(repo.list("default").await.unwrap().is_empty());
        assert_eq!(repo.list("other").await.unwrap().len(), 2);
        pool.close().await;
    }

    #[tokio::test]
    async fn failed_batch_rolls_back_earlier_inserts() {
        let temp = tempfile::tempdir().unwrap();
        let pool = super::super::init_pool(temp.path().join("sources.db").to_str().unwrap())
            .await.unwrap();
        sqlx::query("CREATE TRIGGER reject_fixture BEFORE INSERT ON book_sources WHEN NEW.book_source_url = 'reject' BEGIN SELECT RAISE(ABORT, 'test rejection'); END")
            .execute(&pool).await.unwrap();
        let repo = BookSourceRepo::new(pool.clone());
        let sources = ["accept", "reject"].map(|url| BookSource {
            book_source_url: url.into(), book_source_name: url.into(), ..Default::default()
        });
        assert!(repo.upsert_many("default", &sources).await.is_err());
        assert!(repo.list("default").await.unwrap().is_empty());
        pool.close().await;
    }
}

