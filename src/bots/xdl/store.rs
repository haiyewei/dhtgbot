use anyhow::Result;

use crate::AppContext;

use super::parsing::compare_tweet_ids;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DownloadedTweet {
    pub(super) tweet_id: String,
    pub(super) tweet_url: String,
    pub(super) username: String,
    pub(super) chat_id: i64,
    pub(super) thread_id: Option<i32>,
    pub(super) message_ids: Vec<i32>,
    pub(super) source: String,
    pub(super) downloaded_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TrackedAuthor {
    pub(super) id: String,
    pub(super) username: String,
    pub(super) name: String,
    pub(super) last_tweet_id: Option<String>,
    pub(super) added_at: i64,
    pub(super) added_by: u64,
}

pub(super) async fn load_tracked_authors(context: &AppContext) -> Result<Vec<TrackedAuthor>> {
    let store = context.store.bot("xdl");
    Ok(store
        .kv()
        .get_json::<Vec<TrackedAuthor>>("tracked_authors")
        .await?
        .unwrap_or_default())
}

pub(super) async fn downloaded_tweet(
    context: &AppContext,
    tweet_id: &str,
) -> Result<Option<DownloadedTweet>> {
    context
        .store
        .bot("xdl")
        .kv()
        .get_json::<DownloadedTweet>(&format!("downloaded:{tweet_id}"))
        .await
}

pub(super) async fn save_tracked_authors(
    context: &AppContext,
    authors: &[TrackedAuthor],
) -> Result<()> {
    context
        .store
        .bot("xdl")
        .kv()
        .set_json("tracked_authors", &authors)
        .await
}

pub(super) async fn update_author_last_tweet_id(
    context: &AppContext,
    username: &str,
    tweet_id: &str,
) -> Result<()> {
    let mut authors = load_tracked_authors(context).await?;
    if let Some(author) = authors
        .iter_mut()
        .find(|author| author.username.eq_ignore_ascii_case(username))
    {
        let should_update = author
            .last_tweet_id
            .as_deref()
            .map(|current| compare_tweet_ids(tweet_id, current).is_gt())
            .unwrap_or(true);
        if should_update {
            author.last_tweet_id = Some(tweet_id.to_string());
            save_tracked_authors(context, &authors).await?;
        }
    }
    Ok(())
}
