mod amagi;
mod mapper;
mod models;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use tracing::info;

use self::amagi::{
    AmagiTwitterSearchPage, AmagiTwitterTimeline, AmagiTwitterTweet, AmagiTwitterTweetPage,
    AmagiTwitterUserProfile,
};
use self::mapper::{map_paginated_tweets, map_tweet, map_user};
pub use self::models::{PaginatedTweets, Tweet, TweetMedia, TweetMediaType, UserProfile};
use crate::config::{HttpServiceConfig, TwitterConfig};

const TWITTER_COOKIE_HEADER: &str = "X-Amagi-Twitter-Cookie";

#[derive(Debug, Clone)]
pub struct TwitterBridge {
    base_url: String,
    client: Client,
    twitter_cookie: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TwitterApiSpec {
    methods: Vec<TwitterApiMethodSpec>,
}

#[derive(Debug, Deserialize)]
struct TwitterApiMethodSpec {
    method_key: String,
    route: String,
}

impl TwitterBridge {
    pub fn new(service: &HttpServiceConfig, config: Option<&TwitterConfig>) -> Self {
        Self {
            base_url: service.base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(config_timeout_secs(config)))
                .build()
                .expect("twitter http client should build"),
            twitter_cookie: configured_twitter_cookie(config),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        let mut request = self.client.get(url);
        if let Some(cookie) = self.twitter_cookie.as_deref() {
            request = request.header(TWITTER_COOKIE_HEADER, cookie);
        }
        request
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    async fn call_json<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let query_summary = summarize_query(query);
        info!(
            service = "amagi",
            base_url = %self.base_url,
            path,
            query = %query_summary,
            "sending twitter bridge request"
        );
        let started_at = std::time::Instant::now();
        let mut request = self.client.get(&url).query(query);
        if let Some(cookie) = self.twitter_cookie.as_deref() {
            request = request.header(TWITTER_COOKIE_HEADER, cookie);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to request twitter service: {url}"))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("twitter service error {}: {}", status, body);
        }

        let decoded = serde_json::from_str(&body)
            .with_context(|| format!("failed to decode twitter service response: {url}"))?;
        info!(
            service = "amagi",
            base_url = %self.base_url,
            path,
            query = %query_summary,
            http_status = %status,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "twitter bridge request completed"
        );
        Ok(decoded)
    }

    pub async fn user_profile(&self, username: &str) -> Result<Option<UserProfile>> {
        let raw = self
            .call_json::<AmagiTwitterUserProfile>(&format!("/api/twitter/user/{username}"), &[])
            .await?;
        Ok(Some(map_user(raw)))
    }

    pub async fn tweet(&self, tweet_id: &str) -> Result<Option<Tweet>> {
        let raw = self
            .call_json::<AmagiTwitterTweet>(&format!("/api/twitter/tweet/{tweet_id}"), &[])
            .await?;
        Ok(Some(map_tweet(raw)))
    }

    pub async fn tweets(&self, username: &str, count: usize) -> Result<PaginatedTweets> {
        let raw = self
            .call_json::<AmagiTwitterTimeline>(
                &format!("/api/twitter/user/{username}/timeline"),
                &[("count", count.to_string())],
            )
            .await?;
        Ok(map_paginated_tweets(raw.tweets, raw.next_cursor))
    }

    pub async fn likes(&self, count: usize, username: Option<&str>) -> Result<PaginatedTweets> {
        let path = self.user_likes_path(username).await?;
        let raw = self
            .call_json::<AmagiTwitterTweetPage>(&path, &[("count", count.to_string())])
            .await?;
        Ok(map_paginated_tweets(raw.tweets, raw.next_cursor))
    }

    pub async fn tweets_by_id(&self, user_id: &str, count: usize) -> Result<PaginatedTweets> {
        bail!(
            "twitter backend does not expose user timeline by numeric id: {user_id}, count={count}"
        )
    }

    pub async fn search(&self, query: &str, count: usize) -> Result<PaginatedTweets> {
        let raw = self
            .call_json::<AmagiTwitterSearchPage>(
                "/api/twitter/search/tweets",
                &[
                    ("query", query.to_string()),
                    ("search_type", "latest".to_string()),
                    ("count", count.to_string()),
                ],
            )
            .await?;
        Ok(map_paginated_tweets(raw.tweets, raw.next_cursor))
    }

    pub async fn liked_tweets(&self, count: usize) -> Result<PaginatedTweets> {
        bail!("twitter backend does not expose liked tweets page, requested count={count}")
    }

    pub fn is_logged_in(&self) -> bool {
        true
    }

    async fn user_likes_path(&self, username: Option<&str>) -> Result<String> {
        let spec = self
            .call_json::<TwitterApiSpec>("/api/spec/twitter", &[])
            .await;

        if let Ok(spec) = spec {
            if let Some(route) = spec
                .methods
                .into_iter()
                .find(|method| method.method_key == "userLikes")
                .map(|method| method.route)
            {
                return resolve_user_likes_route(&route, username);
            }
        }

        Ok("/api/twitter/user/likes".to_string())
    }
}

fn summarize_query(query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return String::from("-");
    }

    query
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn configured_twitter_cookie(config: Option<&TwitterConfig>) -> Option<String> {
    config.and_then(|value| value.cookies()).map(str::to_owned)
}

fn config_timeout_secs(config: Option<&TwitterConfig>) -> u64 {
    config.and_then(|cfg| cfg.timeout).unwrap_or(15)
}

fn resolve_user_likes_route(route: &str, username: Option<&str>) -> Result<String> {
    let route = route.trim();
    if route.contains("{screen_name}") {
        let username = username
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "twitter likes endpoint requires a username; set bots.xdl.like_dl.username in config.yaml"
                )
            })?;
        return Ok(format!(
            "/api/twitter{}",
            route.replace("{screen_name}", username)
        ));
    }

    if route.starts_with("/api/") {
        return Ok(route.to_string());
    }

    Ok(format!("/api/twitter{route}"))
}

#[cfg(test)]
mod tests {
    use super::{configured_twitter_cookie, resolve_user_likes_route};
    use crate::config::TwitterConfig;

    #[test]
    fn uses_trimmed_twitter_cookie_from_config_for_request_override() {
        let config: TwitterConfig =
            serde_yaml::from_str("cookies: \"  auth_token=abc; ct0=def; twid=u%3D1  \"").unwrap();

        assert_eq!(
            configured_twitter_cookie(Some(&config)),
            Some(String::from("auth_token=abc; ct0=def; twid=u%3D1"))
        );
    }

    #[test]
    fn ignores_blank_twitter_cookie_when_building_request_override() {
        let config: TwitterConfig = serde_yaml::from_str("cookies: \"   \"").unwrap();

        assert_eq!(configured_twitter_cookie(Some(&config)), None);
    }

    #[test]
    fn resolves_authenticated_likes_route() {
        let path = resolve_user_likes_route("/user/likes", None).unwrap();
        assert_eq!(path, "/api/twitter/user/likes");
    }

    #[test]
    fn resolves_screen_name_likes_route() {
        let path = resolve_user_likes_route("/user/{screen_name}/likes", Some("alice")).unwrap();
        assert_eq!(path, "/api/twitter/user/alice/likes");
    }

    #[test]
    fn rejects_screen_name_route_without_username() {
        let error = resolve_user_likes_route("/user/{screen_name}/likes", None).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("bots.xdl.like_dl.username in config.yaml")
        );
    }
}
