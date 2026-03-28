use super::amagi::{AmagiTwitterMediaEntity, AmagiTwitterTweet, AmagiTwitterUserProfile};
use super::models::{PaginatedTweets, Tweet, TweetMedia, TweetMediaType, UserProfile};
use serde_json::Value;

pub(super) fn map_user(raw: AmagiTwitterUserProfile) -> UserProfile {
    UserProfile {
        id: raw.id,
        username: raw.screen_name,
        name: raw.name,
        description: raw.description.unwrap_or_default(),
        location: raw.location.unwrap_or_default(),
        url: String::new(),
        followers_count: raw.followers_count as i64,
        following_count: raw.following_count as i64,
        tweet_count: raw.statuses_count as i64,
        likes_count: raw.favourites_count as i64,
        verified: raw.verified,
        profile_image_url: raw.avatar_url.unwrap_or_default(),
        profile_banner_url: raw.banner_url.unwrap_or_default(),
        created_at: raw.created_at.unwrap_or_default(),
        pinned_tweet_id: raw.pinned_tweet_id,
    }
}

pub(super) fn map_tweet(raw: AmagiTwitterTweet) -> Tweet {
    let AmagiTwitterTweet {
        id,
        author,
        url,
        created_at,
        full_text,
        language,
        reply_to_tweet_id,
        quoted_tweet,
        retweeted_tweet,
        media,
        reply_count,
        retweet_count,
        quote_count,
        favorite_count,
        bookmark_count,
        view_count,
        upstream_payload,
    } = raw;

    let media = media
        .into_iter()
        .enumerate()
        .filter_map(|(index, media)| map_media(media, upstream_payload.as_ref(), index))
        .collect::<Vec<_>>();

    Tweet {
        id,
        text: full_text,
        created_at: created_at.unwrap_or_default(),
        url,
        lang: language.unwrap_or_default(),
        user_id: author.id,
        username: author.screen_name,
        name: author.name,
        likes: favorite_count as i64,
        retweets: retweet_count as i64,
        replies: reply_count as i64,
        views: view_count.unwrap_or_default() as i64,
        quotes: quote_count as i64,
        bookmarks: bookmark_count.unwrap_or_default() as i64,
        is_retweet: retweeted_tweet.is_some(),
        is_quote: quoted_tweet.is_some(),
        is_reply: reply_to_tweet_id.is_some(),
        hashtags: Vec::new(),
        mentions: Vec::new(),
        urls: Vec::new(),
        media,
        quoted_tweet: quoted_tweet.map(|tweet| Box::new(map_tweet(*tweet))),
        retweeted_tweet: retweeted_tweet.map(|tweet| Box::new(map_tweet(*tweet))),
        reply_to_id: reply_to_tweet_id,
    }
}

pub(super) fn map_paginated_tweets(
    tweets: Vec<AmagiTwitterTweet>,
    next_cursor: Option<String>,
) -> PaginatedTweets {
    let has_more = next_cursor.is_some();
    PaginatedTweets {
        list: tweets.into_iter().map(map_tweet).collect(),
        next_cursor,
        has_more,
    }
}

fn map_media(
    raw: AmagiTwitterMediaEntity,
    upstream_payload: Option<&Value>,
    index: usize,
) -> Option<TweetMedia> {
    let AmagiTwitterMediaEntity {
        media_type,
        media_url,
        preview_image_url,
        expanded_url,
    } = raw;

    let media_type = match media_type.as_str() {
        "photo" => TweetMediaType::Photo,
        "video" => TweetMediaType::Video,
        "animated_gif" | "gif" => TweetMediaType::Gif,
        _ => return None,
    };

    let url = match media_type {
        TweetMediaType::Photo => media_url
            .clone()
            .or(expanded_url.clone())
            .or(preview_image_url.clone())?,
        TweetMediaType::Video | TweetMediaType::Gif => {
            select_video_variant_url(upstream_payload, index, expanded_url.as_deref())
                .or(media_url.clone())
                .or(expanded_url.clone())
                .or(preview_image_url.clone())?
        }
    };

    Some(TweetMedia {
        r#type: media_type,
        url,
        thumbnail_url: preview_image_url,
    })
}

fn select_video_variant_url(
    upstream_payload: Option<&Value>,
    index: usize,
    expanded_url: Option<&str>,
) -> Option<String> {
    let media_items = upstream_media_items(upstream_payload?)?;
    let media = media_items.get(index).or_else(|| {
        expanded_url.and_then(|url| {
            media_items.iter().find(|item| {
                item.get("expanded_url")
                    .and_then(Value::as_str)
                    .map(|value| value == url)
                    .unwrap_or(false)
            })
        })
    })?;
    let variants = media
        .get("video_info")
        .and_then(|value| value.get("variants"))
        .and_then(Value::as_array)?;

    variants
        .iter()
        .filter_map(|variant| {
            let content_type = variant.get("content_type").and_then(Value::as_str)?;
            if content_type != "video/mp4" {
                return None;
            }
            let url = variant.get("url").and_then(Value::as_str)?;
            let bitrate = variant.get("bitrate").and_then(Value::as_u64).unwrap_or(0);
            Some((bitrate, url))
        })
        .max_by_key(|(bitrate, _)| *bitrate)
        .map(|(_, url)| url.to_string())
        .or_else(|| {
            variants.iter().find_map(|variant| {
                variant
                    .get("url")
                    .and_then(Value::as_str)
                    .map(|url| url.to_string())
            })
        })
}

