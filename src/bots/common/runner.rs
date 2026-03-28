use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use anyhow::Result;
use futures::FutureExt;
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

            match AssertUnwindSafe(handle(bot, context, msg))
                .catch_unwind()
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    error!(?error, bot = storage_key, "bot handler failed");
                }
                Err(panic_payload) => {
                    error!(
                        panic = panic_payload_to_string(&panic_payload),
                        bot = storage_key,
                        "bot handler panicked"
                    );
                }
            }

            respond(())
        }
    })
    .await;

    Ok(())
}

fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        return (*text).to_string();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "unknown panic payload".to_string()
}
