use anyhow::Result;
use teloxide::prelude::*;
use tracing::warn;

use crate::AppContext;
use crate::bots::common::{escape_html, message_thread_id, send_html_message};

use super::parsing::{compare_tweet_ids, parse_username_input};
use super::query::send_twitter_service_error;
use super::runtime::XdlRuntime;
use super::store::{TrackedAuthor, load_tracked_authors, save_tracked_authors};

fn x_user_link(username: &str) -> String {
    format!(
        "<a href=\"https://x.com/{0}\">{0}</a>",
        escape_html(username)
    )
}

pub(super) async fn handle_author_track(
    bot: &Bot,
    context: &AppContext,
    runtime: &XdlRuntime,
    message: &Message,
    args: &str,
) -> Result<()> {
    let Some(config) = context.config.bots.xdl.author_track.as_ref() else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 未配置作者追踪，请在 <code>config.yaml</code> 中配置 <code>bots.xdl.author_track</code>。",
        )
        .await?;
        return Ok(());
    };

    let parts = args.split_whitespace().collect::<Vec<_>>();
    let action = parts.first().map(|value| value.to_ascii_lowercase());

    match action.as_deref() {
        Some("add") => {
            let Some(username) = parse_username_input(parts.get(1).copied()) else {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "用法：<code>/author_track add 用户名或链接</code>",
                )
                .await?;
                return Ok(());
            };

            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!("⏳ 正在添加 <code>{}</code>...", escape_html(&username)),
            )
            .await?;

            let profile = match context.twitter_bridge.user_profile(&username).await {
                Ok(profile) => profile,
                Err(error) => {
                    warn!(?error, username = %username, "author profile request failed");
                    send_twitter_service_error(
                        context,
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                    )
                    .await?;
                    return Ok(());
                }
            };
            let Some(profile) = profile else {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 未找到该作者。",
                )
                .await?;
                return Ok(());
            };

            let mut authors = load_tracked_authors(context).await?;
            if authors.iter().any(|author| {
                author.id == profile.id || author.username.eq_ignore_ascii_case(&profile.username)
            }) {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    format!("⚠️ {} 已在追踪列表中。", x_user_link(&profile.username)),
                )
                .await?;
                return Ok(());
            }

            let latest_tweet_id = match context.twitter_bridge.tweets(&profile.username, 10).await {
                Ok(page) => page
                    .list
                    .into_iter()
                    .filter(|tweet| !tweet.is_retweet)
                    .map(|tweet| tweet.id)
                    .max_by(|left, right| compare_tweet_ids(left, right)),
                Err(error) => {
                    warn!(?error, username = %profile.username, "author timeline request failed");
                    None
                }
            };

            authors.push(TrackedAuthor {
                id: profile.id.clone(),
                username: profile.username.clone(),
                name: profile.name.clone(),
                last_tweet_id: latest_tweet_id,
                added_at: chrono::Local::now().timestamp_millis(),
                added_by: message
                    .from
                    .as_ref()
                    .map(|user| user.id.0)
                    .unwrap_or_default(),
            });
            save_tracked_authors(context, &authors).await?;

            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!(
                    "✅ 已添加追踪作者\n\n👤 {}\n🔗 {}\n🆔 <code>{}</code>",
                    escape_html(&profile.name),
                    x_user_link(&profile.username),
                    escape_html(&profile.id),
                ),
            )
            .await?;
        }
        Some("remove") | Some("del") => {
            let Some(input) = parts.get(1).copied() else {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "用法：<code>/author_track remove 用户名|编号</code>",
                )
                .await?;
                return Ok(());
            };

            let mut authors = load_tracked_authors(context).await?;
            let removed = if let Ok(index) = input.parse::<usize>() {
                if index == 0 || index > authors.len() {
                    None
                } else {
                    Some(authors.remove(index - 1))
                }
            } else if let Some(username) = parse_username_input(Some(input)) {
                authors
                    .iter()
                    .position(|author| author.username.eq_ignore_ascii_case(&username))
                    .map(|index| authors.remove(index))
            } else {
                None
            };

            let Some(removed) = removed else {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 未找到对应的作者。",
                )
                .await?;
                return Ok(());
            };

            save_tracked_authors(context, &authors).await?;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!("✅ 已移除追踪作者 {}。", x_user_link(&removed.username)),
            )
            .await?;
        }
        Some("start") => {
            runtime
                .author_monitor
                .start_author(bot.clone(), context.clone())
                .await;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!(
                    "▶️ 作者追踪监控已启动\n\n轮询间隔：{} 秒\n目标群组：<code>{}</code>\n目标话题：<code>{}</code>",
                    config.poll_interval / 1000,
                    escape_html(&config.target_group),
                    config.target_topic,
                ),
            )
            .await?;
        }
        Some("stop") => {
            runtime.author_monitor.stop().await;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "⏹️ 作者追踪监控已停止。",
            )
            .await?;
        }
        _ => {
            let authors = load_tracked_authors(context).await?;
            let mut text = format!(
                "📡 <b>作者追踪状态</b>\n\n状态：{}\n轮询间隔：{} 秒\n目标群组：<code>{}</code>\n目标话题：<code>{}</code>\n\n",
                if runtime.author_monitor.is_running() {
                    "▶️ 运行中"
                } else {
                    "⏹️ 已停止"
                },
                config.poll_interval / 1000,
                escape_html(&config.target_group),
                config.target_topic,
            );

            if authors.is_empty() {
                text.push_str("📋 当前追踪列表为空。\n\n");
            } else {
                text.push_str(&format!("📋 追踪列表（{}）\n", authors.len()));
                for (index, author) in authors.iter().enumerate() {
                    text.push_str(&format!(
                        "{}. {} ({})\n",
                        index + 1,
                        x_user_link(&author.username),
                        escape_html(&author.name),
                    ));
                }
                text.push('\n');
            }

            text.push_str(
                "命令：\n\
                 <code>/author_track add 用户名|链接</code>\n\
                 <code>/author_track remove 用户名|编号</code>\n\
                 <code>/author_track start</code>\n\
                 <code>/author_track stop</code>",
            );

            send_html_message(bot, message.chat.id, message_thread_id(message), text).await?;
        }
    }

    Ok(())
}

