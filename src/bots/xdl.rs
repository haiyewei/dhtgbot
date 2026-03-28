mod commands;
mod download;
mod formatting;
mod monitor;
mod parsing;
mod query;
mod runtime;
mod schedule;
mod store;
mod tracking;

use anyhow::Result;
use teloxide::prelude::*;

use self::commands::handle_message;
use self::runtime::XdlRuntime;
use crate::AppContext;
use crate::bots::common::{BotCommandDef, matches_chat_scope, run_message_bot};

const COMMANDS: &[BotCommandDef] = &[
    BotCommandDef::new("start", "开始使用"),
    BotCommandDef::new("help", "显示帮助信息"),
    BotCommandDef::new("profile", "获取 Twitter 用户资料"),
    BotCommandDef::new("tweet", "获取推文详情"),
    BotCommandDef::new("tweets", "获取用户推文"),
    BotCommandDef::new("search", "搜索推文"),
    BotCommandDef::new("tweetdl", "Twitter 媒体下载功能说明"),
    BotCommandDef::new("tweet_like_dl", "点赞推文自动下载监控"),
    BotCommandDef::new("author_track", "作者追踪管理"),
];

pub async fn run(context: AppContext) -> Result<()> {
    let startup_bot = Bot::new(context.config.bots.xdl.base.token.clone());
    let runtime = XdlRuntime::new();

    if context.config.bots.xdl.like_dl.is_some() {
        runtime
            .like_monitor
            .start_like(startup_bot.clone(), context.clone())
            .await;
    }

    if context.config.bots.xdl.author_track.is_some() {
        runtime
            .author_monitor
            .start_author(startup_bot.clone(), context.clone())
            .await;
    }

    let token = context.config.bots.xdl.base.token.clone();
    let runtime = runtime.clone();
    run_message_bot(
        &token,
        context,
        "xdl",
        COMMANDS,
        should_store_xdl_message,
        move |bot, context, msg| {
            let runtime = runtime.clone();
            async move { handle_message(&bot, &context, &runtime, &msg).await }
        },
    )
    .await
}

fn should_store_xdl_message(context: &AppContext, message: &Message) -> bool {
    if message.chat.is_private() {
        return true;
    }

    let Some(config) = context.config.bots.xdl.tweetdl.as_ref() else {
        return false;
    };

    matches_chat_scope(message, &config.listen_group, config.listen_topic)
}
