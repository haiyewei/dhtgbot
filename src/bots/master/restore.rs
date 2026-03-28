use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use teloxide::net::Download;
use teloxide::prelude::*;
use tracing::error;

use crate::AppContext;
use crate::bots::common::{message_thread_id, send_html_message};
use crate::storage::import_sqlite_dump;

use super::runtime::{MasterRuntime, RestoreStep};

pub(super) async fn process_restore_document(
    bot: &Bot,
    runtime: &MasterRuntime,
    message: &Message,
    user_id: u64,
) -> Result<bool> {
    let mut restores = runtime.pending_restores.lock().await;
    let Some(state) = restores.get_mut(&user_id) else {
        return Ok(false);
    };
    if state.step != RestoreStep::AwaitingZip {
        return Ok(false);
    }

    let Some(document) = message.document() else {
        return Ok(false);
    };

    if !document
        .file_name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .ends_with(".zip")
    {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 请发送 ZIP 文件。",
        )
        .await?;
        return Ok(true);
    }

    let temp_dir = std::env::temp_dir().join("dhtgbot-rs-restore");
    tokio::fs::create_dir_all(&temp_dir).await?;
    let zip_path = temp_dir.join(format!("restore-{}-{}.zip", user_id, message.id.0));

    let file = bot.get_file(document.file.id.clone()).await?;
    let mut dst = tokio::fs::File::create(&zip_path).await?;
    bot.download_file(&file.path, &mut dst).await?;

    state.step = RestoreStep::AwaitingZipPassword;
    state.zip_path = Some(zip_path);

    send_html_message(
        bot,
        message.chat.id,
        message_thread_id(message),
        "✅ 文件已接收。<br/><br/>🔐 第二步：请输入 ZIP 密码；如果 ZIP 无密码，请发送 /restore_nozip。",
    )
    .await?;

    Ok(true)
}

pub(super) async fn process_restore_flow(
    bot: &Bot,
    context: &AppContext,
    runtime: &MasterRuntime,
    message: &Message,
    user_id: u64,
    text: &str,
) -> Result<bool> {
    let mut restores = runtime.pending_restores.lock().await;
    let Some(state) = restores.get_mut(&user_id) else {
        return Ok(false);
    };

    match state.step {
        RestoreStep::AwaitingZip => Ok(false),
        RestoreStep::AwaitingZipPassword => {
            let _ = bot.delete_message(message.chat.id, message.id).await;
            state.zip_password = Some(text.trim().to_string());
            state.step = RestoreStep::AwaitingImportPassword;
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "✅ ZIP 密码已记录。<br/><br/>🔐 第三步：请输入导入密码。",
            )
            .await?;
            Ok(true)
        }
        RestoreStep::AwaitingImportPassword => {
            let _ = bot.delete_message(message.chat.id, message.id).await;
            let Some(backup) = context.config.bots.master.backup.as_ref() else {
                restores.remove(&user_id);
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 未配置备份导入密码。",
                )
                .await?;
                return Ok(true);
            };

            if backup.import_password.as_deref().unwrap_or_default() != text.trim() {
                restores.remove(&user_id);
                send_html_message(
                    bot,
                    message.chat.id,
                    message_thread_id(message),
                    "❌ 导入密码错误，恢复已取消。",
                )
                .await?;
                return Ok(true);
            }

            let zip_path = state
                .zip_path
                .clone()
                .ok_or_else(|| anyhow!("missing restore zip path"))?;
            let zip_password = state.zip_password.clone();
            restores.remove(&user_id);
            drop(restores);

            let status = send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "⏳ 正在恢复数据库...",
            )
            .await?;

            let result = restore_from_zip(
                &context.config.sqlite_path(&context.root),
                &zip_path,
                zip_password,
            )
            .await;
            let _ = bot.delete_message(message.chat.id, status.id).await;

            match result {
                Ok(_) => {
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        "✅ 数据库恢复完成。",
                    )
                    .await?;
                }
                Err(error) => {
                    error!(?error, "restore failed");
                    send_html_message(
                        bot,
                        message.chat.id,
                        message_thread_id(message),
                        "❌ 数据库恢复失败，请查看日志。",
                    )
                    .await?;
                }
            }

            Ok(true)
        }
    }
}

async fn restore_from_zip(
    sqlite_path: &Path,
    zip_path: &Path,
    zip_password: Option<String>,
) -> Result<()> {
    let zip_path_buf = zip_path.to_path_buf();
    let restore_result = tokio::task::spawn_blocking(move || -> Result<String> {
        let file = File::open(&zip_path_buf)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for index in 0..archive.len() {
            let mut entry = match zip_password.as_deref() {
                Some(password) => archive.by_index_decrypt(index, password.as_bytes())?,
                None => archive.by_index(index)?,
            };
            if entry.name().ends_with(".sql") {
                let mut buffer = Vec::new();
                std::io::copy(&mut entry, &mut buffer)?;
                return String::from_utf8(buffer).context("backup sql was not utf-8");
            }
        }
        Err(anyhow!("zip archive does not contain a .sql file"))
    })
    .await?;
    let _ = tokio::fs::remove_file(zip_path).await;
    let sql_dump = restore_result?;

    import_sqlite_dump(sqlite_path, &sql_dump).await?;
    Ok(())
}
