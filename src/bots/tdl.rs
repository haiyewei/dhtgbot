mod forward;
mod help;
mod parsing;
mod store;

use anyhow::Result;
use teloxide::prelude::*;

use self::forward::maybe_schedule_forward;
use self::help::{forward_help, tdl_help};
use crate::AppContext;
use crate::bots::common::{
    BotCommandDef, escape_html, matches_chat_scope, message_thread_id, parse_command,
    run_message_bot, send_html_message,
};

const COMMANDS: &[BotCommandDef] = &[
    BotCommandDef::new("start", "开始使用"),
    BotCommandDef::new("help", "显示帮助信息"),
    BotCommandDef::new("version", "查看 TDLR 版本"),
    BotCommandDef::new("forward", "显示可转发的链接格式"),
];

pub async fn run(context: AppContext) -> Result<()> {
    let token = context.config.bots.tdl.base.token.clone();
    run_message_bot(
        &token,
        context,
        "tdl",
        COMMANDS,
        should_store_tdl_message,
        |bot, context, msg| async move { handle_message(&bot, &context, &msg).await },
    )
    .await
}

fn should_store_tdl_message(context: &AppContext, message: &Message) -> bool {
    if message.chat.is_private() {
        return true;
    }

    let Some(config) = context.config.bots.tdl.forward.as_ref() else {
        return false;
    };

    matches_chat_scope(message, &config.listen_chat, config.listen_thread)
}

async fn handle_message(bot: &Bot, context: &AppContext, message: &Message) -> Result<()> {
    if let Some(text) = message.text() {
        if let Some((command, args)) = parse_command(text) {
            match command {
                "start" => {
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        "欢迎使用 TDL Bot！<br/><br/>Telegram Downloader 管理工具<br/><br/>发送 /help 查看可用命令。",
                    )
                    .await?;
                    return Ok(());
                }
                "help" => {
                    send_html_message(bot, message.chat.id, message_thread_id(message), tdl_help())
                        .await?;
                    return Ok(());
                }
                "version" => {
                    let output = context.tdlr.version().await?;
                    let text = format!(
                        "TDLR 版本信息：<br/><br/><pre>{}</pre>",
                        escape_html(if output.stdout.is_empty() {
                            &output.stderr
                        } else {
                            &output.stdout
                        })
                    );
                    send_html_message(bot, message.chat.id, message_thread_id(message), text)
                        .await?;
                    return Ok(());
                }
                "forward" => {
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        forward_help(),
                    )
                    .await?;
                    return Ok(());
                }
                _ => {
                    let _ = args;
                }
            }
        }
    }

    maybe_schedule_forward(bot, context, message).await?;
    Ok(())
}
