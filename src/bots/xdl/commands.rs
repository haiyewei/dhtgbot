use anyhow::Result;
use teloxide::prelude::*;

use crate::AppContext;
use crate::bots::common::{message_thread_id, parse_command, send_html_message};

use super::formatting::{tweetdl_help, xdl_help};
use super::query::{handle_profile, handle_search, handle_tweet, handle_tweets};
use super::runtime::XdlRuntime;
use super::schedule::maybe_schedule_tweetdl;
use super::tracking::{handle_author_track, handle_tweet_like_dl};

pub(super) async fn handle_message(
    bot: &Bot,
    context: &AppContext,
    runtime: &XdlRuntime,
    message: &Message,
) -> Result<()> {
    if let Some(text) = message.text() {
        if let Some((command, args)) = parse_command(text) {
            match command {
                "start" => {
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        "欢迎使用 XDL Bot！\n\n发送 /help 查看可用命令。",
                    )
                    .await?;
                    return Ok(());
                }
                "help" => {
                    send_html_message(bot, message.chat.id, message_thread_id(message), xdl_help())
                        .await?;
                    return Ok(());
                }
                "profile" => {
                    handle_profile(bot, context, message, args).await?;
                    return Ok(());
                }
                "tweet" => {
                    handle_tweet(bot, context, message, args).await?;
                    return Ok(());
                }
                "tweets" => {
                    handle_tweets(bot, context, message, args).await?;
                    return Ok(());
                }
                "search" => {
                    handle_search(bot, context, message, args).await?;
                    return Ok(());
                }
                "tweetdl" => {
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        tweetdl_help(),
                    )
                    .await?;
                    return Ok(());
                }
                "tweet_like_dl" => {
                    handle_tweet_like_dl(bot, context, runtime, message, args).await?;
                    return Ok(());
                }
                "author_track" => {
                    handle_author_track(bot, context, runtime, message, args).await?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    maybe_schedule_tweetdl(bot, context, message).await?;
    Ok(())
}
