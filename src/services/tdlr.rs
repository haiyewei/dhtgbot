use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::HttpServiceConfig;

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct TdlrClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct HttpExecuteRequest<'a> {
    args: &'a [String],
}

#[derive(Debug, Deserialize)]
struct HttpExecuteResponse {
    ok: bool,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    stdout: Option<String>,
    #[serde(default)]
    stderr: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl TdlrClient {
    pub fn new(config: &HttpServiceConfig) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .connect_timeout(Duration::from_millis(800))
                .timeout(Duration::from_secs(60 * 30))
                .build()
                .expect("tdlr http client should build"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        self.client
            .get(url)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn version(&self) -> Result<ProcessOutput> {
        self.execute(&[String::from("version")]).await
    }

    pub async fn forward(
        &self,
        link: &str,
        target_chat_id: &str,
        target_topic_id: Option<i32>,
        account: Option<&str>,
    ) -> Result<ProcessOutput> {
        let mut args = vec![
            String::from("forward"),
            String::from("-f"),
            link.to_string(),
            String::from("-t"),
            target_chat_id.to_string(),
        ];

        if let Some(account) = account {
            args.push(String::from("-a"));
            args.push(account.to_string());
        }

        if let Some(topic_id) = target_topic_id {
            if topic_id > 0 {
                args.push(String::from("--topic"));
                args.push(topic_id.to_string());
            }
        }

        self.execute(&args).await
    }

    pub async fn upload(
        &self,
        files: &[String],
        thumb_map: &[String],
        chat_id: &str,
        topic_id: Option<i32>,
        caption: Option<&str>,
        is_group: bool,
        rm: bool,
        account: Option<&str>,
    ) -> Result<ProcessOutput> {
        let args = build_upload_args(
            files, thumb_map, chat_id, topic_id, caption, is_group, rm, account,
        );

        self.execute(&args).await
    }

    async fn execute(&self, args: &[String]) -> Result<ProcessOutput> {
        let summary = summarize_args(args);
        info!(
            service = "tdlr",
            base_url = %self.base_url,
            command = summary.command,
            detail = %summary.detail,
            "sending tdlr request"
        );
        let started_at = Instant::now();
        let response = self
            .client
            .post(format!("{}/execute", self.base_url))
            .json(&HttpExecuteRequest { args })
            .send()
            .await
            .with_context(|| format!("failed to call tdlr service: {}/execute", self.base_url))?;

        let status = response.status();
        let text = response.text().await.with_context(|| {
            format!(
                "failed to read tdlr service response: {}/execute",
                self.base_url
            )
        })?;

        let payload: HttpExecuteResponse = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to decode tdlr service response (status {}): {}",
                status, text
            )
        })?;

        let stdout = payload.stdout.unwrap_or_default().trim().to_string();
        let stderr = payload
            .stderr
            .or(payload.error)
            .unwrap_or_default()
            .trim()
            .to_string();
        let code = payload
            .exit_code
            .unwrap_or_else(|| if payload.ok { 0 } else { 1 });

        if !status.is_success() && code == 0 {
            bail!("tdlr service returned HTTP {} with body: {}", status, text);
        }

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        if code == 0 {
            info!(
                service = "tdlr",
                base_url = %self.base_url,
                command = summary.command,
                detail = %summary.detail,
                http_status = %status,
                exit_code = code,
                elapsed_ms,
                "tdlr request completed"
            );
        } else {
            warn!(
                service = "tdlr",
                base_url = %self.base_url,
                command = summary.command,
                detail = %summary.detail,
                http_status = %status,
                exit_code = code,
                elapsed_ms,
                stderr = %stderr,
                "tdlr request returned non-zero exit code"
            );
        }

        Ok(ProcessOutput {
            code,
            stdout,
            stderr,
        })
    }
}

struct CommandSummary {
    command: String,
    detail: String,
}

fn summarize_args(args: &[String]) -> CommandSummary {
    let command = args
        .first()
        .cloned()
        .unwrap_or_else(|| String::from("unknown"));

    match command.as_str() {
        "upload" => {
            let file_count = count_flag_values(args, "-p");
            let thumb_map_count = count_flag_values(args, "--thumb-map");
            let chat_id = first_flag_value(args, "-c").unwrap_or("-");
            let topic = first_flag_value(args, "--topic").unwrap_or("-");
            let account = first_flag_value(args, "-a").unwrap_or("-");
            CommandSummary {
                command,
                detail: format!(
                    "files={file_count},thumb_map={thumb_map_count},chat_id={chat_id},topic={topic},account={account}"
                ),
            }
        }
        "forward" => {
            let link = first_flag_value(args, "-f").unwrap_or("-");
            let chat_id = first_flag_value(args, "-t").unwrap_or("-");
            let topic = first_flag_value(args, "--topic").unwrap_or("-");
            let account = first_flag_value(args, "-a").unwrap_or("-");
            CommandSummary {
                command,
                detail: format!("link={link},chat_id={chat_id},topic={topic},account={account}"),
            }
        }
        other => CommandSummary {
            command: other.to_string(),
            detail: format!("argc={}", args.len()),
        },
    }
}

fn count_flag_values(args: &[String], flag: &str) -> usize {
    args.windows(2)
        .filter(|pair| pair.first().map(|value| value.as_str()) == Some(flag))
        .count()
}

fn first_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair.first().map(|value| value.as_str()) == Some(flag))
        .and_then(|pair| pair.get(1).map(String::as_str))
}

fn build_upload_args(
    files: &[String],
    thumb_map: &[String],
    chat_id: &str,
    topic_id: Option<i32>,
    caption: Option<&str>,
    is_group: bool,
    rm: bool,
    account: Option<&str>,
) -> Vec<String> {
    let mut args = vec![String::from("upload")];

    for file in files {
        args.push(String::from("-p"));
        args.push(file.clone());
    }

    for mapping in thumb_map {
        args.push(String::from("--thumb-map"));
        args.push(mapping.clone());
    }

    args.push(String::from("-c"));
    args.push(chat_id.to_string());

    if let Some(topic_id) = topic_id {
        if topic_id > 0 {
            args.push(String::from("--topic"));
            args.push(topic_id.to_string());
        }
    }

    if let Some(caption) = caption {
        args.push(String::from("--caption"));
        args.push(caption.to_string());
    }

    if is_group {
        args.push(String::from("--group"));
    }

    if rm {
        args.push(String::from("--rm"));
    }

    if let Some(account) = account {
        args.push(String::from("-a"));
        args.push(account.to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::build_upload_args;

    #[test]
    fn build_upload_args_includes_thumb_map_entries() {
        let args = build_upload_args(
            &[String::from("a.mp4"), String::from("b.jpg")],
            &[String::from("a.mp4=a.jpg")],
            "-1001",
            Some(42),
            Some("caption"),
            true,
            false,
            Some("main"),
        );

        assert_eq!(
            args,
            vec![
                "upload",
                "-p",
                "a.mp4",
                "-p",
                "b.jpg",
                "--thumb-map",
                "a.mp4=a.jpg",
                "-c",
                "-1001",
                "--topic",
                "42",
                "--caption",
                "caption",
                "--group",
                "-a",
                "main",
            ]
        );
    }
}
