use anyhow::Result;
use teloxide::prelude::*;
use tokio::sync::oneshot;
use tracing::error;

use crate::AppContext;
use crate::bots::common::{match_group_id, message_text, message_thread_id};
use crate::services::task_queue::TaskLabel;

use super::download::process_tweet_download;
use super::parsing::extract_tweet_ids;

pub(super) fn enqueue_tweet_download(
    bot: Bot,
    context: AppContext,
    tweet_id: String,
    target_group: String,
    target_topic: i32,
    download_dir: Option<String>,
    source: &'static str,
) -> bool {
    let queue = context.work_queue.clone();
    let task_label = TaskLabel::new("xdl", source, tweet_id.clone());
    let task_key = tweet_task_key(&tweet_id, &target_group, target_topic);

    queue.enqueue_unique(task_label, task_key, async move {
        if let Err(error) = process_tweet_download(
            bot,
            context,
            tweet_id.clone(),
            target_group,
            target_topic,
            download_dir,
            source,
        )
        .await
        {
            error!(?error, tweet_id = %tweet_id, source, "tweet download task failed");
        }
    })
}

pub(super) fn enqueue_tweet_download_result(
    bot: Bot,
    context: AppContext,
    tweet_id: String,
    target_group: String,
    target_topic: i32,
    download_dir: Option<String>,
    source: &'static str,
) -> Option<oneshot::Receiver<Result<()>>> {
    let queue = context.work_queue.clone();
    let task_label = TaskLabel::new("xdl", source, tweet_id.clone());
    let task_key = tweet_task_key(&tweet_id, &target_group, target_topic);

    queue.enqueue_unique_result(task_label, task_key, async move {
        process_tweet_download(
            bot,
            context,
            tweet_id,
            target_group,
            target_topic,
            download_dir,
            source,
        )
        .await
    })
}

pub(super) async fn maybe_schedule_tweetdl(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
) -> Result<()> {
    let Some(config) = context.config.bots.xdl.tweetdl.clone() else {
        return Ok(());
    };

    let Some(text) = message_text(message) else {
        return Ok(());
    };
    if text.trim().starts_with('/') || message.chat.is_private() {
        return Ok(());
    }
    if !match_group_id(message.chat.id.0, &config.listen_group) {
        return Ok(());
    }
    if config.listen_topic > 0
        && message_thread_id(message).unwrap_or_default() != config.listen_topic
    {
        return Ok(());
    }

    for tweet_id in extract_tweet_ids(text) {
        let _ = enqueue_tweet_download(
            bot.clone(),
            context.clone(),
            tweet_id,
            config.target_group.clone(),
            config.target_topic,
            config.download_dir.clone(),
            "tweetdl",
        );
    }

    Ok(())
}

fn tweet_task_key(tweet_id: &str, target_group: &str, target_topic: i32) -> String {
    format!("xdl:{target_group}:{target_topic}:{tweet_id}")
}
