mod bots;
mod database;
mod services;

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub use self::bots::{
    AuthorTrackConfig, BotsConfig, LikeDlConfig, TdlForwardConfig, TwitterConfig,
};
pub use self::database::DatabaseConfig;
pub use self::services::{Aria2ServiceConfig, HttpServiceConfig, ServicesConfig};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub bots: BotsConfig,
    pub services: ServicesConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&text)
            .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
        if config.database.db_type != "sqlite" {
            bail!(
                "Rust migration currently supports sqlite only, found {}",
                config.database.db_type
            );
        }
        Ok(config)
    }

    pub fn sqlite_path(&self, root: &Path) -> PathBuf {
        let configured = Path::new(&self.database.path);
        let path = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            root.join(configured)
        };
        normalize_path(path)
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, normalize_path};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn normalize_path_removes_cur_dir_segments() {
        let path = PathBuf::from("workspace")
            .join("project")
            .join(".")
            .join("data")
            .join("bot.sqlite");
        let normalized = normalize_path(path);
        assert_eq!(
            normalized,
            PathBuf::from("workspace")
                .join("project")
                .join("data")
                .join("bot.sqlite")
        );
    }

    #[test]
    fn sqlite_path_resolves_relative_database_path() {
        let config = AppConfig {
            bots: serde_yaml::from_str(
                r#"
master: { name: master, token: t1, enabled: false, admins: [], backup: ~ }
tdl: { name: tdl, token: t2, enabled: false, forward: ~ }
xdl: { name: xdl, token: t3, enabled: false, account: ~, twitter: ~, tweetdl: ~, like_dl: ~, author_track: ~ }
"#,
            )
            .unwrap(),
            services: serde_yaml::from_str(
                r#"
amagi: { base_url: http://127.0.0.1:4567, start_command: "", startup_timeout_ms: 1000 }
tdlr: { base_url: http://127.0.0.1:8787, start_command: "", startup_timeout_ms: 1000 }
aria2: { rpc_url: http://127.0.0.1:6800/jsonrpc, secret: ~, start_command: "", startup_timeout_ms: 1000 }
"#,
            )
            .unwrap(),
            database: serde_yaml::from_str(
                r#"
type: sqlite
path: ./data/bot.sqlite
"#,
            )
            .unwrap(),
        };

        assert_eq!(
            config.sqlite_path(&PathBuf::from("workspace").join("project")),
            PathBuf::from("workspace")
                .join("project")
                .join("data")
                .join("bot.sqlite")
        );
    }

    #[test]
    fn config_example_yaml_parses() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.example.yaml");
        let text = fs::read_to_string(path).unwrap();
        let config: AppConfig = serde_yaml::from_str(&text).unwrap();

        assert_eq!(config.database.db_type, "sqlite");
        assert_eq!(config.database.path, "./data/bot.sqlite");
    }

    #[test]
    fn docker_config_example_yaml_parses() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.example.docker.yaml");
        let text = fs::read_to_string(path).unwrap();
        let config: AppConfig = serde_yaml::from_str(&text).unwrap();

        assert_eq!(config.database.db_type, "sqlite");
        assert_eq!(config.database.path, "./data/bot.sqlite");
        assert_eq!(
            config.services.amagi.start_command.as_deref(),
            Some("amagi serve --host 0.0.0.0 --port 4567")
        );
    }
}
