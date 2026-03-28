use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::Timelike;
use teloxide::prelude::*;
use tracing::error;
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

use crate::AppContext;
use crate::bots::common::{
    latest_message_id, message_thread_id, parse_group_chat_id, send_html_message,
};
use crate::storage::dump_sqlite_database;

use super::runtime::MasterRuntime;

#[derive(Debug, Clone)]
struct BackupRecord {
    message_id: i32,
    chat_id: i64,
    created_at: String,
}

#[derive(Debug, Default)]
struct BackupState {
    last_backup: Option<BackupRecord>,
    last_auto_backup: Option<BackupRecord>,
}

fn backup_state() -> &'static Mutex<BackupState> {
    static STATE: OnceLock<Mutex<BackupState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(BackupState::default()))
}

pub(super) async fn process_backup_password(
    bot: &Bot,
    context: &AppContext,
    runtime: &MasterRuntime,
    message: &Message,
    user_id: u64,
    text: &str,
) -> Result<bool> {
    if !runtime.pending_backups.lock().await.contains_key(&user_id) {
        return Ok(false);
    }

    runtime.pending_backups.lock().await.remove(&user_id);
    let _ = bot.delete_message(message.chat.id, message.id).await;
    let Some(backup) = context.config.bots.master.backup.as_ref() else {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 未配置备份。",
        )
        .await?;
        return Ok(true);
    };

    if text.trim() != backup.password {
        send_html_message(
            bot,
            message.chat.id,
            message_thread_id(message),
            "❌ 密码错误，备份已取消。",
        )
        .await?;
        return Ok(true);
    }

    let status = send_html_message(
        bot,
        message.chat.id,
        message_thread_id(message),
        "⏳ 密码验证成功，正在执行数据库备份...",
    )
    .await?;

    let result = create_and_send_backup(bot, context, "手动").await;
    let _ = bot.delete_message(message.chat.id, status.id).await;

    match result {
        Ok(_) => {
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "✅ 数据库备份完成。",
            )
            .await?;
        }
        Err(error) => {
            error!(?error, "manual backup failed");
            send_html_message(
                bot,
                message.chat.id,
                message_thread_id(message),
                "❌ 数据库备份失败，请查看日志。",
            )
            .await?;
        }
    }

    Ok(true)
}

pub(super) fn backup_status_text(context: &AppContext) -> String {
    let Some(backup) = context.config.bots.master.backup.as_ref() else {
        return "📊 备份服务状态：<br/>⚠️ 未配置备份".to_string();
    };

    let last_backup = backup_state()
        .lock()
        .ok()
        .and_then(|state| state.last_backup.clone());

    let mut lines = vec![
        String::from("📊 备份服务状态："),
        String::from("运行中: ✅"),
        format!("目标群组: {}", backup.target_group),
        format!(
            "目标话题: {}",
            if backup.target_topic > 0 {
                backup.target_topic.to_string()
            } else {
                "主聊天".to_string()
            }
        ),
    ];

    if let Some(record) = last_backup {
        lines.push(format!("上次备份: {}", record.created_at));
        lines.push(format!("消息 ID: {}", record.message_id));
    } else {
        lines.push(String::from("上次备份: 无"));
    }

    lines.push(format!("下次备份: {}", next_half_hour_display()));
    lines.join("<br/>")
}

pub(super) async fn start_backup_scheduler(bot: Bot, context: AppContext) {
    loop {
        let delay = delay_to_next_half_hour();
        tokio::time::sleep(delay).await;
        if let Err(error) = create_and_send_backup(&bot, &context, "自动").await {
            error!(?error, "scheduled backup failed");
        }
    }
}

fn delay_to_next_half_hour() -> Duration {
    let now = chrono::Local::now();
    let mut next = now;
    if next.minute() < 30 {
        next = next
            .with_minute(30)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .unwrap_or(now + chrono::Duration::minutes(30));
    } else {
        next = (next + chrono::Duration::hours(1))
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .unwrap_or(now + chrono::Duration::minutes(30));
    }

    (next - now)
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(60))
}

