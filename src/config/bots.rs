use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct BotsConfig {
    pub master: MasterBotConfig,
    pub tdl: TdlBotConfig,
    pub xdl: XdlBotConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotBaseConfig {
    pub name: String,
    pub token: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MasterBackupConfig {
    pub target_group: String,
    #[serde(default)]
    pub target_topic: i32,
    pub password: String,
    pub import_password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MasterBotConfig {
    #[serde(flatten)]
    pub base: BotBaseConfig,
    #[serde(default)]
    pub admins: Vec<i64>,
    pub backup: Option<MasterBackupConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TdlForwardConfig {
    pub peer: String,
    #[serde(default)]
    pub thread: i32,
    pub listen_chat: String,
    #[serde(default)]
    pub listen_thread: i32,
    pub account: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TdlBotConfig {
    #[serde(flatten)]
    pub base: BotBaseConfig,
    pub forward: Option<TdlForwardConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TwitterConfig {
    #[serde(alias = "apiKey")]
    pub cookies: Option<String>,
    pub timeout: Option<u64>,
}

impl TwitterConfig {
    pub fn cookies(&self) -> Option<&str> {
        self.cookies
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetdlConfig {
    pub listen_group: String,
    #[serde(default)]
    pub listen_topic: i32,
    pub target_group: String,
    #[serde(default)]
    pub target_topic: i32,
    pub download_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LikeDlConfig {
    pub poll_interval: u64,
    pub username: Option<String>,
    pub target_group: String,
    #[serde(default)]
    pub target_topic: i32,
    pub download_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorTrackConfig {
    pub poll_interval: u64,
    pub target_group: String,
    #[serde(default)]
    pub target_topic: i32,
    pub download_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XdlBotConfig {
    #[serde(flatten)]
    pub base: BotBaseConfig,
    pub account: Option<String>,
    pub twitter: Option<TwitterConfig>,
    pub tweetdl: Option<TweetdlConfig>,
    pub like_dl: Option<LikeDlConfig>,
    pub author_track: Option<AuthorTrackConfig>,
}

#[cfg(test)]
mod tests {
    use super::TwitterConfig;

    #[test]
    fn parses_twitter_cookies_field() {
        let config: TwitterConfig =
            serde_yaml::from_str("cookies: \"auth_token=abc; ct0=def; twid=u%3D1\"").unwrap();

        assert_eq!(
            config.cookies(),
            Some("auth_token=abc; ct0=def; twid=u%3D1")
        );
    }

    #[test]
    fn accepts_legacy_api_key_alias_for_cookies() {
        let config: TwitterConfig =
            serde_yaml::from_str("apiKey: \"auth_token=abc; ct0=def; twid=u%3D1\"").unwrap();

        assert_eq!(
            config.cookies(),
            Some("auth_token=abc; ct0=def; twid=u%3D1")
        );
    }
}