pub(super) async fn handle_tweet_like_dl(
    bot: &Bot,
    context: &AppContext,
    runtime: &XdlRuntime,
    message: &Message,
    args: &str,
) -> Result<()> {
    let Some(config) = context.config.bots.xdl.like_dl.as_ref() else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 未配置点赞监控，请在 <code>config.yaml</code> 中配置 <code>bots.xdl.like_dl</code>。",
        )
        .await?;
        return Ok(());
    };

    let action = args
        .split_whitespace()
        .next()
        .map(|value| value.to_ascii_lowercase());

    match action.as_deref() {
        Some("start") => {
            runtime
                .like_monitor
                .start_like(bot.clone(), context.clone())
                .await;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!(
                    "▶️ 点赞监控已启动\n\n轮询间隔：{} 秒\n目标群组：<code>{}</code>\n目标话题：<code>{}</code>",
                    config.poll_interval / 1000,
                    escape_html(&config.target_group),
                    config.target_topic
                ),
            )
            .await?;
        }
        Some("stop") => {
            runtime.like_monitor.stop().await;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "⏹️ 点赞监控已停止。",
            )
            .await?;
        }
        _ => {
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                format!(
                    "❤️ <b>点赞监控状态</b>\n\n状态：{}\n轮询间隔：{} 秒\n目标群组：<code>{}</code>\n目标话题：<code>{}</code>\n\n命令：\n<code>/tweet_like_dl start</code>\n<code>/tweet_like_dl stop</code>",
                    if runtime.like_monitor.is_running() {
                        "▶️ 运行中"
                    } else {
                        "⏹️ 已停止"
                    },
                    config.poll_interval / 1000,
                    escape_html(&config.target_group),
                    config.target_topic,
                ),
            )
            .await?;
        }
    }

    Ok(())
}