fn upstream_media_items(upstream_payload: &Value) -> Option<&Vec<Value>> {
    upstream_payload
        .get("legacy")
        .and_then(|value| value.get("extended_entities"))
        .and_then(|value| value.get("media"))
        .and_then(Value::as_array)
        .or_else(|| {
            upstream_payload
                .get("legacy")
                .and_then(|value| value.get("entities"))
                .and_then(|value| value.get("media"))
                .and_then(Value::as_array)
        })
}

#[cfg(test)]
mod tests {
    use super::map_tweet;
    use crate::services::twitter_bridge::TweetMediaType;

    #[test]
    fn maps_video_to_highest_bitrate_mp4() {
        let raw: crate::services::twitter_bridge::amagi::AmagiTwitterTweet = serde_json::from_value(
            serde_json::json!({
                "id": "1",
                "author": { "id": "u1", "screen_name": "alice", "name": "Alice" },
                "url": "https://x.com/alice/status/1",
                "created_at": "2025-01-01T00:00:00+00:00",
                "full_text": "video",
                "language": "en",
                "reply_to_tweet_id": null,
                "quoted_tweet": null,
                "retweeted_tweet": null,
                "media": [{
                    "media_type": "video",
                    "media_url": "https://pbs.twimg.com/thumb.jpg",
                    "preview_image_url": "https://pbs.twimg.com/thumb.jpg",
                    "expanded_url": "https://x.com/alice/status/1/video/1"
                }],
                "reply_count": 0,
                "retweet_count": 0,
                "quote_count": 0,
                "favorite_count": 0,
                "bookmark_count": 0,
                "view_count": 0,
                "upstream_payload": {
                    "legacy": {
                        "extended_entities": {
                            "media": [{
                                "expanded_url": "https://x.com/alice/status/1/video/1",
                                "video_info": {
                                    "variants": [
                                        { "content_type": "application/x-mpegURL", "url": "https://video.twimg.com/playlist.m3u8" },
                                        { "content_type": "video/mp4", "bitrate": 320000, "url": "https://video.twimg.com/low.mp4" },
                                        { "content_type": "video/mp4", "bitrate": 950000, "url": "https://video.twimg.com/high.mp4" }
                                    ]
                                }
                            }]
                        }
                    }
                }
            }),
        )
        .unwrap();

        let tweet = map_tweet(raw);
        assert_eq!(tweet.media.len(), 1);
        assert!(matches!(tweet.media[0].r#type, TweetMediaType::Video));
        assert_eq!(tweet.media[0].url, "https://video.twimg.com/high.mp4");
        assert_eq!(
            tweet.media[0].thumbnail_url.as_deref(),
            Some("https://pbs.twimg.com/thumb.jpg")
        );
    }

    #[test]
    fn maps_multiple_videos_in_one_tweet() {
        let raw: crate::services::twitter_bridge::amagi::AmagiTwitterTweet = serde_json::from_value(
            serde_json::json!({
                "id": "2",
                "author": { "id": "u2", "screen_name": "bob", "name": "Bob" },
                "url": "https://x.com/bob/status/2",
                "created_at": "2025-01-01T00:00:00+00:00",
                "full_text": "two videos",
                "language": "en",
                "reply_to_tweet_id": null,
                "quoted_tweet": null,
                "retweeted_tweet": null,
                "media": [
                    {
                        "media_type": "video",
                        "media_url": "https://pbs.twimg.com/thumb-1.jpg",
                        "preview_image_url": "https://pbs.twimg.com/thumb-1.jpg",
                        "expanded_url": "https://x.com/bob/status/2/video/1"
                    },
                    {
                        "media_type": "video",
                        "media_url": "https://pbs.twimg.com/thumb-2.jpg",
                        "preview_image_url": "https://pbs.twimg.com/thumb-2.jpg",
                        "expanded_url": "https://x.com/bob/status/2/video/2"
                    }
                ],
                "reply_count": 0,
                "retweet_count": 0,
                "quote_count": 0,
                "favorite_count": 0,
                "bookmark_count": 0,
                "view_count": 0,
                "upstream_payload": {
                    "legacy": {
                        "extended_entities": {
                            "media": [
                                {
                                    "expanded_url": "https://x.com/bob/status/2/video/1",
                                    "video_info": {
                                        "variants": [
                                            { "content_type": "video/mp4", "bitrate": 640000, "url": "https://video.twimg.com/one.mp4" }
                                        ]
                                    }
                                },
                                {
                                    "expanded_url": "https://x.com/bob/status/2/video/2",
                                    "video_info": {
                                        "variants": [
                                            { "content_type": "video/mp4", "bitrate": 832000, "url": "https://video.twimg.com/two.mp4" }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }),
        )
        .unwrap();

        let tweet = map_tweet(raw);
        assert_eq!(tweet.media.len(), 2);
        assert_eq!(tweet.media[0].url, "https://video.twimg.com/one.mp4");
        assert_eq!(tweet.media[1].url, "https://video.twimg.com/two.mp4");
    }
}
