use anyhow::Result;

use crate::config::AppConfig;

use super::KvStore;

pub async fn bootstrap_store(store: &KvStore, config: &AppConfig) -> Result<()> {
    let mut table_names = Vec::new();

    if config.bots.master.base.enabled {
        table_names.push("bot_master");
    }
    if config.bots.tdl.base.enabled {
        table_names.push("bot_tdl");
    }
    if config.bots.xdl.base.enabled {
        table_names.push("bot_xdl");
    }

    store.ensure_tables(&table_names).await?;

    if config.bots.xdl.base.enabled {
        let bot_store = store.bot("xdl");
        bot_store
            .kv()
            .set_json_if_absent("tracked_authors", &Vec::<serde_json::Value>::new())
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::bootstrap_store;
    use crate::config::AppConfig;
    use crate::storage::KvStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> AppConfig {
        serde_yaml::from_str(
            r#"
bots:
  master:
    name: master
    token: t1
    enabled: true
    admins: []
    backup:
  tdl:
    name: tdl
    token: t2
    enabled: true
    forward:
  xdl:
    name: xdl
    token: t3
    enabled: true
    account:
    twitter:
    tweetdl:
    like_dl:
    author_track:
services:
  amagi:
    base_url: http://127.0.0.1:4567
    start_command: ""
    startup_timeout_ms: 1000
  tdlr:
    base_url: http://127.0.0.1:8787
    start_command: ""
    startup_timeout_ms: 1000
  aria2:
    rpc_url: http://127.0.0.1:6800/jsonrpc
    secret:
    start_command: ""
    startup_timeout_ms: 1000
database:
  type: sqlite
  path: ./data/bot.sqlite
"#,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn bootstrap_store_creates_tables_and_seeds_tracked_authors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("dhtgbot-bootstrap-{unique}.sqlite"));
        let store = KvStore::connect(&db_path).await.unwrap();

        bootstrap_store(&store, &test_config()).await.unwrap();

        let rows = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('bot_master', 'bot_tdl', 'bot_xdl')",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(rows, 3);

        let authors = store
            .bot("xdl")
            .kv()
            .get_json::<Vec<serde_json::Value>>("tracked_authors")
            .await
            .unwrap();
        assert_eq!(authors, Some(Vec::new()));

        let _ = tokio::fs::remove_file(db_path).await;
    }
}
