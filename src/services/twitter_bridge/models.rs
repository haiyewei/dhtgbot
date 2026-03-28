use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TweetMediaType {
    Photo,
    Video,
    Gif,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TweetMedia {
    pub r#type: TweetMediaType,
    pub url: String,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub url: String,
    pub lang: String,
    pub user_id: String,
    pub username: String,
    pub name: String,
    pub likes: i64,
    pub retweets: i64,
    pub replies: i64,
    pub views: i64,
    pub quotes: i64,
    pub bookmarks: i64,
    pub is_retweet: bool,
    pub is_quote: bool,
    pub is_reply: bool,
    #[serde(default)]
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub media: Vec<TweetMedia>,
    #[serde(default)]
    pub quoted_tweet: Option<Box<Tweet>>,
    #[serde(default)]
    pub retweeted_tweet: Option<Box<Tweet>>,
    #[serde(default)]
    pub reply_to_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub name: String,
    pub description: String,
    pub location: String,
    pub url: String,
    pub followers_count: i64,
    pub following_count: i64,
    pub tweet_count: i64,
    pub likes_count: i64,
    pub verified: bool,
    pub profile_image_url: String,
    pub profile_banner_url: String,
    pub created_at: String,
    #[serde(default)]
    pub pinned_tweet_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedTweets {
    pub list: Vec<Tweet>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
