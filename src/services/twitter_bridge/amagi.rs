use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterUserSummary {
    pub(super) id: String,
    pub(super) screen_name: String,
    pub(super) name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterUserProfile {
    pub(super) id: String,
    pub(super) screen_name: String,
    pub(super) name: String,
    pub(super) created_at: Option<String>,
    pub(super) description: Option<String>,
    pub(super) location: Option<String>,
    pub(super) avatar_url: Option<String>,
    pub(super) banner_url: Option<String>,
    pub(super) verified: bool,
    pub(super) followers_count: u64,
    pub(super) following_count: u64,
    pub(super) statuses_count: u64,
    pub(super) favourites_count: u64,
    pub(super) pinned_tweet_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterMediaEntity {
    pub(super) media_type: String,
    pub(super) media_url: Option<String>,
    pub(super) preview_image_url: Option<String>,
    pub(super) expanded_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterTweet {
    pub(super) id: String,
    pub(super) author: AmagiTwitterUserSummary,
    pub(super) url: String,
    pub(super) created_at: Option<String>,
    pub(super) full_text: String,
    pub(super) language: Option<String>,
    pub(super) reply_to_tweet_id: Option<String>,
    pub(super) quoted_tweet: Option<Box<AmagiTwitterTweet>>,
    pub(super) retweeted_tweet: Option<Box<AmagiTwitterTweet>>,
    pub(super) media: Vec<AmagiTwitterMediaEntity>,
    pub(super) reply_count: u64,
    pub(super) retweet_count: u64,
    pub(super) quote_count: u64,
    pub(super) favorite_count: u64,
    pub(super) bookmark_count: Option<u64>,
    pub(super) view_count: Option<u64>,
    #[serde(default)]
    pub(super) upstream_payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterTimeline {
    pub(super) tweets: Vec<AmagiTwitterTweet>,
    #[allow(dead_code)]
    pub(super) previous_cursor: Option<String>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterTweetPage {
    pub(super) tweets: Vec<AmagiTwitterTweet>,
    #[allow(dead_code)]
    pub(super) previous_cursor: Option<String>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AmagiTwitterSearchPage {
    pub(super) tweets: Vec<AmagiTwitterTweet>,
    pub(super) next_cursor: Option<String>,
}
