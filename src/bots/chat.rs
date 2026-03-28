use anyhow::{Context, Result};
use teloxide::payloads::{ForwardMessageSetters, ForwardMessagesSetters, SendMessageSetters};
use teloxide::prelude::{Bot, Requester};
use teloxide::types::{ChatId, MessageId};

use crate::bots::shared::{normalize_group_id, thread_id_from_i32};

pub fn parse_group_chat_id(group_id: &str) -> Result<ChatId> {
    let normalized = normalize_group_id(group_id);
    let parsed = normalized
        .parse::<i64>()
        .with_context(|| format!("invalid chat id: {group_id}"))?;
    Ok(ChatId(parsed))
}

pub async fn latest_message_id(
    bot: &Bot,
    chat_id: ChatId,
    thread_id: Option<i32>,
) -> Result<Option<i32>> {
    let mut request = bot.send_message(chat_id, "⏳");
    if let Some(thread_id) = thread_id {
        request = request.message_thread_id(thread_id_from_i32(thread_id));
    }
    let sent = request.await?;
    let _ = bot.delete_message(chat_id, sent.id).await;
    Ok(Some(sent.id.0))
}

pub async fn forward_message_ids(
    bot: &Bot,
    source_chat_id: ChatId,
    target_chat_id: ChatId,
    target_thread_id: Option<i32>,
    message_ids: &[i32],
) -> Result<bool> {
    if message_ids.is_empty() {
        return Ok(false);
    }

    let message_ids = message_ids
        .iter()
        .copied()
        .map(MessageId)
        .collect::<Vec<_>>();

    if message_ids.len() == 1 {
        let mut request = bot.forward_message(target_chat_id, source_chat_id, message_ids[0]);
        if let Some(thread_id) = target_thread_id {
            request = request.message_thread_id(thread_id_from_i32(thread_id));
        }
        request.await?;
    } else {
        let mut request = bot.forward_messages(target_chat_id, source_chat_id, message_ids);
        if let Some(thread_id) = target_thread_id {
            request = request.message_thread_id(thread_id_from_i32(thread_id));
        }
        request.await?;
    }

    Ok(true)
}
