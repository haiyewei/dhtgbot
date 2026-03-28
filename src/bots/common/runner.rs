use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use tracing::error;

use crate::AppContext;

use super::commands::{BotCommandDef, set_commands};
use super::{match_group_id, message_thread_id, save_message};

pub fn matches_chat_scope(message: &Message, group_id: &str, thread_id: i32) -> bool {
    if !match_group_id(message.chat.id.0, group_id) {
        return false;
    }

    if thread_id > 0 {
        return message_thread_id(message).unwrap_or_default() == thread_id;
    }

    true
}

pub async fn run_message_bot<ShouldStore, Handle, Fut>(
    token: &str,
    context: AppContext,
    storage_key: &'static str,
    commands: &[BotCommandDef],
    should_store: ShouldStore,
    handle: Handle,
) -> Result<()>
where
    ShouldStore: Fn(&AppContext, &Message) -> bool + Send + Sync + 'static,
    Handle: Fn(Bot, AppContext, Message) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let bot = Bot::new(token.to_string());
    set_commands(&bot, commands).await?;

    let should_store = Arc::new(should_store);
    let handle = Arc::new(handle);

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let context = context.clone();
        let should_store = Arc::clone(&should_store);
        let handle = Arc::clone(&handle);

        async move {
            if should_store(&context, &msg) {
                let _ = save_message(&context, storage_key, &msg).await;
            }

            if let Err(error) = handle(bot, context, msg).await {
                error!(?error, bot = storage_key, "bot handler failed");
            }

            respond(())
        }
    })
    .await;

    Ok(())
}
