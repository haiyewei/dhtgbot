use std::time::Duration;

use anyhow::Result;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::*;
use teloxide::types::{ForceReply, ReplyMarkup};

use crate::bots::common::escape_html;

const ECHO_PROMPT_TEXT: &str = "请输入要回显的消息：";

pub(super) fn master_help() -> String {
    [
        "可用命令：",
        "/start - 开始使用",
        "/help - 显示帮助信息",
        "/echo <文本> - 回显消息（10秒后自动删除）",
        "/mdata - 查看当前消息或回复消息的 JSON",
        "/backup - 手动触发数据库备份",
        "/backup_status - 查看备份服务状态",
        "/restore - 从 ZIP 备份恢复数据库",
        "/restore_cancel - 取消恢复流程",
        "/restore_nozip - ZIP 无密码时跳过 ZIP 密码步骤",
    ]
    .join("<br/>")
}

pub(super) async fn handle_echo(bot: &Bot, message: &Message, args: &str) -> Result<()> {
    if args.trim().is_empty() {
        let _ = bot.delete_message(message.chat.id, message.id).await;
        bot.send_message(message.chat.id, ECHO_PROMPT_TEXT)
            .reply_markup(ReplyMarkup::ForceReply(
                ForceReply::new()
                    .input_field_placeholder(Some(String::from("输入消息...")))
                    .selective(),
            ))
            .await?;
        return Ok(());
    }

    let sent = bot.send_message(message.chat.id, args.to_string()).await?;
    let bot_clone = bot.clone();
    let chat_id = message.chat.id;
    let user_message_id = message.id;
    let sent_message_id = sent.id;

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let _ = bot_clone.delete_message(chat_id, user_message_id).await;
        let _ = bot_clone.delete_message(chat_id, sent_message_id).await;
    });
    Ok(())
}

pub(super) async fn process_echo_reply(bot: &Bot, message: &Message) -> Result<bool> {
    let Some(reply_to) = message.reply_to_message() else {
        return Ok(false);
    };
    let Some(reply_text) = reply_to.text() else {
        return Ok(false);
    };
    if reply_text != ECHO_PROMPT_TEXT {
        return Ok(false);
    }
    if !reply_to
        .from
        .as_ref()
        .map(|user| user.is_bot)
        .unwrap_or(false)
    {
        return Ok(false);
    }

    let Some(text) = message.text() else {
        return Ok(false);
    };

    let sent = bot.send_message(message.chat.id, text.to_string()).await?;
    let bot_clone = bot.clone();
    let chat_id = message.chat.id;
    let prompt_message_id = reply_to.id;
    let user_message_id = message.id;
    let sent_message_id = sent.id;

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let _ = bot_clone.delete_message(chat_id, prompt_message_id).await;
        let _ = bot_clone.delete_message(chat_id, user_message_id).await;
        let _ = bot_clone.delete_message(chat_id, sent_message_id).await;
    });

    Ok(true)
}

pub(super) async fn handle_mdata(bot: &Bot, message: &Message) -> Result<()> {
    let target = message.reply_to_message().unwrap_or(message);
    let json = serde_json::to_string_pretty(target)?;
    for chunk in json.as_bytes().chunks(3800) {
        let html = format!(
            "<pre>{}</pre>",
            escape_html(&String::from_utf8_lossy(chunk))
        );
        bot.send_message(message.chat.id, html)
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
    }
    Ok(())
}
