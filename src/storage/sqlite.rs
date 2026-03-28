use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use sqlx::{
    Pool, Row, Sqlite,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct KvStore {
    pool: Pool<Sqlite>,
    ensured_tables: Arc<Mutex<HashSet<String>>>,
}

#[derive(Debug, Clone)]
pub struct BotStore {
    store: KvStore,
    table_name: String,
    namespace: &'static str,
}

#[derive(Debug, Clone)]
pub struct KvNamespaceStore {
    bot: BotStore,
}

#[derive(Debug, Clone)]
pub struct ScopedNamespaceStore {
    inner: KvNamespaceStore,
}

#[derive(Debug, Clone)]
pub struct MessageNamespaceStore {
    inner: KvNamespaceStore,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub message_id: i32,
    pub chat_id: i64,
    pub from_id: Option<i64>,
    pub from_username: Option<String>,
    pub is_bot: bool,
    pub date: i64,
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct KeyvEnvelope<T> {
    value: T,
}

impl KvStore {
    pub async fn connect(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create db dir: {}", parent.display()))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("failed to connect sqlite: {}", path.display()))?;

        Ok(Self {
            pool,
            ensured_tables: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub fn bot(&self, bot_name: &str) -> BotStore {
        BotStore {
            store: self.clone(),
            table_name: format!("bot_{bot_name}"),
            namespace: "kv",
        }
    }

    pub async fn ensure_table(&self, table_name: &str) -> Result<()> {
        let mut ensured_tables = self.ensured_tables.lock().await;
        if ensured_tables.contains(table_name) {
            return Ok(());
        }

        ensure_legacy_table(&self.pool, table_name).await?;
        normalize_legacy_rows(&self.pool, table_name).await?;
        ensured_tables.insert(table_name.to_string());
        Ok(())
    }

    pub async fn ensure_tables(&self, table_names: &[&str]) -> Result<()> {
        for table_name in table_names {
            self.ensure_table(table_name).await?;
        }
        Ok(())
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

impl BotStore {
    pub async fn ensure(&self) -> Result<()> {
        self.store.ensure_table(&self.table_name).await
    }

    pub fn kv(&self) -> KvNamespaceStore {
        KvNamespaceStore { bot: self.clone() }
    }

    pub fn chat(&self) -> ScopedNamespaceStore {
        ScopedNamespaceStore {
            inner: self.namespace("chat"),
        }
    }

    pub fn user(&self) -> ScopedNamespaceStore {
        ScopedNamespaceStore {
            inner: self.namespace("user"),
        }
    }

    pub fn message(&self) -> MessageNamespaceStore {
        MessageNamespaceStore {
            inner: self.namespace("message"),
        }
    }

    fn namespace(&self, namespace: &'static str) -> KvNamespaceStore {
        KvNamespaceStore {
            bot: BotStore {
                store: self.store.clone(),
                table_name: self.table_name.clone(),
                namespace,
            },
        }
    }

    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn namespace_prefix(&self) -> &'static str {
        self.namespace
    }
}

impl KvNamespaceStore {
    pub async fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.bot.ensure().await?;
        let storage_key = format!("{}:{key}", self.bot.namespace_prefix());
        let payload = serde_json::to_string(&KeyvEnvelope { value })?;
        let sql = format!(
            r#"INSERT INTO "{table}" (key, value)
               VALUES (?1, ?2)
               ON CONFLICT(key) DO UPDATE SET value = excluded.value"#,
            table = self.bot.table_name()
        );
        sqlx::query(&sql)
            .bind(storage_key)
            .bind(payload)
            .execute(self.bot.store.pool())
            .await?;
        Ok(())
    }

    pub async fn set_json_if_absent<T: Serialize>(&self, key: &str, value: &T) -> Result<bool> {
        self.bot.ensure().await?;
        let storage_key = format!("{}:{key}", self.bot.namespace_prefix());
        let payload = serde_json::to_string(&KeyvEnvelope { value })?;
        let sql = format!(
            r#"INSERT OR IGNORE INTO "{table}" (key, value)
               VALUES (?1, ?2)"#,
            table = self.bot.table_name()
        );
        let result = sqlx::query(&sql)
            .bind(storage_key)
            .bind(payload)
            .execute(self.bot.store.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.bot.ensure().await?;
        let storage_key = format!("{}:{key}", self.bot.namespace_prefix());
        let sql = format!(
            r#"SELECT value FROM "{table}" WHERE key = ?1"#,
            table = self.bot.table_name()
        );
        let row = sqlx::query(&sql)
            .bind(storage_key)
            .fetch_optional(self.bot.store.pool())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let value: String = row.try_get("value")?;
        let envelope: KeyvEnvelope<T> = serde_json::from_str(&value)?;
        Ok(Some(envelope.value))
    }

    pub async fn delete(&self, key: &str) -> Result<bool> {
        self.bot.ensure().await?;
        let storage_key = format!("{}:{key}", self.bot.namespace_prefix());
        let sql = format!(
            r#"DELETE FROM "{table}" WHERE key = ?1"#,
            table = self.bot.table_name()
        );
        let result = sqlx::query(&sql)
            .bind(storage_key)
            .execute(self.bot.store.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn clear(&self) -> Result<()> {
        self.bot.ensure().await?;
        let sql = format!(
            r#"DELETE FROM "{table}" WHERE key LIKE ?1"#,
            table = self.bot.table_name()
        );
        sqlx::query(&sql)
            .bind(format!("{}:%", self.bot.namespace_prefix()))
            .execute(self.bot.store.pool())
            .await?;
        Ok(())
    }
}

impl ScopedNamespaceStore {
    pub async fn set_json<T: Serialize>(&self, scope_id: i64, key: &str, value: &T) -> Result<()> {
        self.inner
            .set_json(&format!("{scope_id}:{key}"), value)
            .await
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        scope_id: i64,
        key: &str,
    ) -> Result<Option<T>> {
        self.inner.get_json(&format!("{scope_id}:{key}")).await
    }

    pub async fn delete(&self, scope_id: i64, key: &str) -> Result<bool> {
        self.inner.delete(&format!("{scope_id}:{key}")).await
    }
}

impl MessageNamespaceStore {
    pub async fn set_json<T: Serialize>(
        &self,
        chat_id: i64,
        message_id: i32,
        value: &T,
    ) -> Result<()> {
        self.inner
            .set_json(&format!("{chat_id}:{message_id}"), value)
            .await
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        chat_id: i64,
        message_id: i32,
    ) -> Result<Option<T>> {
        self.inner
            .get_json(&format!("{chat_id}:{message_id}"))
            .await
    }

    pub async fn delete(&self, chat_id: i64, message_id: i32) -> Result<bool> {
        self.inner.delete(&format!("{chat_id}:{message_id}")).await
    }
}

async fn ensure_legacy_table(pool: &Pool<Sqlite>, table_name: &str) -> Result<()> {
    let current_sql = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?1",
    )
    .bind(table_name)
    .fetch_optional(pool)
    .await?;

    match current_sql {
        None => {
            let sql = format!(
                r#"CREATE TABLE "{table_name}" (key VARCHAR(255) PRIMARY KEY, value TEXT )"#
            );
            sqlx::query(&sql).execute(pool).await?;
        }
        Some(sql) if !is_legacy_table_sql(&sql) => {
            rewrite_table_to_legacy_schema(pool, table_name).await?;
        }
        Some(_) => {}
    }

    Ok(())
}

fn is_legacy_table_sql(sql: &str) -> bool {
    let normalized = sql.to_ascii_lowercase().replace(char::is_whitespace, "");
    normalized.contains("keyvarchar(255)primarykey")
        && normalized.contains("valuetext")
        && !normalized.contains("expires_at")
}

async fn rewrite_table_to_legacy_schema(pool: &Pool<Sqlite>, table_name: &str) -> Result<()> {
    let temp_table = format!("{table_name}__legacy_rewrite_tmp");
    let statements = [
        format!(r#"ALTER TABLE "{table_name}" RENAME TO "{temp_table}""#),
        format!(r#"CREATE TABLE "{table_name}" (key VARCHAR(255) PRIMARY KEY, value TEXT )"#),
        format!(
            r#"INSERT INTO "{table_name}" (key, value)
               SELECT key, value FROM "{temp_table}""#
        ),
        format!(r#"DROP TABLE "{temp_table}""#),
    ];

    for sql in statements {
        sqlx::query(&sql).execute(pool).await?;
    }

    Ok(())
}

async fn normalize_legacy_rows(pool: &Pool<Sqlite>, table_name: &str) -> Result<()> {
    let sql = format!(r#"SELECT key, value FROM "{table_name}""#);
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    for row in rows {
        let key: String = row.try_get("key")?;
        let raw_value: String = row.try_get("value")?;
        if is_keyv_envelope(&raw_value) {
            continue;
        }

        let payload = serde_json::from_str::<Value>(&raw_value)
            .unwrap_or_else(|_| Value::String(raw_value.clone()));
        let legacy_payload = normalize_payload_for_legacy(table_name, &key, payload);
        let wrapped = serde_json::to_string(&json!({ "value": legacy_payload }))?;
        let update_sql = format!(r#"UPDATE "{table_name}" SET value = ?1 WHERE key = ?2"#);
        sqlx::query(&update_sql)
            .bind(wrapped)
            .bind(&key)
            .execute(pool)
            .await?;
    }

    Ok(())
}

fn is_keyv_envelope(raw_value: &str) -> bool {
    serde_json::from_str::<Value>(raw_value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|object| object.contains_key("value"))
        .unwrap_or(false)
}

fn normalize_payload_for_legacy(table_name: &str, key: &str, payload: Value) -> Value {
    if key.starts_with("message:") {
        return normalize_message_payload(payload);
    }

    match (table_name, key) {
        ("bot_xdl", "kv:tracked_authors") => normalize_tracked_authors_payload(payload),
        ("bot_xdl", _) if key.starts_with("kv:downloaded:") => {
            normalize_downloaded_tweet_payload(payload)
        }
        ("bot_tdl", _) if key.starts_with("kv:forwarded:") => {
            normalize_forwarded_message_payload(payload)
        }
        _ => payload,
    }
}

fn normalize_message_payload(payload: Value) -> Value {
    normalize_object_payload(payload, |object| {
        rename_key(object, "message_id", "messageId");
        rename_key(object, "chat_id", "chatId");
        rename_key(object, "from_id", "fromId");
        rename_key(object, "from_username", "fromUsername");
        rename_key(object, "is_bot", "isBot");
        rename_key(object, "kind", "type");
    })
}

fn normalize_tracked_authors_payload(payload: Value) -> Value {
    match payload {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| {
                    normalize_object_payload(item, |object| {
                        rename_key(object, "last_tweet_id", "lastTweetId");
                        rename_key(object, "added_at", "addedAt");
                        rename_key(object, "added_by", "addedBy");
                    })
                })
                .collect(),
        ),
        other => other,
    }
}

fn normalize_downloaded_tweet_payload(payload: Value) -> Value {
    normalize_object_payload(payload, |object| {
        rename_key(object, "tweet_id", "tweetId");
        rename_key(object, "tweet_url", "tweetUrl");
        rename_key(object, "chat_id", "chatId");
        rename_key(object, "thread_id", "threadId");
        rename_key(object, "message_ids", "messageIds");
        rename_key(object, "downloaded_at", "downloadedAt");
    })
}

fn normalize_forwarded_message_payload(payload: Value) -> Value {
    normalize_object_payload(payload, |object| {
        rename_key(object, "source_link", "sourceLink");
        rename_key(object, "link_id", "linkId");
        rename_key(object, "target_chat_id", "targetChatId");
        rename_key(object, "target_thread_id", "targetThreadId");
        rename_key(object, "message_ids", "messageIds");
        rename_key(object, "forwarded_at", "forwardedAt");
    })
}

fn normalize_object_payload(
    payload: Value,
    mut normalize: impl FnMut(&mut Map<String, Value>),
) -> Value {
    match payload {
        Value::Object(mut object) => {
            normalize(&mut object);
            Value::Object(object)
        }
        other => other,
    }
}

fn rename_key(object: &mut Map<String, Value>, from: &str, to: &str) {
    if object.contains_key(to) {
        object.remove(from);
        return;
    }
    if let Some(value) = object.remove(from) {
        object.insert(to.to_string(), value);
    }
}

fn quote_sql_string(value: &str) -> String {
    let escaped = value.replace('\'', "''");
    format!("'{escaped}'")
}

pub fn dump_sqlite_database(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    let sql = tokio::task::block_in_place(move || {
        let conn = rusqlite::Connection::open(path)?;
        let mut out = String::from("BEGIN TRANSACTION;\n");

        let mut stmt = conn.prepare(
            "SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let tables = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for table in tables {
            let (name, create_sql) = table?;
            let _ = writeln!(out, "DROP TABLE IF EXISTS \"{name}\";");
            out.push_str(&create_sql);
            out.push_str(";\n");

            let select_sql = format!("SELECT key, value FROM \"{name}\"");
            let mut rows = conn.prepare(&select_sql)?;
            let mapped = rows.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;

            for row in mapped {
                let (key, value) = row?;
                let _ = writeln!(
                    out,
                    "INSERT INTO \"{name}\" (key, value) VALUES ({}, {});",
                    quote_sql_string(&key),
                    quote_sql_string(&value),
                );
            }
        }

        out.push_str("COMMIT;\n");
        Ok::<String, rusqlite::Error>(out)
    })?;

    Ok(sql)
}

pub async fn import_sqlite_dump(path: &Path, dump_sql: &str) -> Result<()> {
    let path = path.to_path_buf();
    let dump_sql = dump_sql.to_owned();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let conn = rusqlite::Connection::open(path)?;
        drop_existing_tables(&conn)?;
        conn.execute_batch(&dump_sql)?;
        Ok(())
    })
    .await??;
    Ok(())
}

fn drop_existing_tables(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let table_names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    for table_name in table_names {
        let sql = format!("DROP TABLE IF EXISTS \"{table_name}\"");
        conn.execute_batch(&sql)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{KvStore, dump_sqlite_database, import_sqlite_dump};

    #[tokio::test]
    async fn ensure_table_normalizes_current_rust_layout_to_legacy_layout() {
        let path = std::env::temp_dir().join(format!(
            "dhtgbot-normalize-test-{}-{}.sqlite",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "bot_xdl" (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                expires_at INTEGER
            );
            INSERT INTO "bot_xdl" (key, value, expires_at) VALUES (
                'kv:downloaded:1',
                '{"tweet_id":"1","tweet_url":"https://x.com/a/status/1","username":"a","chat_id":1,"thread_id":2,"message_ids":[3],"source":"like_dl","downloaded_at":"2026-03-28T00:00:00Z"}',
                NULL
            );
            "#,
        )
        .unwrap();
        drop(conn);

        let store = KvStore::connect(&path).await.unwrap();
        store.ensure_table("bot_xdl").await.unwrap();

        let conn = Connection::open(&path).unwrap();
        let create_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='bot_xdl'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!create_sql.contains("expires_at"));

        let value: String = conn
            .query_row(
                r#"SELECT value FROM "bot_xdl" WHERE key = 'kv:downloaded:1'"#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(parsed["value"]["tweetId"], serde_json::json!("1"));
        assert_eq!(parsed["value"]["chatId"], serde_json::json!(1));
        assert_eq!(parsed["value"]["messageIds"], serde_json::json!([3]));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn kv_namespace_reads_old_database_payloads() {
        let path = std::env::temp_dir().join(format!(
            "dhtgbot-olddb-test-{}-{}.sqlite",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "bot_xdl" (key VARCHAR(255) PRIMARY KEY, value TEXT );
            INSERT INTO "bot_xdl" (key, value) VALUES (
                'kv:tracked_authors',
                '{"value":[{"id":"1","username":"alice","name":"Alice","lastTweetId":"9","addedAt":1,"addedBy":2}]}'
            );
            "#,
        )
        .unwrap();
        drop(conn);

        #[derive(Debug, serde::Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        struct Author {
            id: String,
            username: String,
            name: String,
            last_tweet_id: Option<String>,
            added_at: i64,
            added_by: u64,
        }

        let store = KvStore::connect(&path).await.unwrap();
        let authors = store
            .bot("xdl")
            .kv()
            .get_json::<Vec<Author>>("tracked_authors")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            authors,
            vec![Author {
                id: String::from("1"),
                username: String::from("alice"),
                name: String::from("Alice"),
                last_tweet_id: Some(String::from("9")),
                added_at: 1,
                added_by: 2,
            }]
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dump_sqlite_database_includes_drop_table_statements() {
        let path = std::env::temp_dir().join(format!(
            "dhtgbot-dump-test-{}-{}.sqlite",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "bot_xdl" (key VARCHAR(255) PRIMARY KEY, value TEXT );
            INSERT INTO "bot_xdl" (key, value) VALUES ('k1', '{"value":"v1"}');
            "#,
        )
        .unwrap();
        drop(conn);

        let dump = dump_sqlite_database(&path).unwrap();
        assert!(dump.contains("DROP TABLE IF EXISTS \"bot_xdl\";"));
        assert!(dump.contains("INSERT INTO \"bot_xdl\" (key, value)"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn import_sqlite_dump_replaces_existing_tables() {
        let path = std::env::temp_dir().join(format!(
            "dhtgbot-import-test-{}-{}.sqlite",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE "bot_xdl" (key VARCHAR(255) PRIMARY KEY, value TEXT );
            INSERT INTO "bot_xdl" (key, value) VALUES ('old', '{"value":"old-value"}');
            "#,
        )
        .unwrap();
        drop(conn);

        let dump_sql = r#"
            BEGIN TRANSACTION;
            CREATE TABLE "bot_xdl" (key VARCHAR(255) PRIMARY KEY, value TEXT );
            INSERT INTO "bot_xdl" (key, value) VALUES ('new', '{"value":"new-value"}');
            COMMIT;
        "#;

        import_sqlite_dump(&path, dump_sql).await.unwrap();

        let conn = Connection::open(&path).unwrap();
        let mut stmt = conn
            .prepare(r#"SELECT key, value FROM "bot_xdl" ORDER BY key"#)
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            vec![(
                String::from("new"),
                String::from(r#"{"value":"new-value"}"#)
            )]
        );
        let _ = std::fs::remove_file(path);
    }
}
