use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::warn;

use crate::AppContext;
use crate::bots::common::{escape_html, message_thread_id, send_html_message};

use super::formatting::{format_profile, format_tweet_html};
use super::parsing::extract_tweet_id;

pub(super) async fn handle_profile(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
    args: &str,
) -> Result<()> {
    let username = args.trim().trim_start_matches('@');
    if username.is_empty() {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "用法：<code>/profile 用户名</code>",
        )
        .await?;
        return Ok(());
    }

    let profile = match context.twitter_bridge.user_profile(username).await {
        Ok(profile) => profile,
        Err(error) => {
            warn!(?error, username = %username, "twitter profile request failed");
            send_twitter_service_error(context, bot, message.chat.id, message_thread_id(message))
                .await?;
            return Ok(());
        }
    };
    let Some(profile) = profile else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 未找到该用户。",
        )
        .await?;
        return Ok(());
    };
    send_html_message(
        bot,
        message.chat.id,
        message_thread_id(message),
        format_profile(&profile),
    )
    .await?;
    Ok(())
}

pub(super) async fn handle_tweet(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
    args: &str,
) -> Result<()> {
    let tweet_id = extract_tweet_id(args).unwrap_or_else(|| args.trim().to_string());
    if tweet_id.is_empty() {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "用法：<code>/tweet 推文ID或链接</code>",
        )
        .await?;
        return Ok(());
    }

    let tweet = match context.twitter_bridge.tweet(&tweet_id).await {
        Ok(tweet) => tweet,
        Err(error) => {
            warn!(?error, tweet_id = %tweet_id, "twitter tweet request failed");
            send_twitter_service_error(context, bot, message.chat.id, message_thread_id(message))
                .await?;
            return Ok(());
        }
    };
    let Some(tweet) = tweet else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 未找到该推文。",
        )
        .await?;
        return Ok(());
    };
    send_html_message(
        bot,
        message.chat.id,
        message_thread_id(message),
        format_tweet_html(&tweet),
    )
    .await?;
    Ok(())
}

pub(super) async fn handle_tweets(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
    args: &str,
) -> Result<()> {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    let Some(username) = parts.first().map(|item| item.trim_start_matches('@')) else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "用法：<code>/tweets 用户名 [数量]</code>",
        )
        .await?;
        return Ok(());
    };
    let count = parts
        .get(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5)
        .min(20);

    let page = match context.twitter_bridge.tweets(username, count).await {
        Ok(page) => page,
        Err(error) => {
            warn!(?error, username = %username, "twitter timeline request failed");
            send_twitter_service_error(context, bot, message.chat.id, message_thread_id(message))
                .await?;
            return Ok(());
        }
    };
    if page.list.is_empty() {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "未找到推文。",
        )
        .await?;
        return Ok(());
    }

    for tweet in page.list.into_iter().take(count) {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            format_tweet_html(&tweet),
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn handle_search(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
    args: &str,
) -> Result<()> {
    let query = args.trim();
    if query.is_empty() {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "用法：<code>/search 关键词</code>",
        )
        .await?;
        return Ok(());
    }

    let page = match context.twitter_bridge.search(query, 5).await {
        Ok(page) => page,
        Err(error) => {
            warn!(?error, query = %query, "twitter search request failed");
            send_twitter_service_error(context, bot, message.chat.id, message_thread_id(message))
                .await?;
            return Ok(());
        }
    };
    if page.list.is_empty() {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "未找到相关推文。",
        )
        .await?;
        return Ok(());
    }

    for tweet in page.list.into_iter().take(5) {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            format_tweet_html(&tweet),
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn send_twitter_service_error(
    context: &AppContext,
    bot: &Bot,
    chat_id: ChatId,
    thread_id: Option<i32>,
) -> Result<()> {
    send_html_message(
        bot,
        chat_id,
        thread_id,
        format!(
            "❌ Twitter 抓取服务暂不可用。请确认服务已启动：<code>{}</code>。",
            escape_html(context.twitter_bridge.base_url())
        ),
    )
    .await?;
    Ok(())
}
