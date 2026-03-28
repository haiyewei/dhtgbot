use anyhow::Result;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, Message, MessageId, ParseMode, ThreadId};

use crate::AppContext;
use crate::storage::StoredMessage;

pub fn normalize_group_id(group_id: &str) -> String {
    if group_id.starts_with('-') {
        group_id.to_string()
    } else {
        format!("-100{group_id}")
    }
}

pub fn match_group_id(chat_id: i64, config_group_id: &str) -> bool {
    let normalized = normalize_group_id(config_group_id);
    chat_id.to_string() == normalized || chat_id.to_string() == config_group_id
}

pub fn parse_command(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];
    let mut parts = without_slash.splitn(2, char::is_whitespace);
    let name = parts.next()?.split('@').next()?;
    let args = parts.next().unwrap_or("").trim();
    Some((name, args))
}

pub fn message_thread_id(message: &Message) -> Option<i32> {
    message.thread_id.as_ref().map(|id| id.0.0)
}

pub fn message_text(message: &Message) -> Option<&str> {
    message.text().or_else(|| message.caption())
}

pub fn thread_id_from_i32(value: i32) -> ThreadId {
    ThreadId(MessageId(value))
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn normalize_telegram_html(text: &str) -> String {
    text.replace("<br/>", "\n").replace("<br>", "\n")
}

pub fn extract_stored_message(message: &Message) -> StoredMessage {
    let kind = if message.text().is_some() {
        "text"
    } else if message.photo().is_some() {
        "photo"
    } else if message.video().is_some() {
        "video"
    } else if message.document().is_some() {
        "document"
    } else {
        "other"
    };

    StoredMessage {
        message_id: message.id.0,
        chat_id: message.chat.id.0,
        from_id: message.from.as_ref().map(|user| user.id.0 as i64),
        from_username: message.from.as_ref().and_then(|user| user.username.clone()),
        is_bot: message
            .from
            .as_ref()
            .map(|user| user.is_bot)
            .unwrap_or(false),
        date: message.date.timestamp(),
        text: message_text(message).map(ToOwned::to_owned),
        kind: kind.to_string(),
    }
}

pub async fn save_message(context: &AppContext, bot_name: &str, message: &Message) -> Result<()> {
    let store = context.store.bot(bot_name);
    let value = extract_stored_message(message);
    store
        .message()
        .set_json(message.chat.id.0, message.id.0, &value)
        .await
}

pub async fn send_html_message(
    bot: &teloxide::Bot,
    chat_id: ChatId,
    thread_id: Option<i32>,
    text: impl Into<String>,
) -> Result<Message> {
    let text = normalize_telegram_html(&text.into());
    let mut request = bot.send_message(chat_id, text).parse_mode(ParseMode::Html);
    if let Some(thread_id) = thread_id {
        request = request.message_thread_id(thread_id_from_i32(thread_id));
    }
    Ok(request.await?)
}

#[cfg(test)]
mod tests {
    use super::normalize_telegram_html;

    #[test]
    fn normalizes_html_break_tags_for_telegram() {
        assert_eq!(
            normalize_telegram_html("line1<br/>line2<br>line3"),
            "line1\nline2\nline3"
        );
    }
}
