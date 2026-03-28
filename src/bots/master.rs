mod backup;
mod commands;
mod restore;
mod runtime;

use anyhow::Result;
use teloxide::prelude::*;
use tracing::warn;

use self::backup::{backup_status_text, process_backup_password, start_backup_scheduler};
use self::commands::{handle_echo, handle_mdata, master_help, process_echo_reply};
use self::restore::{process_restore_document, process_restore_flow};
use self::runtime::{MasterRuntime, RestoreState, RestoreStep};
use crate::AppContext;
use crate::bots::common::{
    BotCommandDef, match_group_id, message_text, message_thread_id, parse_command, run_message_bot,
    send_html_message,
};

const COMMANDS: &[BotCommandDef] = &[
    BotCommandDef::new("start", "开始使用"),
    BotCommandDef::new("help", "显示帮助信息"),
    BotCommandDef::new("echo", "回显消息（10秒后自动删除）"),
    BotCommandDef::new("mdata", "查看消息 JSON 数据"),
    BotCommandDef::new("backup", "手动触发数据库备份"),
    BotCommandDef::new("backup_status", "查看备份服务状态"),
    BotCommandDef::new("restore", "从 ZIP 备份恢复数据库"),
    BotCommandDef::new("restore_cancel", "取消数据库恢复"),
    BotCommandDef::new("restore_nozip", "备份 ZIP 无密码"),
];

pub async fn run(context: AppContext) -> Result<()> {
    let startup_bot = Bot::new(context.config.bots.master.base.token.clone());

    if context.config.bots.master.backup.is_some() {
        tokio::spawn(start_backup_scheduler(startup_bot.clone(), context.clone()));
    }

    let runtime = MasterRuntime::new();
    let token = context.config.bots.master.base.token.clone();
    run_message_bot(
        &token,
        context,
        "master",
        COMMANDS,
        should_store_master_message,
        move |bot, context, msg| {
            let runtime = runtime.clone();
            async move { handle_message(&bot, &context, &runtime, &msg).await }
        },
    )
    .await
}

fn should_store_master_message(context: &AppContext, message: &Message) -> bool {
    let chat = &message.chat;
    if chat.is_private() {
        return true;
    }

    let mut excludes: Vec<(&str, i32)> = Vec::new();
    if let Some(tweetdl) = &context.config.bots.xdl.tweetdl {
        excludes.push((&tweetdl.listen_group, tweetdl.listen_topic));
    }
    if let Some(forward) = &context.config.bots.tdl.forward {
        excludes.push((&forward.listen_chat, forward.listen_thread));
    }

    for (group, topic) in excludes {
        if match_group_id(chat.id.0, group) {
            if topic <= 0 {
                return false;
            }
            if message_thread_id(message).unwrap_or_default() == topic {
                return false;
            }
        }
    }

    true
}

async fn handle_message(
    bot: &Bot,
    context: &AppContext,
    runtime: &MasterRuntime,
    message: &Message,
) -> Result<()> {
    if let Some(user) = &message.from {
        if let Some(text) = message_text(message) {
            if process_backup_password(bot, context, runtime, message, user.id.0, text).await? {
                return Ok(());
            }
            if process_restore_flow(bot, context, runtime, message, user.id.0, text).await? {
                return Ok(());
            }
        }
    }

    if process_echo_reply(bot, message).await? {
        return Ok(());
    }

    if message.document().is_some() {
        if let Some(user) = &message.from {
            if process_restore_document(bot, runtime, message, user.id.0).await? {
                return Ok(());
            }
        }
    }

    let Some(text) = message.text() else {
        return Ok(());
    };
    let Some((command, args)) = parse_command(text) else {
        return Ok(());
    };

    let Some(user) = &message.from else {
        return Ok(());
    };

    if !is_admin(context, user.id.0 as i64) {
        warn!(user_id = user.id.0, "unauthorized master command ignored");
        return Ok(());
    }

    match command {
        "start" => {
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "欢迎使用 Telegram Bot！<br/><br/>发送 /help 查看可用命令。",
            )
            .await?;
        }
        "help" => {
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                master_help(),
            )
            .await?;
        }
        "echo" => {
            handle_echo(bot, message, args).await?;
        }
        "mdata" => {
            handle_mdata(bot, message).await?;
        }
        "backup" => {
            runtime.pending_backups.lock().await.insert(user.id.0, ());
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "🔐 请直接回复备份密码以确认导出。",
            )
            .await?;
        }
        "backup_status" => {
            let text = backup_status_text(context);
            send_html_message(bot, message.chat.id, message_thread_id(message), text).await?;
        }
        "restore" => {
            if context
                .config
                .bots
                .master
                .backup
                .as_ref()
                .and_then(|cfg| cfg.import_password.as_ref())
                .is_none()
            {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 未配置导入密码，请在 config.yaml 中配置 bots.master.backup.import_password。",
                )
                .await?;
            } else {
                runtime.pending_restores.lock().await.insert(
                    user.id.0,
                    RestoreState {
                        step: RestoreStep::AwaitingZip,
                        zip_path: None,
                        zip_password: None,
                    },
                );
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "📁 第一步：请发送 ZIP 备份文件。",
                )
                .await?;
            }
        }
        "restore_cancel" => {
            runtime.pending_restores.lock().await.remove(&user.id.0);
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "✅ 已取消恢复操作。",
            )
            .await?;
        }
        "restore_nozip" => {
            let mut restores = runtime.pending_restores.lock().await;
            let Some(state) = restores.get_mut(&user.id.0) else {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 当前没有待处理的恢复请求。",
                )
                .await?;
                return Ok(());
            };
            if state.step != RestoreStep::AwaitingZipPassword {
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 当前不在 ZIP 密码步骤。",
                )
                .await?;
                return Ok(());
            }
            state.step = RestoreStep::AwaitingImportPassword;
            state.zip_password = None;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "✅ 已设置 ZIP 无密码。<br/><br/>🔐 请输入导入密码。",
            )
            .await?;
        }
        _ => {}
    }

    Ok(())
}

fn is_admin(context: &AppContext, user_id: i64) -> bool {
    context.config.bots.master.admins.contains(&user_id)
}
