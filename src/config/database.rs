use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    #[serde(rename = "type")]
    pub db_type: String,
    pub path: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: "sqlite".to_string(),
            path: "./data/bot.sqlite".to_string(),
            host: None,
            port: None,
            user: None,
            password: None,
            database: None,
        }
    }
}