fn next_half_hour_display() -> String {
    let now = chrono::Local::now();
    let delay = delay_to_next_half_hour();
    (now + chrono::Duration::from_std(delay).unwrap_or_default())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

async fn create_and_send_backup(bot: &Bot, context: &AppContext, mode: &str) -> Result<()> {
    let backup = context
        .config
        .bots
        .master
        .backup
        .as_ref()
        .ok_or_else(|| anyhow!("backup config missing"))?;

    if mode == "自动" {
        delete_last_auto_backup(bot).await;
    }

    let temp_dir = std::env::temp_dir().join("dhtgbot-rs-backup");
    tokio::fs::create_dir_all(&temp_dir).await?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let zip_path = temp_dir.join(format!("backup-{timestamp}.zip"));
    let sqlite_path = context.config.sqlite_path(&context.root);
    build_backup_archive(sqlite_path, zip_path.clone(), backup.password.clone()).await?;

    let target_chat_id = parse_group_chat_id(&backup.target_group)?;
    let target_thread_id = (backup.target_topic > 0).then_some(backup.target_topic);
    let upload_chat_id = target_chat_id.0.to_string();
    let caption = format!(
        "📦 数据库备份（{mode}）\n📅 {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let before_id = latest_message_id(bot, target_chat_id, target_thread_id)
        .await
        .ok()
        .flatten();
    let file_args = vec![zip_path.to_string_lossy().to_string()];
    let send_result = context
        .tdlr
        .upload(
            &file_args,
            &[],
            &upload_chat_id,
            target_thread_id,
            Some(&caption),
            false,
            false,
            None,
        )
        .await;

    let _ = tokio::fs::remove_file(zip_path).await;
    let output = send_result?;
    if output.code != 0 {
        let detail = if output.stderr.is_empty() {
            output.stdout
        } else {
            output.stderr
        };
        return Err(anyhow!("tdlr backup upload failed: {detail}"));
    }

    let sent_message_id =
        detect_backup_message_id(bot, target_chat_id, target_thread_id, before_id).await?;
    remember_backup_record(mode, target_chat_id.0, sent_message_id);
    Ok(())
}

async fn build_backup_archive(
    sqlite_path: PathBuf,
    zip_path: PathBuf,
    password: String,
) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let sql_dump = dump_sqlite_database(&sqlite_path)?;
        write_zip_archive(&zip_path, "backup.sql", sql_dump.as_bytes(), &password)?;
        Ok(())
    })
    .await??;

    Ok(())
}

async fn detect_backup_message_id(
    bot: &Bot,
    chat_id: ChatId,
    thread_id: Option<i32>,
    before_id: Option<i32>,
) -> Result<i32> {
    let after_id = latest_message_id(bot, chat_id, thread_id)
        .await?
        .ok_or_else(|| anyhow!("failed to detect backup upload message id"))?;

    compute_backup_message_id(before_id, after_id)
        .ok_or_else(|| anyhow!("failed to compute backup upload message id"))
}

fn compute_backup_message_id(before_id: Option<i32>, after_id: i32) -> Option<i32> {
    if let Some(before_id) = before_id {
        if after_id > before_id + 1 {
            return Some(after_id - 1);
        }
        return None;
    }

    (after_id > 1).then_some(after_id - 1)
}

fn write_zip_archive(path: &Path, filename: &str, contents: &[u8], password: &str) -> Result<()> {
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .with_aes_encryption(AesMode::Aes256, password);
    zip.start_file(filename, options)?;
    zip.write_all(contents)?;
    zip.finish()?;
    Ok(())
}

async fn delete_last_auto_backup(bot: &Bot) {
    let record = backup_state()
        .lock()
        .ok()
        .and_then(|state| state.last_auto_backup.clone());
    let Some(record) = record else {
        return;
    };

    let _ = bot
        .delete_message(
            ChatId(record.chat_id),
            teloxide::types::MessageId(record.message_id),
        )
        .await;

    if let Ok(mut state) = backup_state().lock() {
        if state
            .last_auto_backup
            .as_ref()
            .map(|current| {
                current.chat_id == record.chat_id && current.message_id == record.message_id
            })
            .unwrap_or(false)
        {
            state.last_auto_backup = None;
        }
    }
}

fn remember_backup_record(mode: &str, chat_id: i64, message_id: i32) {
    let record = BackupRecord {
        message_id,
        chat_id,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    if let Ok(mut state) = backup_state().lock() {
        state.last_backup = Some(record.clone());
        if mode == "自动" {
            state.last_auto_backup = Some(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;

    use super::{compute_backup_message_id, write_zip_archive};

    #[test]
    fn writes_password_protected_backup_zip() {
        let zip_path = std::env::temp_dir().join(format!(
            "dhtgbot-backup-test-{}-{}.zip",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));

        write_zip_archive(&zip_path, "backup.sql", b"SELECT 1;", "secret").unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut entry = archive.by_index_decrypt(0, b"secret").unwrap();
        let mut content = String::new();
        entry.read_to_string(&mut content).unwrap();

        assert_eq!(content, "SELECT 1;");
        let _ = std::fs::remove_file(zip_path);
    }

    #[test]
    fn computes_backup_message_id_from_probe_messages() {
        assert_eq!(compute_backup_message_id(Some(100), 102), Some(101));
        assert_eq!(compute_backup_message_id(Some(100), 101), None);
        assert_eq!(compute_backup_message_id(None, 77), Some(76));
        assert_eq!(compute_backup_message_id(None, 1), None);
    }
}
