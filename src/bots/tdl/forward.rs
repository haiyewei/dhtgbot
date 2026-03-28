use anyhow::Result;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::error;

use crate::AppContext;
use crate::bots::common::{
    forward_message_ids, latest_message_id, match_group_id, message_text, message_thread_id,
    parse_group_chat_id, send_html_message, thread_id_from_i32,
};
use crate::config::TdlForwardConfig;
use crate::services::task_queue::TaskLabel;

use super::parsing::{extract_telegram_links, generate_link_id};
use super::store::ForwardedMessage;

pub(super) async fn maybe_schedule_forward(
    bot: &Bot,
    context: &AppContext,
    message: &Message,
) -> Result<()> {
    let Some(config) = context.config.bots.tdl.forward.clone() else {
        return Ok(());
    };

    let Some(text) = message_text(message) else {
        return Ok(());
    };
    if text.trim().starts_with('/') || message.chat.is_private() {
        return Ok(());
    }
    if !match_group_id(message.chat.id.0, &config.listen_chat) {
        return Ok(());
    }
    if config.listen_thread > 0
        && message_thread_id(message).unwrap_or_default() != config.listen_thread
    {
        return Ok(());
    }

    let links = extract_telegram_links(text);
    if links.is_empty() {
        return Ok(());
    }

    for link in links {
        let link_id = generate_link_id(&link);
        let bot = bot.clone();
        let context = context.clone();
        let config = config.clone();
        let queue = context.work_queue.clone();
        queue.enqueue(TaskLabel::new("tdl", "forward", link_id), async move {
            if let Err(error) = execute_forward(&bot, &context, &config, &link).await {
                error!(?error, %link, "forward task failed");
            }
        });
    }

    Ok(())
}

async fn execute_forward(
    bot: &Bot,
    context: &AppContext,
    config: &TdlForwardConfig,
    link: &str,
) -> Result<()> {
    let store = context.store.bot("tdl");
    let kv = store.kv();

    let target_chat_id = parse_group_chat_id(&config.peer)?;
    let target_thread_id = (config.thread > 0).then_some(config.thread);
    let link_id = generate_link_id(link);
    let key = format!("forwarded:{link_id}");

    if let Some(record) = kv.get_json::<ForwardedMessage>(&key).await? {
        if forward_message_ids(
            bot,
            ChatId(record.target_chat_id),
            target_chat_id,
            target_thread_id,
            &record.message_ids,
        )
        .await
        .unwrap_or(false)
        {
            return Ok(());
        }
        let _ = kv.delete(&key).await;
    }

    let status = {
        let mut request = bot.send_message(target_chat_id, format!("⏬ 正在下载中...\n🔗 {link}"));
        if let Some(thread_id) = target_thread_id {
            request = request.message_thread_id(thread_id_from_i32(thread_id));
        }
        request.await?
    };

    let before_id = latest_message_id(bot, target_chat_id, target_thread_id)
        .await
        .ok()
        .flatten();
    let tdlr_result = context
        .tdlr
        .forward(
            link,
            &config.peer,
            target_thread_id,
            config.account.as_deref(),
        )
        .await;
    let _ = bot.delete_message(target_chat_id, status.id).await;
    let result = tdlr_result?;
    if result.code != 0 {
        send_forward_failure(bot, target_chat_id, target_thread_id, link, &result.stderr).await?;
        return Ok(());
    }

    let mut message_ids = Vec::new();
    if let Some(before_id) = before_id {
        if let Some(after_id) = latest_message_id(bot, target_chat_id, target_thread_id)
            .await
            .ok()
            .flatten()
        {
            let uploaded = after_id - before_id - 1;
            if uploaded > 0 {
                for offset in 1..=uploaded {
                    message_ids.push(before_id + offset);
                }
            }
        }
    }

    let record = ForwardedMessage {
        source_link: link.to_string(),
        link_id,
        target_chat_id: target_chat_id.0,
        target_thread_id,
        message_ids,
        forwarded_at: chrono::Local::now().to_rfc3339(),
    };
    kv.set_json(&key, &record).await?;

    let mut request = bot.send_message(target_chat_id, format!("✅ 下载完成\n🔗 {link}"));
    if let Some(thread_id) = target_thread_id {
        request = request.message_thread_id(thread_id_from_i32(thread_id));
    }
    request.await?;
    Ok(())
}

async fn send_forward_failure(
    bot: &Bot,
    target_chat_id: ChatId,
    target_thread_id: Option<i32>,
    link: &str,
    stderr: &str,
) -> Result<()> {
    let text = format!(
        "❌ 转发失败\n🔗 {link}\n\n<pre>{}</pre>",
        crate::bots::common::escape_html(stderr)
    );
    send_html_message(bot, target_chat_id, target_thread_id, text).await?;
    Ok(())
}
