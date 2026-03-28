use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::{info, warn};

use crate::AppContext;
use crate::bots::common::{
    forward_message_ids, latest_message_id, normalize_telegram_html, parse_group_chat_id,
    send_html_message,
};
use crate::services::twitter_bridge::{Tweet, TweetMedia, TweetMediaType};

use super::formatting::format_tweet_html;
use super::parsing::{extension_from_url, generate_tweet_filename};
use super::store::DownloadedTweet;

const MAX_TDLR_RETRY_AFTER_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DownloadedMedia {
    file_path: PathBuf,
    thumb_path: Option<PathBuf>,
}

pub(super) async fn process_tweet_download(
    bot: Bot,
    context: AppContext,
    tweet_id: String,
    target_group: String,
    target_topic: i32,
    download_dir: Option<String>,
    source: &'static str,
) -> Result<()> {
    let kv = context.store.bot("xdl").kv();

    let target_chat_id = parse_group_chat_id(&target_group)?;
    let target_thread_id = (target_topic > 0).then_some(target_topic);
    let record_key = format!("downloaded:{tweet_id}");
    info!(
        bot = "xdl",
        source,
        tweet_id = %tweet_id,
        target_chat_id = target_chat_id.0,
        target_thread_id = ?target_thread_id,
        "tweet download task entered"
    );

    if let Some(record) = kv.get_json::<DownloadedTweet>(&record_key).await? {
        if record.chat_id == target_chat_id.0 && record.thread_id == target_thread_id {
            info!(
                bot = "xdl",
                source,
                tweet_id = %tweet_id,
                chat_id = record.chat_id,
                thread_id = ?record.thread_id,
                "tweet already downloaded for same target, skipping"
            );
            return Ok(());
        }

        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            from_chat_id = record.chat_id,
            from_thread_id = ?record.thread_id,
            to_chat_id = target_chat_id.0,
            to_thread_id = ?target_thread_id,
            message_count = record.message_ids.len(),
            "reusing existing downloaded tweet by forwarding saved messages"
        );
        match forward_message_ids(
            &bot,
            ChatId(record.chat_id),
            target_chat_id,
            target_thread_id,
            &record.message_ids,
        )
        .await
        {
            Ok(true) => return Ok(()),
            Ok(false) => {
                let _ = kv.delete(&record_key).await;
            }
            Err(error) => {
                warn!(?error, tweet_id = %tweet_id, "failed to forward existing tweet record");
                let _ = kv.delete(&record_key).await;
            }
        }
    }

    let resolved_download_dir =
        resolve_download_dir(context.root.as_ref(), download_dir.as_deref(), &tweet_id);
    info!(
        bot = "xdl",
        source,
        tweet_id = %tweet_id,
        download_dir = %resolved_download_dir.display(),
        "starting tweet download workflow"
    );

    let outcome = async {
        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            "fetching tweet metadata from twitter bridge"
        );
        let tweet = context
            .twitter_bridge
            .tweet(&tweet_id)
            .await?
            .with_context(|| format!("tweet not found: {tweet_id}"))?;
        let tweet_url = if tweet.url.is_empty() {
            format!("https://x.com/i/status/{tweet_id}")
        } else {
            tweet.url.clone()
        };
        let caption = format_tweet_html(&tweet);
        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            username = %tweet.username,
            media_count = tweet.media.len(),
            has_caption = !caption.is_empty(),
            "tweet metadata loaded"
        );
        let files = download_tweet_media(&context, &tweet, &resolved_download_dir).await?;
        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            downloaded_media = files.len(),
            "tweet media downloaded"
        );
        let message_ids = if files.is_empty() {
            send_tweet_text_message(&bot, target_chat_id, target_thread_id, &caption).await?
        } else {
            upload_media_via_tdlr(
                &bot,
                &context,
                target_chat_id,
                target_thread_id,
                &files,
                &caption,
            )
            .await?
        };
        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            uploaded_message_ids = message_ids.len(),
            "tdlr upload stage completed"
        );

        let record = DownloadedTweet {
            tweet_id: tweet.id.clone(),
            tweet_url: tweet_url.clone(),
            username: tweet.username.clone(),
            chat_id: target_chat_id.0,
            thread_id: target_thread_id,
            message_ids,
            source: source.to_string(),
            downloaded_at: chrono::Local::now().to_rfc3339(),
        };
        kv.set_json(&record_key, &record).await?;
        info!(
            bot = "xdl",
            source,
            tweet_id = %tweet_id,
            "tweet download record stored"
        );

        Ok::<String, anyhow::Error>(tweet_url)
    }
    .await;

    cleanup_download_dir(&resolved_download_dir);

    match outcome {
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn upload_media_via_tdlr(
    bot: &Bot,
    context: &AppContext,
    target_chat_id: ChatId,
    target_thread_id: Option<i32>,
    files: &[DownloadedMedia],
    caption: &str,
) -> Result<Vec<i32>> {
    let file_args = files
        .iter()
        .map(|media| media.file_path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let thumb_map_args = build_thumb_map_args(files);
    let before_id = latest_message_id(bot, target_chat_id, target_thread_id)
        .await
        .ok()
        .flatten();
    info!(
        bot = "xdl",
        target_chat_id = target_chat_id.0,
        target_thread_id = ?target_thread_id,
        file_count = file_args.len(),
        thumb_map_count = thumb_map_args.len(),
        "sending media to tdlr upload"
    );
    upload_via_tdlr_with_retry(
        context,
        &file_args,
        &thumb_map_args,
        &target_chat_id.0.to_string(),
        target_thread_id,
        caption,
    )
    .await?;

    detect_uploaded_message_ids(bot, target_chat_id, target_thread_id, before_id).await
}

async fn send_tweet_text_message(
    bot: &Bot,
    target_chat_id: ChatId,
    target_thread_id: Option<i32>,
    caption: &str,
) -> Result<Vec<i32>> {
    info!(
        bot = "xdl",
        target_chat_id = target_chat_id.0,
        target_thread_id = ?target_thread_id,
        "tweet contains no media files, sending html message directly"
    );
    let sent =
        send_html_message(bot, target_chat_id, target_thread_id, caption.to_string()).await?;
    Ok(vec![sent.id.0])
}

async fn detect_uploaded_message_ids(
    bot: &Bot,
    target_chat_id: ChatId,
    target_thread_id: Option<i32>,
    before_id: Option<i32>,
) -> Result<Vec<i32>> {
    let Some(before_id) = before_id else {
        return Ok(Vec::new());
    };
    let after_id = latest_message_id(bot, target_chat_id, target_thread_id)
        .await?
        .unwrap_or(before_id);
    Ok(compute_uploaded_message_ids(before_id, after_id))
}

fn compute_uploaded_message_ids(before_id: i32, after_id: i32) -> Vec<i32> {
    let uploaded = after_id.saturating_sub(before_id).saturating_sub(1);
    (1..=uploaded)
        .map(|offset| before_id + offset)
        .collect::<Vec<_>>()
}

async fn upload_via_tdlr_with_retry(
    context: &AppContext,
    file_args: &[String],
    thumb_map_args: &[String],
    chat_id: &str,
    topic_id: Option<i32>,
    caption: &str,
) -> Result<crate::services::tdlr::ProcessOutput> {
    let normalized_caption = normalize_telegram_html(caption);

    for attempt in 0..=MAX_TDLR_RETRY_AFTER_ATTEMPTS {
        let output = context
            .tdlr
            .upload(
                file_args,
                thumb_map_args,
                chat_id,
                topic_id,
                Some(&normalized_caption),
                file_args.len() > 1,
                false,
                context.config.bots.xdl.account.as_deref(),
            )
            .await?;

        if output.code == 0 {
            return Ok(output);
        }

        let detail = if output.stderr.is_empty() {
            output.stdout.clone()
        } else {
            output.stderr.clone()
        };

        let Some(wait_seconds) = parse_retry_after_seconds(&detail) else {
            bail!("tdlr upload failed: {detail}");
        };

        if attempt == MAX_TDLR_RETRY_AFTER_ATTEMPTS {
            bail!("tdlr upload failed after retrying flood wait: {detail}");
        }

        warn!(
            wait_seconds,
            attempt = attempt + 1,
            max_attempts = MAX_TDLR_RETRY_AFTER_ATTEMPTS + 1,
            "tdlr upload hit flood wait, retrying after delay"
        );
        tokio::time::sleep(Duration::from_secs(wait_seconds.saturating_add(1))).await;
    }

    unreachable!("retry loop should always return or bail")
}

async fn download_tweet_media(
    context: &AppContext,
    tweet: &Tweet,
    download_dir: &Path,
) -> Result<Vec<DownloadedMedia>> {
    fs::create_dir_all(download_dir)
        .with_context(|| format!("failed to create download dir: {}", download_dir.display()))?;

    let mut files = Vec::new();
    for (index, media) in tweet.media.iter().enumerate() {
        let (filename, thumb_filename) = media_filenames(tweet, index, media);
        let output_dir = download_dir.to_string_lossy().to_string();
        info!(
            bot = "xdl",
            tweet_id = %tweet.id,
            username = %tweet.username,
            media_index = index,
            media_type = ?media.r#type,
            filename,
            thumb_filename = ?thumb_filename,
            "downloading tweet media asset"
        );
        context
            .aria2
            .download(&media.url, &output_dir, &filename, 8)
            .await?;

        let file_path = download_dir.join(&filename);
        if !file_path.exists() {
            bail!("aria2 下载完成但文件不存在: {}", file_path.display());
        }

        let thumb_path = if let Some(thumb_filename) = thumb_filename {
            let thumb_url = media
                .thumbnail_url
                .as_deref()
                .context("video thumbnail filename exists without thumbnail url")?;
            info!(
                bot = "xdl",
                tweet_id = %tweet.id,
                media_index = index,
                thumb_filename,
                "downloading tweet thumbnail asset"
            );
            context
                .aria2
                .download(thumb_url, &output_dir, &thumb_filename, 8)
                .await?;

            let thumb_path = download_dir.join(&thumb_filename);
            if !thumb_path.exists() {
                bail!("aria2 下载完成但封面文件不存在: {}", thumb_path.display());
            }
            Some(thumb_path)
        } else {
            None
        };

        files.push(DownloadedMedia {
            file_path,
            thumb_path,
        });
    }

    Ok(files)
}

fn media_filenames(tweet: &Tweet, index: usize, media: &TweetMedia) -> (String, Option<String>) {
    let default_ext = match media.r#type {
        TweetMediaType::Photo => ".jpg",
        TweetMediaType::Video | TweetMediaType::Gif => ".mp4",
    };
    let ext = extension_from_url(&media.url, default_ext);
    let filename = generate_tweet_filename(tweet, index, &ext);
    let thumb_filename = media
        .thumbnail_url
        .as_ref()
        .filter(|_| matches!(media.r#type, TweetMediaType::Video | TweetMediaType::Gif))
        .map(|url| generate_tweet_filename(tweet, index, &extension_from_url(url, ".jpg")));

    (filename, thumb_filename)
}

fn build_thumb_map_args(files: &[DownloadedMedia]) -> Vec<String> {
    files
        .iter()
        .filter_map(|media| {
            media.thumb_path.as_ref().map(|thumb_path| {
                format!(
                    "{}={}",
                    media.file_path.to_string_lossy(),
                    thumb_path.to_string_lossy()
                )
            })
        })
        .collect()
}

fn resolve_download_dir(root: &Path, configured: Option<&str>, tweet_id: &str) -> PathBuf {
    let base = configured
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join("data").join("downloads"));
    base.join(tweet_id)
}

fn cleanup_download_dir(download_dir: &Path) {
    if let Ok(entries) = fs::read_dir(download_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(&path);
            } else if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }

    let _ = fs::remove_dir(download_dir);
}

fn parse_retry_after_seconds(text: &str) -> Option<u64> {
    let normalized = text.trim();
    let marker = "retry after ";
    let start = normalized.to_ascii_lowercase().find(marker)? + marker.len();
    let digits = normalized[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty())
        .then(|| digits.parse::<u64>().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        DownloadedMedia, build_thumb_map_args, compute_uploaded_message_ids, media_filenames,
        parse_retry_after_seconds,
    };
    use crate::services::twitter_bridge::{Tweet, TweetMedia, TweetMediaType};

    fn sample_tweet() -> Tweet {
        Tweet {
            id: "123".to_string(),
            text: "hello world".to_string(),
            created_at: String::new(),
            url: "https://x.com/alice/status/123".to_string(),
            lang: "en".to_string(),
            user_id: "u1".to_string(),
            username: "alice".to_string(),
            name: "Alice".to_string(),
            likes: 0,
            retweets: 0,
            replies: 0,
            views: 0,
            quotes: 0,
            bookmarks: 0,
            is_retweet: false,
            is_quote: false,
            is_reply: false,
            hashtags: Vec::new(),
            mentions: Vec::new(),
            urls: Vec::new(),
            media: Vec::new(),
            quoted_tweet: None,
            retweeted_tweet: None,
            reply_to_id: None,
        }
    }

    #[test]
    fn parses_retry_after_seconds_from_tdlr_error() {
        assert_eq!(parse_retry_after_seconds("Retry after 22s"), Some(22));
        assert_eq!(parse_retry_after_seconds("retry after 7"), Some(7));
        assert_eq!(parse_retry_after_seconds("other error"), None);
    }

    #[test]
    fn media_filenames_keep_same_index_for_video_and_thumb() {
        let media = TweetMedia {
            r#type: TweetMediaType::Video,
            url: "https://video.twimg.com/media/test.mp4?tag=12".to_string(),
            thumbnail_url: Some("https://pbs.twimg.com/media/test.jpg?name=small".to_string()),
        };

        let (video, thumb) = media_filenames(&sample_tweet(), 1, &media);
        let thumb = thumb.expect("video should have thumb filename");

        assert_eq!(Path::new(&video).file_stem(), Path::new(&thumb).file_stem());
        assert_eq!(
            Path::new(&video).extension().and_then(|ext| ext.to_str()),
            Some("mp4")
        );
        assert_eq!(
            Path::new(&thumb).extension().and_then(|ext| ext.to_str()),
            Some("jpg")
        );
    }

    #[test]
    fn build_thumb_map_args_only_includes_video_mappings() {
        let args = build_thumb_map_args(&[
            DownloadedMedia {
                file_path: PathBuf::from("tweet_0.mp4"),
                thumb_path: Some(PathBuf::from("tweet_0.jpg")),
            },
            DownloadedMedia {
                file_path: PathBuf::from("tweet_1.jpg"),
                thumb_path: None,
            },
            DownloadedMedia {
                file_path: PathBuf::from("tweet_2.mp4"),
                thumb_path: Some(PathBuf::from("tweet_2.png")),
            },
        ]);

        assert_eq!(
            args,
            vec![
                String::from("tweet_0.mp4=tweet_0.jpg"),
                String::from("tweet_2.mp4=tweet_2.png"),
            ]
        );
    }

    #[test]
    fn computes_uploaded_message_ids_excluding_probe_messages() {
        assert_eq!(compute_uploaded_message_ids(100, 102), vec![101]);
        assert_eq!(compute_uploaded_message_ids(100, 104), vec![101, 102, 103]);
        assert!(compute_uploaded_message_ids(100, 101).is_empty());
    }
}
