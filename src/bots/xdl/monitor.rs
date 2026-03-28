use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::Duration;

use anyhow::Result;
use teloxide::prelude::*;
use tracing::{error, info, warn};

use crate::AppContext;
use crate::config::{AuthorTrackConfig, LikeDlConfig};

use super::parsing::{compare_tweet_ids, tweet_id_gt};
use super::schedule::{enqueue_tweet_download, enqueue_tweet_download_result};
use super::store::{downloaded_tweet, load_tracked_authors, update_author_last_tweet_id};

pub(super) async fn like_monitor_loop(bot: Bot, context: AppContext, running: Arc<AtomicBool>) {
    let Some(config) = context.config.bots.xdl.like_dl.clone() else {
        return;
    };
    let poll_interval = Duration::from_millis(config.poll_interval.max(1_000));

    while running.load(AtomicOrdering::SeqCst) {
        if let Err(error) = process_liked_tweets_once(&bot, &context, &config).await {
            error!(?error, "like monitor task failed");
        }

        if !running.load(AtomicOrdering::SeqCst) {
            break;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub(super) async fn author_monitor_loop(bot: Bot, context: AppContext, running: Arc<AtomicBool>) {
    let Some(config) = context.config.bots.xdl.author_track.clone() else {
        return;
    };
    let poll_interval = Duration::from_millis(config.poll_interval.max(1_000));

    while running.load(AtomicOrdering::SeqCst) {
        match load_tracked_authors(&context).await {
            Ok(authors) if authors.is_empty() => {}
            Ok(authors) => {
                for author in authors {
                    if !running.load(AtomicOrdering::SeqCst) {
                        break;
                    }

                    if let Err(error) = process_author_once(&bot, &context, &config, &author).await
                    {
                        error!(?error, username = %author.username, "author monitor task failed");
                    }

                    if running.load(AtomicOrdering::SeqCst) {
                        tokio::time::sleep(next_author_request_delay()).await;
                    }
                }
            }
            Err(error) => {
                error!(?error, "failed to load tracked authors");
            }
        }

        if !running.load(AtomicOrdering::SeqCst) {
            break;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

async fn process_liked_tweets_once(
    bot: &Bot,
    context: &AppContext,
    config: &LikeDlConfig,
) -> Result<()> {
    let page = match context
        .twitter_bridge
        .likes(100, config.username.as_deref())
        .await
    {
        Ok(page) => page,
        Err(error) => {
            warn!(?error, "twitter likes request failed");
            return Ok(());
        }
    };

    let mut tweets = Vec::new();
    for tweet in page.list {
        if downloaded_tweet(context, &tweet.id).await?.is_some() {
            continue;
        }
        tweets.push(tweet);
    }

    if tweets.is_empty() {
        return Ok(());
    }
    let tweets = order_liked_tweets_for_enqueue(tweets);

    let mut enqueued = 0usize;
    let mut skipped = 0usize;
    for tweet in tweets {
        if enqueue_tweet_download(
            bot.clone(),
            context.clone(),
            tweet.id.clone(),
            config.target_group.clone(),
            config.target_topic,
            config.download_dir.clone(),
            "like_dl",
        ) {
            enqueued += 1;
        } else {
            skipped += 1;
        }
    }
    info!(enqueued, skipped, "like monitor batch queued");

    Ok(())
}

async fn process_author_once(
    bot: &Bot,
    context: &AppContext,
    config: &AuthorTrackConfig,
    author: &super::store::TrackedAuthor,
) -> Result<()> {
    let page = match fetch_author_timeline(context, author).await {
        Ok(page) => page,
        Err(error) => {
            warn!(?error, username = %author.username, "author timeline request failed");
            return Ok(());
        }
    };

    let new_tweets = page
        .list
        .into_iter()
        .filter(|tweet| !tweet.is_retweet)
        .filter(|tweet| {
            author
                .last_tweet_id
                .as_deref()
                .map(|last_id| tweet_id_gt(&tweet.id, last_id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    let new_tweets = order_author_tweets_for_enqueue(new_tweets);

    if new_tweets.is_empty() {
        return Ok(());
    }

    let mut receivers = Vec::new();
    let mut enqueued = 0usize;
    let mut skipped = 0usize;
    for tweet in new_tweets {
        let tweet_id = tweet.id.clone();
        if let Some(receiver) = enqueue_tweet_download_result(
            bot.clone(),
            context.clone(),
            tweet_id.clone(),
            config.target_group.clone(),
            config.target_topic,
            config.download_dir.clone(),
            "author_track",
        ) {
            enqueued += 1;
            receivers.push((tweet_id, receiver));
        } else {
            skipped += 1;
        }
    }
    info!(username = %author.username, enqueued, skipped, "author monitor batch queued");

    if !receivers.is_empty() {
        let context = context.clone();
        let username = author.username.clone();
        tokio::spawn(async move {
            for (tweet_id, receiver) in receivers {
                match receiver.await {
                    Ok(Ok(())) => {
                        if let Err(error) =
                            update_author_last_tweet_id(&context, &username, &tweet_id).await
                        {
                            error!(
                                ?error,
                                username = %username,
                                tweet_id = %tweet_id,
                                "failed to update author last tweet id after queued download"
                            );
                        }
                    }
                    Ok(Err(error)) => {
                        error!(
                            ?error,
                            username = %username,
                            tweet_id = %tweet_id,
                            "author tweet download failed"
                        );
                    }
                    Err(error) => {
                        error!(
                            ?error,
                            username = %username,
                            tweet_id = %tweet_id,
                            "author queued task receiver dropped"
                        );
                    }
                }
            }
        });
    }

    Ok(())
}

async fn fetch_author_timeline(
    context: &AppContext,
    author: &super::store::TrackedAuthor,
) -> Result<crate::services::twitter_bridge::PaginatedTweets> {
    if !author.id.trim().is_empty() {
        if let Ok(page) = context.twitter_bridge.tweets_by_id(&author.id, 20).await {
            return Ok(page);
        }
    }

    context.twitter_bridge.tweets(&author.username, 20).await
}

fn next_author_request_delay() -> Duration {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis();
    author_request_delay_from_seed(seed)
}

fn author_request_delay_from_seed(seed: u32) -> Duration {
    Duration::from_millis(17_000 + u64::from(seed % 4_000))
}

fn order_liked_tweets_for_enqueue(
    mut tweets: Vec<crate::services::twitter_bridge::Tweet>,
) -> Vec<crate::services::twitter_bridge::Tweet> {
    // Likes endpoints return newest liked items first; reverse to enqueue the oldest pending like first.
    tweets.reverse();
    tweets
}

fn order_author_tweets_for_enqueue(
    mut tweets: Vec<crate::services::twitter_bridge::Tweet>,
) -> Vec<crate::services::twitter_bridge::Tweet> {
    tweets.sort_by(|left, right| compare_tweet_ids(&left.id, &right.id));
    tweets
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        author_request_delay_from_seed, order_author_tweets_for_enqueue,
        order_liked_tweets_for_enqueue,
    };
    use crate::services::twitter_bridge::Tweet;

    fn sample_tweet(id: &str) -> Tweet {
        Tweet {
            id: id.to_string(),
            text: String::new(),
            created_at: String::new(),
            url: String::new(),
            lang: String::new(),
            user_id: String::new(),
            username: String::from("alice"),
            name: String::from("Alice"),
            likes: 0,
            retweets: 0,
            replies: 0,
            views: 0,
            quotes: 0,
            bookmarks: 0,
            is_retweet: false,
            is_quote: false,
            is_reply: false,
            hashtags: Vec::new(),
            mentions: Vec::new(),
            urls: Vec::new(),
            media: Vec::new(),
            quoted_tweet: None,
            retweeted_tweet: None,
            reply_to_id: None,
        }
    }

    #[test]
    fn liked_tweets_keep_api_order_but_oldest_first() {
        let ordered = order_liked_tweets_for_enqueue(vec![
            sample_tweet("300"),
            sample_tweet("10"),
            sample_tweet("200"),
        ]);

        assert_eq!(
            ordered
                .into_iter()
                .map(|tweet| tweet.id)
                .collect::<Vec<_>>(),
            vec![String::from("200"), String::from("10"), String::from("300"),]
        );
    }

    #[test]
    fn author_tweets_are_sorted_from_oldest_to_newest() {
        let ordered = order_author_tweets_for_enqueue(vec![
            sample_tweet("300"),
            sample_tweet("10"),
            sample_tweet("200"),
        ]);

        assert_eq!(
            ordered
                .into_iter()
                .map(|tweet| tweet.id)
                .collect::<Vec<_>>(),
            vec![String::from("10"), String::from("200"), String::from("300"),]
        );
    }

    #[test]
    fn author_request_delay_matches_legacy_backoff_window() {
        assert_eq!(author_request_delay_from_seed(0), Duration::from_secs(17));
        assert_eq!(
            author_request_delay_from_seed(3_999),
            Duration::from_millis(20_999)
        );
    }
}
